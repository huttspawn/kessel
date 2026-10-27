//! World/spatial extraction: appearance, fx, scripts, nodes, planets, spawns, conversation refs.

use super::*;

impl Database {
    /// Populate `appearance_specs` from every EPP file in the archives
    /// (#183), gated on the EPP-content hashes the main sweep discovered (#292).
    ///
    /// Each row carries the FQN extracted from the XML root attribute,
    /// JSON-encoded lists of distinct AppearanceAction types and fxSpec
    /// refs found in the body, and the raw decoded XML. Per-file decode
    /// failures are skipped silently rather than aborting the walk.
    pub fn populate_appearance_specs(
        &self,
        tor_dir: &std::path::Path,
        epp_hashes: &std::collections::HashSet<u64>,
    ) -> Result<u64> {
        use crate::myp::Archive;
        use crate::schema::epp;

        let tor_files: Vec<std::path::PathBuf> = std::fs::read_dir(tor_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "tor").unwrap_or(false))
            .collect();

        let mut written = 0u64;
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO appearance_specs \
               (fqn, appearance_actions, fx_spec_refs, raw_xml) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for tor_path in &tor_files {
            let mut archive = match Archive::open(tor_path) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let entries: Vec<_> = match archive.entries() {
                Ok(e) => e.cloned().collect(),
                Err(_) => continue,
            };
            for entry in &entries {
                if !epp_hashes.contains(&entry.filename_hash) {
                    continue;
                }
                let data = match archive.read_entry(entry) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let spec = match epp::decode_epp(&data) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let actions = serde_json::to_string(&spec.appearance_actions)?;
                let refs = serde_json::to_string(&spec.fx_spec_refs)?;
                insert.execute(params![spec.fqn, actions, refs, spec.raw_xml])?;
                written += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok(written)
    }

    /// Populate `fx_specs` from every `.fxspec` file an
    /// `appearance_specs.fx_spec_refs` entry points at (#183), with the paths
    /// derived from those refs instead of the hash dictionary (#294). Runs
    /// after `populate_appearance_specs`.
    ///
    /// FQN is derived from the resource path between `/fxspec/` and the
    /// trailing `.fxspec`, matching the path-relative keys used by
    /// `appearance_specs.fx_spec_refs`. node_classes is JSON-encoded.
    pub fn populate_fx_specs(&self, tor_dir: &std::path::Path) -> Result<u64> {
        use crate::myp::Archive;
        use crate::schema::fxspec;

        let by_hash = self.derived_fxspec_paths()?;

        let tor_files: Vec<std::path::PathBuf> = std::fs::read_dir(tor_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "tor").unwrap_or(false))
            .collect();

        let mut written = 0u64;
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO fx_specs (fqn, node_classes_json, raw_xml) \
             VALUES (?1, ?2, ?3)",
        )?;
        for tor_path in &tor_files {
            let mut archive = match Archive::open(tor_path) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let entries: Vec<_> = match archive.entries() {
                Ok(e) => e.cloned().collect(),
                Err(_) => continue,
            };
            for entry in &entries {
                let Some(path) = by_hash.get(&entry.filename_hash) else {
                    continue;
                };
                let Some(fqn) = fxspec_fqn_from_path(path) else {
                    continue;
                };
                let data = match archive.read_entry(entry) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let spec = match fxspec::decode_fxspec(&data, fqn) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let classes = serde_json::to_string(&spec.node_classes)?;
                insert.execute(params![spec.fqn, classes, spec.raw_xml])?;
                written += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok(written)
    }

    /// Archive filename hash -> `.fxspec` path for every distinct ref in
    /// `appearance_specs.fx_spec_refs` (#294).
    fn derived_fxspec_paths(&self) -> Result<std::collections::HashMap<u64, String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("database mutex poisoned"))?;
        let mut stmt = conn.prepare("SELECT fx_spec_refs FROM appearance_specs")?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        let mut paths = std::collections::HashMap::new();
        for json in rows {
            let Some(json) = json? else { continue };
            for fx_ref in serde_json::from_str::<Vec<String>>(&json)? {
                let (hash, path) = fxspec_path_from_ref(&fx_ref);
                paths.insert(hash, path);
            }
        }
        Ok(paths)
    }

    /// Populate `scripts` with decrypted SCPT bodies (#182).
    ///
    /// Walks every SCPT-magic entry the main sweep discovered (#292), runs
    /// `kessel::scpt::parse_and_decrypt`, and persists the body
    /// (base64-encoded) plus the numeric_id from the SCPT header.
    /// Per-script semantic interpretation (combat formulas, GSF physics,
    /// UI script logic) lives downstream of this row; this populator
    /// supplies the raw decoded bytes so consumers don't re-decrypt.
    pub fn populate_scripts(
        &self,
        tor_dir: &std::path::Path,
        scpt_hashes: &std::collections::HashSet<u64>,
    ) -> Result<u64> {
        use crate::myp::Archive;
        use crate::scpt;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let tor_files: Vec<std::path::PathBuf> = std::fs::read_dir(tor_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "tor").unwrap_or(false))
            .collect();

        let mut written = 0u64;
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO scripts (script_id, decoded_size, decoded_body_b64) \
             VALUES (?1, ?2, ?3)",
        )?;

        for tor_path in &tor_files {
            let mut archive = match Archive::open(tor_path) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let entries: Vec<_> = match archive.entries() {
                Ok(e) => e.cloned().collect(),
                Err(_) => continue,
            };
            for entry in &entries {
                if !scpt_hashes.contains(&entry.filename_hash) {
                    continue;
                }
                let data = match archive.read_entry(entry) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let (header, body) = match scpt::parse_and_decrypt(&data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let body_b64 = BASE64.encode(&body);
                insert.execute(params![header.numeric_id as i64, body.len(), body_b64])?;
                written += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok(written)
    }

    /// Populate `quest_chain` with `planet_transition` links by scanning every
    /// `leaving_{planet}` quest for strings that name the destination.
    ///
    /// Pattern: strings containing `_to_{planet}` (e.g. `jrn_start_take_the_shuttle_to_dromund_kaas`)
    /// are used to locate the class intro quest at that planet. Strings that name
    /// intermediate stops (e.g. `the_imperial_transit_station`) produce no match
    /// and are silently skipped.
    pub fn populate_planet_transitions(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();

        // Build lookup: fqn -> game_id for all intro quests.
        let mut intro_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT fqn, game_id FROM objects WHERE fqn LIKE 'qst.location.%.class.%.intro' AND is_canonical = 1",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows.filter_map(|r| r.ok()) {
                intro_map.insert(row.0, row.1);
            }
        }

        let mut leaving_quests: Vec<(String, String, String)> = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT fqn, game_id, json_extract(json, '$.strings') \
                 FROM objects \
                 WHERE fqn LIKE 'qst.location.%.class.%.leaving_%' \
                   AND json_extract(json, '$.strings') IS NOT NULL \
                   AND is_canonical = 1",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows.filter_map(|r| r.ok()) {
                leaving_quests.push(row);
            }
        }

        let tx = conn.unchecked_transaction()?;
        let mut count: u64 = 0;

        for (fqn, game_id, strings_json) in &leaving_quests {
            // Extract class segment: qst.location.{planet}.class.{class}.leaving_{planet}
            let parts: Vec<&str> = fqn.split('.').collect();
            let class_pos = parts.iter().position(|&p| p == "class");
            let class = match class_pos {
                Some(i) if i + 1 < parts.len() => parts[i + 1],
                _ => continue,
            };

            let strings: Vec<String> = match serde_json::from_str(strings_json) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Scan strings for `_to_{dest}` patterns; try each as a planet FQN component.
            for s in &strings {
                if let Some(dest) = extract_transit_dest(s) {
                    let intro_fqn = format!("qst.location.{}.class.{}.intro", dest, class);
                    if let Some(target_game_id) = intro_map.get(&intro_fqn) {
                        tx.execute(
                            "INSERT OR IGNORE INTO quest_chain \
                             (source_game_id, target_game_id, link_type) \
                             VALUES (?1, ?2, 'planet_transition')",
                            params![game_id, target_game_id],
                        )?;
                        count += 1;
                        break;
                    }
                }
            }
        }

        tx.commit()?;
        Ok(count)
    }

    /// Extract every SPN triple (`spn.X;target.Y;<numeric>`) from quest
    /// payloads and write rows into `spawn_runtime_ids`. The numeric is
    /// kept as-is for the combat-log bridge: it may be a runtime node ID,
    /// packed coordinates, or both. Decoding waits on combat log capture
    /// (#20).
    pub fn populate_spawn_runtime_ids(&self) -> Result<u64> {
        use crate::pbuk::extract_strings_from_payload;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let quest_rows: Vec<(String, String)> = {
            let conn = self.conn.lock().unwrap();
            fetch_fqn_payloads(&conn, "Quest")?
        };

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO spawn_runtime_ids (spn_fqn, target_fqn, runtime_id) VALUES (?1, ?2, ?3)",
        )?;

        let mut count = 0u64;
        for (_quest_fqn, payload_b64) in &quest_rows {
            let Ok(payload) = BASE64.decode(payload_b64) else {
                continue;
            };
            for s in extract_strings_from_payload(&payload) {
                if let Some((spn_fqn, target_fqn, runtime_id)) = parse_spn_triple(&s) {
                    stmt.execute(rusqlite::params![spn_fqn, target_fqn, runtime_id as i64,])?;
                    count += 1;
                }
            }
        }

        drop(stmt);
        tx.commit()?;
        Ok(count)
    }
}

