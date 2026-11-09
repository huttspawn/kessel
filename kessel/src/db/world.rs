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

    /// Populate `fx_specs` from every FX-spec-content entry the main sweep
    /// found (#183, #295). Names come from the paths derived from
    /// `appearance_specs.fx_spec_refs` (#294), so this runs after
    /// `populate_appearance_specs`. A supplied hash dictionary only adds
    /// names for entries no ref names (FR-KESSEL-024); an entry with no name
    /// is kept with a NULL `fqn` and its `file_hash`.
    ///
    /// FQN is derived from the resource path between `/fxspec/` and the
    /// trailing `.fxspec`, matching the path-relative keys used by
    /// `appearance_specs.fx_spec_refs`. node_classes is JSON-encoded.
    pub fn populate_fx_specs(
        &self,
        tor_dir: &std::path::Path,
        fxspec_hashes: &std::collections::HashSet<u64>,
        hashes: &crate::hash::HashDictionary,
    ) -> Result<u64> {
        use crate::myp::Archive;
        use crate::schema::fxspec;

        let mut by_hash = self.derived_fxspec_paths()?;
        for (hash, path) in hashes.paths_matching(".fxspec") {
            if path.ends_with(".fxspec") {
                by_hash.entry(hash).or_insert_with(|| path.clone());
            }
        }

        let tor_files: Vec<std::path::PathBuf> = std::fs::read_dir(tor_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "tor").unwrap_or(false))
            .collect();

        let mut written = 0u64;
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO fx_specs (file_hash, fqn, node_classes_json, raw_xml) \
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
                if !fxspec_hashes.contains(&entry.filename_hash) {
                    continue;
                }
                let fqn = by_hash
                    .get(&entry.filename_hash)
                    .and_then(|path| fxspec_fqn_from_path(path));
                let data = match archive.read_entry(entry) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let spec = match fxspec::decode_fxspec(&data, fqn.clone().unwrap_or_default()) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let classes = serde_json::to_string(&spec.node_classes)?;
                let file_hash = format!("{:016X}", entry.filename_hash);
                insert.execute(params![file_hash, fqn, classes, spec.raw_xml])?;
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

/// What the world-area pass wrote, for the run summary.
#[derive(Debug, Default)]
pub struct WorldAreaCounts {
    /// (log area id, directory) pairs, and the distinct log area ids among them.
    pub area_log_ids: u64,
    pub log_area_ids: u64,
    /// Directories tried; area.dat found in the archives; parsed; found but
    /// unreadable or unparseable.
    pub areas_listed: u64,
    pub areas_found: u64,
    pub areas_parsed: u64,
    pub areas_unparseable: u64,
    /// Rooms the parsed area.dat files list; read; not in the archives;
    /// found but unreadable or unparseable.
    pub rooms_listed: u64,
    pub rooms_read: u64,
    pub rooms_missing: u64,
    pub rooms_unparseable: u64,
    pub instances: u64,
    pub assets_resolved: u64,
    pub placements: u64,
    pub trigger_volumes: u64,
    pub placeable_grants: u64,
    pub script_actions: u64,
}

// mapAreasDataProto: the area map, and per entry the area directory and the
// id the combat log prints in AreaEntered.
const MAP_AREAS: u32 = 0x0027_bf7b;
const MAP_AREA_DIR: u32 = 0x0027_bf77;
const MAP_AREA_LOG_ID: u32 = 0x0027_bf7f;
// Spawn: entity list, each naming the template it places.
const SPN_ENTRIES: u32 = 0xaa61_0ec0;
const SPN_ENTRY_TEMPLATE: u32 = 0xb78d_6eec;
// Encounter: slot letter -> spawner FQN.
const ENC_SLOTS: u32 = 0xd9a5_2ab9;
// Placeable: plcAbilitySpecOnUse, plcCodexSpec, plcConvo, On Use script.
const PLC_ON_USE: u32 = 0x1d6b_4924;
const PLC_CODEX: u32 = 0x2d7b_8797;
const PLC_CONVO: u32 = 0xf082_7b5d;
const PLC_SCRIPT: u32 = 0x4fa1_4a25;
// Hydra: handler map, action lists, action name and value.
const HYD_HANDLERS: u32 = 0xe389_e2e3;
const HYD_ACTION_LIST: u32 = 0xe534_715a;
const HYD_ACTION: u32 = 0xe534_6fa0;
const HYD_VALUE: u32 = 0xe534_6faf;
// hydObject, hydQueueObject: signed FNV-1a 64 of a selector name.
const HYD_OBJECT: u32 = 0xed10_e6a4;
const HYD_QUEUE_OBJECT: u32 = 0x9756_f193;
/// The selectors an action credits, by name.
const HYD_SELECTORS: [&str; 9] = [
    "INSTIGATOR",
    "PHASE OCCUPANTS",
    "OBJECT SELF",
    "EFFECT TARGET",
    "PHASE OWNER",
    "INSTIGATOR GROUP",
    "TEAMA PLAYERS",
    "TEAMB PLAYERS",
    "PET MASTER",
];