/// Convert a `.fxspec` resource path into the path-relative key used by
/// `<fxSpecString>` references in `.epp` files. Returns None when the
/// path doesn't contain a `/fxspec/` segment or a `.fxspec` suffix. Used
/// by `populate_fx_specs` (#183) to make `appearance_specs.fx_spec_refs`
/// joinable to `fx_specs.fqn`.
///
/// Example: `/resources/art/fx/fxspec/abilities/sith_warrior/sw_massacre_sword_glow.fxspec`
/// → `abilities/sith_warrior/sw_massacre_sword_glow`.
pub(crate) fn fxspec_fqn_from_path(path: &str) -> Option<String> {
    let after_marker = path.split_once("/fxspec/")?.1;
    let trimmed = after_marker.strip_suffix(".fxspec")?;
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// The archive filename hash and path of the `.fxspec` an `<fxSpecString>`
/// ref names: the inverse of `fxspec_fqn_from_path` (#294). Archive paths are
/// lowercase with forward slashes, while refs are written freely
/// (`\worlddesign\...`, `/mtx/...`, `Sp_Stealth_cast_01`, `x.fxspec`), so the
/// ref is normalized to that form before hashing.
fn fxspec_path_from_ref(fx_ref: &str) -> (u64, String) {
    let key = fx_ref.trim().to_lowercase().replace('\\', "/");
    let key = key.trim_start_matches('/');
    let key = key.strip_suffix(".fxspec").unwrap_or(key);
    let path = format!("/resources/art/fx/fxspec/{key}.fxspec");
    let (ph, sh) = crate::hash::hashlittle2(path.as_bytes(), 0, 0);
    (crate::hash::combine_hash(ph, sh), path)
}

/// Search a record's bytes for the CC <cc_id> marker, then extract the
/// trailing length-prefixed `_pla_<planet>` ASCII string. Used by the
/// conquest events populator.
pub(crate) fn find_planet_code_after_cc(record: &[u8], cc_id: &[u8; 4]) -> Option<String> {
    let mut i = 0;
    while i + 5 <= record.len() {
        if record[i] == 0xCC && record[i + 1..i + 5] == *cc_id {
            // Scan ahead for the `_pla_<planet>` ASCII run.
            let tail = &record[i + 5..];
            for j in 0..tail.len().saturating_sub(5) {
                if &tail[j..j + 5] == b"_pla_" {
                    let mut end = j + 5;
                    while end < tail.len() {
                        let b = tail[end];
                        if !(b.is_ascii_alphanumeric() || b == b'_') {
                            break;
                        }
                        end += 1;
                    }
                    return std::str::from_utf8(&tail[j..end]).ok().map(String::from);
                }
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Create the world tables (idempotent).
pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        r#"
            -- Spawn runtime IDs: every SPN triple `spn.X;target.Y;<id>` in a
            -- quest payload becomes one row. The numeric ID may be the runtime
            -- node ID the combat log emits when the entity is interacted with
            -- (hypothesis from #20, awaiting log verification). Even if it
            -- turns out to be packed coordinates, the bridge data lives here.
            CREATE TABLE IF NOT EXISTS spawn_runtime_ids (
                spn_fqn     TEXT NOT NULL,
                target_fqn  TEXT NOT NULL,
                runtime_id  INTEGER NOT NULL,
                PRIMARY KEY (spn_fqn, target_fqn, runtime_id)
            );
            CREATE INDEX IF NOT EXISTS idx_spawn_runtime_ids_target ON spawn_runtime_ids(target_fqn);
            CREATE INDEX IF NOT EXISTS idx_spawn_runtime_ids_runtime ON spawn_runtime_ids(runtime_id);
            -- Appearance specs (#183). One row per .epp file at
            -- /resources/gamedata/epp/.../<name>.epp. FQN is the dotted
            -- form of the path-relative key. appearance_actions and
            -- fx_spec_refs are JSON arrays decoded from the XML body;
            -- raw_xml preserves the full XML for downstream typed-field
            -- consumers.
            CREATE TABLE IF NOT EXISTS appearance_specs (
                fqn                 TEXT PRIMARY KEY,
                appearance_actions  TEXT,
                fx_spec_refs        TEXT,
                raw_xml             TEXT NOT NULL
            );
            -- FX specs (#183). One row per .fxspec file. node_classes is a
            -- JSON array of the class names listed in the <classes> block;
            -- raw_xml preserves the full XML for per-node-instance
            -- consumers.
            CREATE TABLE IF NOT EXISTS fx_specs (
                fqn                 TEXT PRIMARY KEY,
                node_classes_json   TEXT NOT NULL,
                raw_xml             TEXT NOT NULL
            );
            -- SCPT compiled-native script bodies (#182, closes #127's
            -- consumer gap). One row per .scpt file at
            -- /resources/systemgenerated/compilednative/<numeric_id>.
            -- decoded_body is the post-XOR-decrypt body bytes (typically
            -- x86-64 UI/SFX native code per kessel/src/scpt.rs docs).
            -- Per-script semantic interpretation is a downstream consumer's
            -- job; this table provides the raw decrypted bytes.
            CREATE TABLE IF NOT EXISTS scripts (
                script_id          INTEGER PRIMARY KEY,
                decoded_size       INTEGER NOT NULL,
                decoded_body_b64   TEXT NOT NULL,
                extracted_at       INTEGER NOT NULL DEFAULT (unixepoch())
            );
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::temp_db_path;

    #[test]
    fn fxspec_path_from_ref_matches_archive_hash() {
        // Real PH/SH from hashes_filename.txt for this path.
        let fx_ref = "abilities/sith_warrior/sw_massacre_sword_glow";
        let (hash, path) = fxspec_path_from_ref(fx_ref);
        assert_eq!(hash, crate::hash::combine_hash(0x7898_6A70, 0x7442_C922));
        assert_eq!(fxspec_fqn_from_path(&path).as_deref(), Some(fx_ref));
        for written in [
            "Abilities/Sith_Warrior/SW_Massacre_Sword_Glow",
            "\\abilities\\sith_warrior\\sw_massacre_sword_glow",
            "/abilities/sith_warrior/sw_massacre_sword_glow.fxspec",
        ] {
            assert_eq!(fxspec_path_from_ref(written), (hash, path.clone()));
        }
    }

    #[test]
    fn derived_fxspec_paths_reads_distinct_refs() {
        let path = temp_db_path("derived_fxspecs");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                r#"INSERT INTO appearance_specs (fqn, fx_spec_refs, raw_xml) VALUES
                     ('epp.a', '["abilities/sith_warrior/sw_massacre_sword_glow","x/y"]', ''),
                     ('epp.b', '["x/y"]', ''),
                     ('epp.c', NULL, '');"#,
            )
            .unwrap();
        }
        let paths = db.derived_fxspec_paths().unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths
                .get(&crate::hash::combine_hash(0x7898_6A70, 0x7442_C922))
                .map(String::as_str),
            Some("/resources/art/fx/fxspec/abilities/sith_warrior/sw_massacre_sword_glow.fxspec")
        );
        assert!(paths
            .values()
            .any(|p| p == "/resources/art/fx/fxspec/x/y.fxspec"));
    }

    #[test]
    fn find_planet_code_after_cc_reads_pla_suffix() {
        // CC marker + cc_id, then a `_pla_<planet>` run terminated by a
        // non-[alnum|underscore] byte.
        let cc = [0x0B, 0xAC, 0x73, 0xFD];
        let mut record = vec![0xCC, 0x0B, 0xAC, 0x73, 0xFD];
        record.extend_from_slice(b"\x05x_pla_alderaan\x00more");
        assert_eq!(
            find_planet_code_after_cc(&record, &cc),
            Some("_pla_alderaan".to_string())
        );
    }
    #[test]
    fn find_planet_code_after_cc_returns_none_without_marker() {
        let cc = [0x0B, 0xAC, 0x73, 0xFD];
        // CC byte present but the following four bytes are not the cc_id.
        let record = vec![0xCC, 0x00, 0x00, 0x00, 0x00, b'_', b'p', b'l', b'a', b'_'];
        assert_eq!(find_planet_code_after_cc(&record, &cc), None);
    }
    #[test]
    fn find_planet_code_after_cc_returns_none_when_no_pla_run() {
        let cc = [0x0B, 0xAC, 0x73, 0xFD];
        let mut record = vec![0xCC, 0x0B, 0xAC, 0x73, 0xFD];
        record.extend_from_slice(b"no planet here");
        assert_eq!(find_planet_code_after_cc(&record, &cc), None);
    }
}