impl Database {
    /// Populate the world tables (FR-KESSEL-025 c5, c10): `area_log_ids`
    /// from mapAreasDataProto; `placeable_grants` and `script_actions` from
    /// the Placeable and Hydra objects; `placements` and `trigger_volumes`
    /// from every area.dat and room .dat under `/resources/world/areas`.
    /// Area directories come from mapAreasDataProto and the map-page objects
    /// `world.areas.<id>.mapdata`; a supplied hash dictionary adds its
    /// area.dat paths.
    pub fn populate_world_areas(
        &self,
        tor_dir: &std::path::Path,
        hashes: &crate::hash::HashDictionary,
    ) -> Result<WorldAreaCounts> {
        use std::collections::{BTreeSet, HashMap};

        self.with_tx(|tx| {
            let mut counts = WorldAreaCounts::default();

            let pairs = match load_singleton(tx, "mapAreasDataProto")? {
                Some(proto) => area_log_pairs(&proto),
                None => Vec::new(),
            };
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO area_log_ids (log_area_id, area_dir_id) VALUES (?1, ?2)",
            )?;
            for (log_id, dir) in &pairs {
                if let Some(log_id) = log_id {
                    insert.execute(params![log_id, *dir as i64])?;
                    counts.area_log_ids += 1;
                }
            }
            drop(insert);
            counts.log_area_ids = pairs
                .iter()
                .filter_map(|(log_id, _)| *log_id)
                .collect::<BTreeSet<_>>()
                .len() as u64;

            counts.placeable_grants = insert_placeable_grants(tx)?;
            counts.script_actions = insert_script_actions(tx)?;

            let mut dirs: BTreeSet<u64> = pairs.iter().map(|(_, dir)| *dir).collect();
            let mut mapdata =
                tx.prepare("SELECT fqn FROM objects WHERE fqn LIKE 'world.areas.%.mapdata'")?;
            for fqn in mapdata.query_map([], |r| r.get::<_, String>(0))? {
                if let Some(dir) = fqn?
                    .strip_prefix("world.areas.")
                    .and_then(|s| s.strip_suffix(".mapdata"))
                    .and_then(|d| d.parse().ok())
                {
                    dirs.insert(dir);
                }
            }
            drop(mapdata);
            for (_, path) in hashes.paths_matching("/resources/world/areas/") {
                if let Some(dir) = path
                    .strip_prefix("/resources/world/areas/")
                    .and_then(|p| p.strip_suffix("/area.dat"))
                    .and_then(|d| d.parse().ok())
                {
                    dirs.insert(dir);
                }
            }

            let spawn_entities: HashMap<String, String> = decoded_objects(tx, "Spawn")?
                .into_iter()
                .filter_map(|(fqn, obj)| {
                    let first = obj.embedded_field(SPN_ENTRIES)?.as_list()?.first()?;
                    Some((fqn, gom_str(first.embedded_field(SPN_ENTRY_TEMPLATE)?)?))
                })
                .collect();
            let encounter_slots: HashMap<String, HashMap<String, String>> =
                decoded_objects(tx, "Encounter")?
                    .into_iter()
                    .filter_map(|(fqn, obj)| {
                        let slots = obj.embedded_field(ENC_SLOTS)?.as_map()?;
                        let slots = slots
                            .iter()
                            .filter_map(|(k, v)| Some((gom_str(k)?, gom_str(v)?)))
                            .collect();
                        Some((fqn, slots))
                    })
                    .collect();

            let mut files = ArchiveFiles::open(tor_dir)?;
            let mut placement = tx.prepare(
                "INSERT OR REPLACE INTO placements \
                   (area_dir_id, room, instance_id, parent_instance_id, asset_path, spawner_fqn, \
                    spawner_type, entity_fqn, world_x, world_y, world_z, yaw, log_x, log_y, log_z) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;
            let mut volume = tx.prepare(
                "INSERT OR REPLACE INTO trigger_volumes \
                   (area_dir_id, room, instance_id, shape, size_x, size_height, size_z, yaw, \
                    world_x, world_y, world_z, log_min_x, log_min_y, log_min_z, \
                    log_max_x, log_max_y, log_max_z, script_fqn, region_name) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                         ?16, ?17, ?18, ?19)",
            )?;
            for dir in dirs {
                counts.areas_listed += 1;
                let prefix = format!("/resources/world/areas/{dir}");
                let Some(data) = files.read(&format!("{prefix}/area.dat")) else {
                    continue;
                };
                counts.areas_found += 1;
                let Some(area) = data.ok().and_then(|d| crate::area::read_area(&d).ok()) else {
                    counts.areas_unparseable += 1;
                    continue;
                };
                counts.areas_parsed += 1;
                let mut insts = Vec::new();
                // Archive paths are lowercase; area.dat writes room names in mixed case.
                for room in area.rooms.iter().map(|r| r.to_lowercase()) {
                    counts.rooms_listed += 1;
                    let Some(data) = files.read(&format!("{prefix}/{room}.dat")) else {
                        counts.rooms_missing += 1;
                        continue;
                    };
                    let Some(room_insts) = data.ok().and_then(|d| crate::area::read_room(&d).ok())
                    else {
                        counts.rooms_unparseable += 1;
                        continue;
                    };
                    counts.rooms_read += 1;
                    insts.extend(room_insts.into_iter().map(|i| (room.clone(), i)));
                }
                let by_id: HashMap<u64, &crate::area::Instance> =
                    insts.iter().map(|(_, i)| (i.id, i)).collect();
                for (room, inst) in &insts {
                    counts.instances += 1;
                    let Some(path) = area.assets.get(&inst.asset) else {
                        continue;
                    };
                    counts.assets_resolved += 1;
                    let world = crate::area::world_of(inst.id, &by_id);
                    let (pos, yaw) = (world.map(|w| w.0), world.map(|w| w.1));
                    if path.ends_with(".trg") {
                        let size = inst.size;
                        let bounds = match (pos, yaw, size) {
                            (Some(p), Some(y), [Some(x), Some(h), Some(z)]) => {
                                Some(crate::area::log_bounds(p, [x, h, z], y))
                            }
                            _ => None,
                        };
                        let script = inst.script.as_deref();
                        let is_hydra = script.is_some_and(|s| s.starts_with("hyd."));
                        volume.execute(params![
                            dir as i64,
                            room,
                            inst.id as i64,
                            inst.shape,
                            size[0],
                            size[1],
                            size[2],
                            yaw,
                            pos.map(|p| p[0]),
                            pos.map(|p| p[1]),
                            pos.map(|p| p[2]),
                            bounds.map(|b| b.0[0]),
                            bounds.map(|b| b.0[1]),
                            bounds.map(|b| b.0[2]),
                            bounds.map(|b| b.1[0]),
                            bounds.map(|b| b.1[1]),
                            bounds.map(|b| b.1[2]),
                            script.filter(|_| is_hydra),
                            script.filter(|_| !is_hydra),
                        ])?;
                        counts.trigger_volumes += 1;
                    } else if let Some(spawner) = crate::area::spawner_of_asset(path) {
                        let spawner_fqn = match &spawner {
                            crate::area::SpawnerAsset::Spawner { fqn, .. } => Some(fqn.clone()),
                            crate::area::SpawnerAsset::Slot {
                                encounter, slot, ..
                            } => encounter_slots
                                .get(encounter)
                                .and_then(|s| s.get(slot))
                                .cloned(),
                        };
                        let entity = spawner_fqn.as_ref().and_then(|f| spawn_entities.get(f));
                        let log = pos.map(crate::area::to_log);
                        placement.execute(params![
                            dir as i64,
                            room,
                            inst.id as i64,
                            (inst.parent != 0).then_some(inst.parent as i64),
                            path,
                            spawner_fqn,
                            crate::area::spawner_type(spawner.ext()),
                            entity,
                            pos.map(|p| p[0]),
                            pos.map(|p| p[1]),
                            pos.map(|p| p[2]),
                            yaw,
                            log.map(|l| l[0]),
                            log.map(|l| l[1]),
                            log.map(|l| l[2]),
                        ])?;
                        counts.placements += 1;
                    }
                }
            }
            Ok(counts)
        })
    }
}

/// Every file in the archives, found by the hash of its path.
struct ArchiveFiles {
    archives: Vec<crate::myp::Archive>,
    index: std::collections::HashMap<u64, (usize, crate::myp::FileEntry)>,
}

impl ArchiveFiles {
    fn open(tor_dir: &std::path::Path) -> Result<Self> {
        let mut tor_files: Vec<std::path::PathBuf> = std::fs::read_dir(tor_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "tor"))
            .collect();
        tor_files.sort();
        let mut archives = Vec::new();
        let mut index = std::collections::HashMap::new();
        for path in tor_files {
            let Ok(archive) = crate::myp::Archive::open(&path) else {
                continue;
            };
            let i = archives.len();
            for entry in archive.entries()? {
                index
                    .entry(entry.filename_hash)
                    .or_insert_with(|| (i, entry.clone()));
            }
            archives.push(archive);
        }
        Ok(Self { archives, index })
    }

    /// `None` when no archive holds the path; `Some(Err)` when the entry
    /// cannot be read.
    fn read(&mut self, path: &str) -> Option<Result<Vec<u8>>> {
        let (ph, sh) = crate::hash::hashlittle2(path.as_bytes(), 0, 0);
        let (i, entry) = self.index.get(&crate::hash::combine_hash(ph, sh))?;
        Some(self.archives.get_mut(*i)?.read_entry(entry))
    }
}

/// Decode a singleton by FQN; `None` when absent or undecodable.
pub(super) fn load_singleton(
    tx: &Transaction,
    fqn: &str,
) -> Result<Option<crate::gom_reader::GomValue>> {
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT header_hex, payload_b64 FROM singletons WHERE fqn = ?1",
            [fqn],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    Ok(row.and_then(|(header, payload)| decode_framed(&header, &payload)))
}

/// Every canonical object of `kind`, decoded; undecodable payloads are skipped.
pub(super) fn decoded_objects(
    tx: &Transaction,
    kind: &str,
) -> Result<Vec<(String, crate::gom_reader::GomValue)>> {
    let mut stmt = tx.prepare(
        "SELECT fqn, json_extract(json, '$.header_hex'), json_extract(json, '$.payload_b64') \
         FROM objects WHERE kind = ?1 AND is_canonical = 1",
    )?;
    let rows = stmt
        .query_map([kind], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(fqn, header, payload)| Some((fqn, decode_framed(&header?, &payload?)?)))
        .collect())
}

pub(super) fn decode_framed(
    header_hex: &str,
    payload_b64: &str,
) -> Option<crate::gom_reader::GomValue> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    let header = hex::decode(header_hex).ok()?;
    let payload = BASE64.decode(payload_b64).ok()?;
    crate::gom_reader::read_framed_object(&header, &payload)
        .ok()
        .map(|(v, _)| v)
}

pub(super) fn gom_str(v: &crate::gom_reader::GomValue) -> Option<String> {
    match v {
        crate::gom_reader::GomValue::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// (log area id, area directory id) for every mapAreasDataProto entry; an
/// entry with no log area id still names its directory.
fn area_log_pairs(proto: &crate::gom_reader::GomValue) -> Vec<(Option<i64>, u64)> {
    let Some(areas) = proto.embedded_field(MAP_AREAS).and_then(|m| m.as_map()) else {
        return Vec::new();
    };
    areas
        .iter()
        .filter_map(|(_, area)| {
            let dir = area.embedded_field(MAP_AREA_DIR)?.as_i64()?;
            let log_id = area
                .embedded_field(MAP_AREA_LOG_ID)
                .and_then(crate::gom_reader::GomValue::as_i64);
            Some((log_id, dir as u64))
        })
        .collect()
}

/// The GUID-or-FQN fields of a placeable that name what it grants.
#[derive(Debug, Default, PartialEq)]
struct PlaceableRefs {
    on_use: Option<u64>,
    codex: Option<u64>,
    convo: Option<String>,
    script: Option<u64>,
}

fn placeable_refs(obj: &crate::gom_reader::GomValue) -> PlaceableRefs {
    let guid = |id| {
        obj.embedded_field(id)
            .and_then(crate::gom_reader::GomValue::as_i64)
            .filter(|g| *g != 0)
            .map(|g| g as u64)
    };
    PlaceableRefs {
        on_use: guid(PLC_ON_USE),
        codex: guid(PLC_CODEX),
        convo: obj.embedded_field(PLC_CONVO).and_then(gom_str),
        script: guid(PLC_SCRIPT),
    }
}

fn insert_placeable_grants(tx: &Transaction) -> Result<u64> {
    let mut by_guid = tx.prepare(
        "SELECT fqn, string_id FROM objects WHERE guid = ?1 AND is_canonical = 1 LIMIT 1",
    )?;
    let mut lookup = |guid: Option<u64>| -> Option<(String, Option<i64>)> {
        let guid = format!("{:016X}", guid?);
        by_guid
            .query_row([guid], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok()
    };
    let mut insert = tx.prepare(
        "INSERT OR REPLACE INTO placeable_grants \
           (placeable_fqn, use_ability_fqn, use_ability_log_id, codex_fqn, conversation_fqn, script_fqn) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let mut written = 0;
    for (fqn, obj) in decoded_objects(tx, "Placeable")? {
        let refs = placeable_refs(&obj);
        if refs == PlaceableRefs::default() {
            continue;
        }
        let ability = lookup(refs.on_use);
        let log_id = ability
            .as_ref()
            .and_then(|(_, sid)| *sid)
            .map(|sid| sid << 32);
        insert.execute(params![
            fqn,
            ability.map(|a| a.0),
            log_id,
            lookup(refs.codex).map(|c| c.0),
            refs.convo,
            lookup(refs.script).map(|s| s.0),
        ])?;
        written += 1;
    }
    Ok(written)
}

/// One action in a hydra script handler, in handler order.
#[derive(Debug, PartialEq)]
struct ScriptAction {
    handler: String,
    action: String,
    target_fqn: Option<String>,
    value: Option<String>,
    selector: Option<String>,
}

/// The selector an action's hydObject (else hydQueueObject) names, or the raw
/// id when it is none of the known selectors.
fn action_selector(action: &crate::gom_reader::GomValue) -> Option<String> {
    let id = [HYD_OBJECT, HYD_QUEUE_OBJECT].iter().find_map(|f| {
        action
            .embedded_field(*f)
            .and_then(crate::gom_reader::GomValue::as_i64)
    })?;
    Some(
        HYD_SELECTORS
            .iter()
            .find(|n| crate::hash::fnv1a64(n) as i64 == id)
            .map_or_else(|| id.to_string(), |n| (*n).to_string()),
    )
}

/// Every action of every handler in a hydra script. Actions are the entries
/// of the action lists; conditions are not actions.
fn script_actions(obj: &crate::gom_reader::GomValue) -> Vec<ScriptAction> {
    use crate::gom_reader::GomValue;

    fn walk(v: &GomValue, handler: &str, out: &mut Vec<ScriptAction>) {
        match v {
            GomValue::Embedded(fields) => {
                for (id, x) in fields {
                    if *id as u32 == HYD_ACTION_LIST {
                        for a in x.as_list().unwrap_or_default() {
                            if let Some(action) = a.embedded_field(HYD_ACTION).and_then(gom_str) {
                                let value = a.embedded_field(HYD_VALUE).and_then(|v| match v {
                                    GomValue::Str(s) => Some(s.clone()),
                                    other => other.as_i64().map(|n| n.to_string()),
                                });
                                out.push(ScriptAction {
                                    handler: handler.to_string(),
                                    action,
                                    target_fqn: value.clone().filter(|v| names_object(v)),
                                    value,
                                    selector: action_selector(a),
                                });
                            }
                        }
                    }
                    walk(x, handler, out);
                }
            }
            GomValue::List(items) => items.iter().for_each(|x| walk(x, handler, out)),
            GomValue::Map(entries) => entries.iter().for_each(|(_, x)| walk(x, handler, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    for (name, handler) in obj
        .embedded_field(HYD_HANDLERS)
        .and_then(GomValue::as_map)
        .unwrap_or_default()
    {
        if let Some(name) = gom_str(name) {
            walk(handler, &name, &mut out);
        }
    }
    out
}

/// True when an action value is an object FQN (`cdx.persons.x`), not a name
/// or a number.
fn names_object(value: &str) -> bool {
    value.split_once('.').is_some_and(|(prefix, rest)| {
        !prefix.is_empty()
            && prefix.bytes().all(|b| b.is_ascii_lowercase())
            && !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
    })
}

fn insert_script_actions(tx: &Transaction) -> Result<u64> {
    let mut insert = tx.prepare(
        "INSERT OR REPLACE INTO script_actions \
           (script_fqn, handler, ordinal, action, target_fqn, value, selector) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut written = 0;
    for (fqn, obj) in decoded_objects(tx, "Hydra")? {
        let mut ordinal: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for a in script_actions(&obj) {
            let n = ordinal.entry(a.handler.clone()).or_default();
            insert.execute(params![
                fqn,
                a.handler,
                *n,
                a.action,
                a.target_fqn,
                a.value,
                a.selector
            ])?;
            *n += 1;
            written += 1;
        }
    }
    Ok(written)
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
            -- FX specs (#183). One row per .fxspec file, keyed by its
            -- archive filename hash (#295); fqn is NULL when no path names
            -- the file. node_classes is a JSON array of the class names
            -- listed in the <classes> block; raw_xml preserves the full XML
            -- for per-node-instance consumers. Dropped first: the pass
            -- rewrites it whole, and a pre-#295 output file holds the
            -- old fqn-keyed table.
            DROP TABLE IF EXISTS fx_specs;
            CREATE TABLE IF NOT EXISTS fx_specs (
                file_hash           TEXT PRIMARY KEY,
                fqn                 TEXT UNIQUE,
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
            -- World areas (FR-KESSEL-025 c5, c10). The id the combat log
            -- prints in AreaEntered, joined to the area directories it names
            -- (mapAreasDataProto 0027bf7f / 0027bf77); an id with several
            -- directories has several rows.
            CREATE TABLE IF NOT EXISTS area_log_ids (
                log_area_id  INTEGER NOT NULL,
                area_dir_id  INTEGER NOT NULL,
                PRIMARY KEY (log_area_id, area_dir_id)
            );
            -- One row per room instance whose asset is a spawner (encounter
            -- slots included). World position (x, height y, z) is composed
            -- through the parent chain; the log frame is 10 x (x, z, y).
            CREATE TABLE IF NOT EXISTS placements (
                area_dir_id         INTEGER NOT NULL,
                room                TEXT NOT NULL,
                instance_id         INTEGER NOT NULL,
                parent_instance_id  INTEGER,
                asset_path          TEXT NOT NULL,
                spawner_fqn         TEXT,
                spawner_type        TEXT,
                entity_fqn          TEXT,
                world_x REAL, world_y REAL, world_z REAL,
                yaw     REAL,
                log_x   REAL, log_y   REAL, log_z   REAL,
                PRIMARY KEY (area_dir_id, instance_id)
            );
            CREATE INDEX IF NOT EXISTS idx_placements_spawner ON placements(spawner_fqn);
            CREATE INDEX IF NOT EXISTS idx_placements_entity ON placements(entity_fqn);
            -- What a placeable names: its use ability (and the id the log
            -- prints for it, string_id << 32), codex entry, conversation and
            -- On Use script.
            CREATE TABLE IF NOT EXISTS placeable_grants (
                placeable_fqn       TEXT PRIMARY KEY,
                use_ability_fqn     TEXT,
                use_ability_log_id  INTEGER,
                codex_fqn           TEXT,
                conversation_fqn    TEXT,
                script_fqn          TEXT
            );
            -- Trigger volumes: full sizes centred on the volume, turned by
            -- yaw; the log-frame box encloses it. script_fqn holds a hyd.*
            -- script, region_name a bare region name.
            CREATE TABLE IF NOT EXISTS trigger_volumes (
                area_dir_id  INTEGER NOT NULL,
                room         TEXT NOT NULL,
                instance_id  INTEGER NOT NULL,
                shape        INTEGER,
                size_x REAL, size_height REAL, size_z REAL,
                yaw    REAL,
                world_x REAL, world_y REAL, world_z REAL,
                log_min_x REAL, log_min_y REAL, log_min_z REAL,
                log_max_x REAL, log_max_y REAL, log_max_z REAL,
                script_fqn   TEXT,
                region_name  TEXT,
                PRIMARY KEY (area_dir_id, instance_id)
            );
            CREATE INDEX IF NOT EXISTS idx_trigger_volumes_script ON trigger_volumes(script_fqn);
            -- Every hydra handler's actions in order. target_fqn is the
            -- value when it names an object (Grant Codex cdx.*); value is
            -- the action's value as written (an event name, a title id);
            -- selector the object it acts on or credits (hydObject, else
            -- hydQueueObject: INSTIGATOR, PHASE OCCUPANTS ...), the raw id
            -- when no known selector hashes to it.
            CREATE TABLE IF NOT EXISTS script_actions (
                script_fqn  TEXT NOT NULL,
                handler     TEXT NOT NULL,
                ordinal     INTEGER NOT NULL,
                action      TEXT NOT NULL,
                target_fqn  TEXT,
                value       TEXT,
                selector    TEXT,
                PRIMARY KEY (script_fqn, handler, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_script_actions_target ON script_actions(target_fqn);
            -- What a logged use cast or conversation can land on (FR-KESSEL-025
            -- c9), in the log's frame and keyed by the ids the log prints.
            -- Every placed placeable that plays a use ability, by the cast id
            -- (use_ability_log_id) a use line prints.
            CREATE VIEW IF NOT EXISTS log_use_targets AS
                SELECT l.log_area_id, g.use_ability_log_id, g.placeable_fqn,
                       p.area_dir_id, p.instance_id, p.log_x, p.log_y, p.log_z,
                       g.codex_fqn, g.conversation_fqn, g.script_fqn
                FROM placeable_grants g
                JOIN placements p ON p.entity_fqn = g.placeable_fqn
                JOIN area_log_ids l ON l.area_dir_id = p.area_dir_id
                WHERE g.use_ability_log_id IS NOT NULL;
            -- Every placed NPC or placeable that owns a conversation
            -- (conversation_owners, created by cnv). owner_log_id is the id a
            -- TargetSet line prints for an NPC, string_id << 32; a placeable
            -- has none. A placeable whose On Use script plays a conversation
            -- owns that conversation too.
            CREATE VIEW IF NOT EXISTS log_conversation_targets AS
                SELECT l.log_area_id,
                       CASE WHEN o.owner_kind = 'npc' THEN
                           (SELECT ob.string_id << 32 FROM objects ob
                            WHERE ob.fqn = o.owner_fqn AND ob.is_canonical = 1
                              AND ob.string_id IS NOT NULL LIMIT 1)
                       END AS owner_log_id,
                       o.owner_fqn, o.owner_kind, o.conversation_fqn,
                       p.area_dir_id, p.instance_id, p.log_x, p.log_y, p.log_z
                FROM conversation_owners o
                JOIN placements p ON p.entity_fqn = o.owner_fqn
                JOIN area_log_ids l ON l.area_dir_id = p.area_dir_id
                UNION ALL
                SELECT l.log_area_id, NULL, g.placeable_fqn, 'placeable', a.target_fqn,
                       p.area_dir_id, p.instance_id, p.log_x, p.log_y, p.log_z
                FROM placeable_grants g
                JOIN script_actions a ON a.script_fqn = g.script_fqn
                     AND a.handler = 'On Use' AND a.action = 'Play Conversation'
                     AND a.target_fqn IS NOT NULL
                JOIN placements p ON p.entity_fqn = g.placeable_fqn
                JOIN area_log_ids l ON l.area_dir_id = p.area_dir_id;
            -- What a placeable is and what using it triggers (FR-KESSEL-025
            -- c9): its codex entry (plcCodexSpec), its conversation
            -- (plcConvo), every action of its On Use script that changes game
            -- state or moves the player, following Execute Hydra Script
            -- ('<script>:<event>' runs that script's 'On Custom:<event>')
            -- with a cycle guard, and the quest tasks it is an interaction
            -- object of (quest_task_objects, created by quest), which using
            -- it advances. Pure presentation and expression steps are left out.
            CREATE VIEW IF NOT EXISTS placeable_triggers AS
                WITH RECURSIVE reach(placeable_fqn, script_fqn, handler, path) AS (
                    SELECT placeable_fqn, script_fqn, 'On Use', ',' || script_fqn || ','
                    FROM placeable_grants WHERE script_fqn IS NOT NULL
                    UNION
                    SELECT r.placeable_fqn, substr(a.value, 1, instr(a.value, ':') - 1),
                           'On Custom:' || substr(a.value, instr(a.value, ':') + 1),
                           r.path || substr(a.value, 1, instr(a.value, ':') - 1) || ','
                    FROM reach r
                    JOIN script_actions a ON a.script_fqn = r.script_fqn AND a.handler = r.handler
                    WHERE a.action = 'Execute Hydra Script' AND instr(a.value, ':') > 0
                      AND instr(r.path, ',' || substr(a.value, 1, instr(a.value, ':') - 1) || ',') = 0
                )
                SELECT placeable_fqn, 'codex' AS kind, codex_fqn AS target_fqn,
                       'plcCodexSpec' AS detail
                FROM placeable_grants WHERE codex_fqn IS NOT NULL
                UNION ALL
                SELECT placeable_fqn, 'conversation', conversation_fqn, 'plcConvo'
                FROM placeable_grants WHERE conversation_fqn IS NOT NULL
                UNION ALL
                SELECT DISTINCT r.placeable_fqn, 'on_use', COALESCE(a.target_fqn, a.value, ''),
                       a.action || ' (' || a.script_fqn || ' ' || a.handler || ')'
                FROM reach r
                JOIN script_actions a ON a.script_fqn = r.script_fqn AND a.handler = r.handler
                WHERE a.action NOT IN ('Execute Hydra Script', 'Play SFX', 'Play SFX From Camera',
                                       'Play Animation', 'Wait', 'Broadcast String',
                                       'Broadcast String From Player', 'Debug Broadcast String',
                                       'Set Target', 'Set Facing', 'Play Appearance', 'Play VFX',
                                       'Play Targeted VFX', 'Stop Character VFX',
                                       'Play Camera Shake', 'Play Music', 'Play Ambient',
                                       'Fade to Black', 'Fade from Black', 'Fade to Color',
                                       'Set Callback Timer', '=', '+', '-')
                UNION ALL
                SELECT o.placeable_fqn, 'quest_task', o.quest_fqn,
                       'task (' || o.branch || ',' || o.step || ',' || o.task || ') '''
                         || COALESCE(t.text, '') || ''' hook ' || COALESCE(h.hook_id, '')
                -- A task names the placeable itself or a spawner that places it.
                FROM (SELECT object_fqn AS placeable_fqn, quest_fqn, branch, step, task
                      FROM quest_task_objects
                      UNION
                      SELECT pl.entity_fqn, q.quest_fqn, q.branch, q.step, q.task
                      FROM quest_task_objects q
                      JOIN placements pl ON pl.spawner_fqn = q.object_fqn
                      WHERE pl.entity_fqn IS NOT NULL) o
                JOIN objects ob ON ob.fqn = o.placeable_fqn AND ob.kind = 'Placeable'
                     AND ob.is_canonical = 1
                LEFT JOIN quest_tasks t ON t.quest_fqn = o.quest_fqn AND t.branch = o.branch
                     AND t.step = o.step AND t.task = o.task
                LEFT JOIN quest_task_hooks h ON h.quest_fqn = o.quest_fqn AND h.branch = o.branch
                     AND h.step = o.step AND h.task = o.task;
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

    use crate::gom_reader::GomValue as G;

    fn obj(fields: Vec<(u32, G)>) -> G {
        G::Embedded(
            fields
                .into_iter()
                .map(|(id, v)| (0x4000_0000_0000_0000 | u64::from(id), v))
                .collect(),
        )
    }

    #[test]
    fn area_log_ids_keep_every_directory() {
        let entry = |dir: u64, log: i64| {
            obj(vec![
                (MAP_AREA_DIR, G::U64(dir)),
                (MAP_AREA_LOG_ID, G::I64(log)),
            ])
        };
        let proto = obj(vec![(
            MAP_AREAS,
            G::Map(vec![
                (G::U64(1), entry(4_611_686_019_802_841_877, 137_438_987_025)),
                (G::U64(2), entry(4_611_686_019_802_841_878, 137_438_987_025)),
                (G::U64(3), obj(vec![(MAP_AREA_DIR, G::U64(9))])),
            ]),
        )]);
        assert_eq!(
            area_log_pairs(&proto),
            [
                (Some(137_438_987_025), 4_611_686_019_802_841_877),
                (Some(137_438_987_025), 4_611_686_019_802_841_878),
                (None, 9)
            ]
        );
    }

    #[test]
    fn placeable_refs_read_grant_fields() {
        let plc = obj(vec![
            (PLC_CONVO, G::Str("cnv.location.tython.datacron_b".into())),
            (PLC_ON_USE, G::U64(0xE000_6101_0000_0001)),
            (PLC_CODEX, G::U64(0xE000_2D74_70E2_B65E)),
            (PLC_SCRIPT, G::U64(0xE000_0000_0000_0002)),
        ]);
        assert_eq!(
            placeable_refs(&plc),
            PlaceableRefs {
                on_use: Some(0xE000_6101_0000_0001),
                codex: Some(0xE000_2D74_70E2_B65E),
                convo: Some("cnv.location.tython.datacron_b".into()),
                script: Some(0xE000_0000_0000_0002),
            }
        );
        assert_eq!(placeable_refs(&obj(vec![])), PlaceableRefs::default());
    }

    #[test]
    fn script_actions_read_each_handler_in_order_without_conditions() {
        let action = |name: &str, value: &str| {
            obj(vec![
                (HYD_ACTION, G::Str(name.into())),
                (HYD_VALUE, G::Str(value.into())),
                (
                    HYD_OBJECT,
                    G::I64(crate::hash::fnv1a64("INSTIGATOR") as i64),
                ),
            ])
        };
        let handler = |actions: Vec<G>| {
            obj(vec![(
                0xe52e_3b17,
                G::List(vec![obj(vec![
                    (
                        0xe52e_42ef,
                        G::List(vec![obj(vec![(HYD_ACTION_LIST, G::List(actions))])]),
                    ),
                    (
                        0xe52e_4395,
                        obj(vec![(0x9e1f_7013, G::List(vec![action("==", "true")]))]),
                    ),
                ])]),
            )])
        };
        let script = obj(vec![(
            HYD_HANDLERS,
            G::Map(vec![
                (
                    G::Str("On Enter".into()),
                    handler(vec![
                        action("Grant Codex", "cdx.persons.smuggler.drooga_the_hutt"),
                        action("Achievement Event", "RishiHiddenTome05"),
                    ]),
                ),
                (
                    G::Str("On Custom:Found Willpower".into()),
                    handler(vec![action("Grant Title", "671")]),
                ),
            ]),
        )]);
        let got = script_actions(&script);
        let row = |h: &str, a: &str, t: Option<&str>, v: &str| ScriptAction {
            handler: h.into(),
            action: a.into(),
            target_fqn: t.map(Into::into),
            value: Some(v.into()),
            selector: Some("INSTIGATOR".into()),
        };
        assert_eq!(
            got,
            [
                row(
                    "On Enter",
                    "Grant Codex",
                    Some("cdx.persons.smuggler.drooga_the_hutt"),
                    "cdx.persons.smuggler.drooga_the_hutt"
                ),
                row("On Enter", "Achievement Event", None, "RishiHiddenTome05"),
                row("On Custom:Found Willpower", "Grant Title", None, "671"),
            ]
        );
    }

    #[test]
    fn action_selector_names_known_selectors_else_the_raw_id() {
        let sel = |f: u32, id: i64| obj(vec![(f, G::I64(id))]);
        let target = crate::hash::fnv1a64("EFFECT TARGET") as i64;
        assert_eq!(
            action_selector(&sel(HYD_QUEUE_OBJECT, target)).as_deref(),
            Some("EFFECT TARGET")
        );
        assert_eq!(action_selector(&sel(HYD_OBJECT, 5)).as_deref(), Some("5"));
        assert_eq!(action_selector(&obj(vec![])), None);
    }

    #[test]
    fn names_object_tells_fqns_from_names_and_numbers() {
        assert!(names_object("cdx.persons.smuggler.drooga_the_hutt"));
        assert!(names_object(
            "qst.daily_area.oricon.republic.seeds_of_change"
        ));
        assert!(!names_object("SDR_FoundCreature"));
        assert!(!names_object("671"));
        assert!(!names_object("-7306138348266915620"));
        assert!(!names_object("a:enc.location.tython.flesh_raider_leader"));
    }

    #[test]
    fn world_tables_exist_after_schema_init() {
        let path = temp_db_path("world_tables");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        let conn = db.conn.lock().unwrap();
        for table in [
            "area_log_ids",
            "placements",
            "placeable_grants",
            "trigger_volumes",
            "script_actions",
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table}");
        }
    }

    #[test]
    fn log_target_views_key_placements_by_log_ids() {
        let path = temp_db_path("log_target_views");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        let conn = db.conn.lock().unwrap();
        conn.execute_batch(
            "INSERT INTO area_log_ids VALUES (137438953533, 7);
             INSERT INTO placements (area_dir_id, room, instance_id, asset_path, entity_fqn,
                                     log_x, log_y, log_z)
                 VALUES (7, 'r', 1, 'a', 'plc.lore', 1.0, 2.0, 3.0),
                        (7, 'r', 2, 'b', 'npc.bela', 4.0, 5.0, 6.0),
                        (7, 'r', 3, 'c', 'plc.crate', 0.0, 0.0, 0.0),
                        (7, 'r', 4, 'd', 'plc.cache', 8.0, 0.0, 0.0);
             INSERT INTO placeable_grants VALUES
                 ('plc.lore', 'abl.placeables.examine.x', 811315027247104, 'cdx.lore.x', NULL, NULL),
                 ('plc.crate', NULL, NULL, 'cdx.crate', NULL, NULL),
                 ('plc.cache', 'abl.placeables.open.x', 811400926593024, NULL, NULL, 'hyd.cache');
             INSERT INTO script_actions (script_fqn, handler, ordinal, action, target_fqn, value) VALUES
                 ('hyd.cache', 'On Use', 0, 'Play Conversation', 'cnv.cache', 'cnv.cache'),
                 ('hyd.cache', 'On Enter', 0, 'Play Conversation', 'cnv.other', 'cnv.other');
             INSERT INTO objects (game_id, stable_id, payload_hash, guid, fqn, kind, string_id, json)
                 VALUES ('g', 's', 'h', 'G', 'npc.bela', 'Npc', 67878, '{}');
             INSERT INTO conversation_owners VALUES
                 ('npc.bela', 'npc', 'cnv.bela'), ('plc.lore', 'placeable', 'cnv.lore');",
        )
        .unwrap();
        let uses: Vec<(i64, i64, String, i64, Option<String>)> = conn
            .prepare(
                "SELECT log_area_id, use_ability_log_id, placeable_fqn, instance_id, codex_fqn \
                 FROM log_use_targets ORDER BY placeable_fqn",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // The crate plays no use ability, so a use cast cannot land on it.
        assert_eq!(
            uses,
            vec![
                (137438953533, 811400926593024, "plc.cache".into(), 4, None),
                (
                    137438953533,
                    811315027247104,
                    "plc.lore".into(),
                    1,
                    Some("cdx.lore.x".into())
                ),
            ]
        );
        let owners: Vec<(Option<i64>, String, String, f64)> = conn
            .prepare(
                "SELECT owner_log_id, owner_fqn, conversation_fqn, log_x \
                 FROM log_conversation_targets ORDER BY owner_fqn",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            owners,
            vec![
                (Some(67878 << 32), "npc.bela".into(), "cnv.bela".into(), 4.0),
                (None, "plc.cache".into(), "cnv.cache".into(), 8.0),
                (None, "plc.lore".into(), "cnv.lore".into(), 1.0),
            ]
        );
    }

    #[test]
    fn placeable_triggers_union_codex_conversation_on_use_and_quest_tasks() {
        let path = temp_db_path("placeable_triggers");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        let conn = db.conn.lock().unwrap();
        conn.execute_batch(
            "INSERT INTO placeable_grants VALUES
                 ('plc.lore', NULL, NULL, 'cdx.lore', 'cnv.lore', 'hyd.lore');
             INSERT INTO script_actions (script_fqn, handler, ordinal, action, target_fqn, value)
                 VALUES ('hyd.lore', 'On Use', 0, 'Play SFX', NULL, 'Play_x'),
                        ('hyd.lore', 'On Use', 1, 'Achievement Event', NULL, 'read_lore'),
                        ('hyd.lore', 'On Enter', 0, 'Grant Codex', 'cdx.other', 'cdx.other'),
                        ('hyd.lore', 'On Use', 2, 'Execute Hydra Script', NULL, 'hyd.called:Go'),
                        ('hyd.called', 'On Custom:Go', 0, 'Teleport To', NULL, 'dock'),
                        ('hyd.called', 'On Custom:Go', 1, 'Execute Hydra Script', NULL, 'hyd.lore:Back'),
                        ('hyd.lore', 'On Custom:Back', 0, 'Give Items', 'itm.loop', 'itm.loop');
             INSERT INTO objects (game_id, stable_id, payload_hash, guid, fqn, kind, json)
                 VALUES ('g', 's', 'h', 'G', 'plc.device', 'Placeable', '{}'),
                        ('g2', 's2', 'h2', 'G2', 'plc.core', 'Placeable', '{}');
             INSERT INTO placements (area_dir_id, room, instance_id, asset_path, spawner_fqn, entity_fqn)
                 VALUES (7, 'r', 1, 'a', 'spn.device', 'plc.device'),
                        (7, 'r', 2, 'b', 'spn.core', 'plc.core');
             INSERT INTO quest_tasks (quest_fqn, branch, step, task, text)
                 VALUES ('qst.q', 0, 5, 1, 'Plant a Device');
             INSERT INTO quest_task_hooks VALUES ('qst.q', 0, 5, 1, 77);
             INSERT INTO quest_task_objects VALUES
                 ('qst.q', 0, 5, 1, 'plc.device'), ('qst.q', 0, 5, 1, 'spn.device'),
                 ('qst.q', 0, 5, 1, 'spn.core');",
        )
        .unwrap();
        let rows: Vec<(String, String, String, String)> = conn
            .prepare("SELECT * FROM placeable_triggers ORDER BY placeable_fqn, kind, detail")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let row = |p: &str, k: &str, t: &str, d: &str| (p.into(), k.into(), t.into(), d.into());
        // Play SFX and the On Enter handler trigger nothing a use counts; the
        // On Use script's Execute Hydra Script reaches hyd.called's
        // 'On Custom:Go' (Teleport To), whose call back into hyd.lore is a
        // cycle and is not followed (no Give Items). A
        // task naming only the spawner (spn.core) reaches the placeable it
        // places; plc.device, named both ways, appears once.
        assert_eq!(
            rows,
            vec![
                row(
                    "plc.core",
                    "quest_task",
                    "qst.q",
                    "task (0,5,1) 'Plant a Device' hook 77"
                ),
                row(
                    "plc.device",
                    "quest_task",
                    "qst.q",
                    "task (0,5,1) 'Plant a Device' hook 77"
                ),
                row("plc.lore", "codex", "cdx.lore", "plcCodexSpec"),
                row("plc.lore", "conversation", "cnv.lore", "plcConvo"),
                row(
                    "plc.lore",
                    "on_use",
                    "read_lore",
                    "Achievement Event (hyd.lore On Use)"
                ),
                row(
                    "plc.lore",
                    "on_use",
                    "dock",
                    "Teleport To (hyd.called On Custom:Go)"
                ),
            ]
        );
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
