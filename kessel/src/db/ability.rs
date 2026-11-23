//! Ability, talent, discipline, and GSF (talent/ability/requisition) extraction.

use super::*;

impl Database {
    /// Fetch and base64-decode the payloads of canonical objects whose `fqn`
    /// matches `fqn_like` (an SQL LIKE pattern such as `"abl.%"`). Returns
    /// `(game_id, fqn, payload)` so each caller keys by whichever it needs;
    /// rows missing `payload_b64` or with undecodable base64 are skipped. This
    /// is the shared replacement for the SELECT-payload_b64 + BASE64.decode
    /// scaffold the per-prefix populate passes used to hand-roll (#316).
    pub(crate) fn object_payloads(&self, fqn_like: &str) -> Result<Vec<(String, String, Vec<u8>)>> {
        crate::queries::object_payloads(&self.conn.lock().unwrap(), fqn_like)
    }

    /// Add columns to `disciplines` populated from authoritative `dis.*`
    /// records (issue #170): the discipline's short codename
    /// (`power_pyrotech`), icon + mod-tree apc.* refs as game_ids, and the
    /// signature ability's game_id (`flaming_fist` for Pyrotech). Also creates
    /// the `discipline_mods` join table for the 8-tier x 3-choice mod tree
    /// with per-mod level gates and default-selection flags.
    ///
    /// Idempotent -- safe to re-run; checks pragma_table_info first.
    pub fn migrate_disciplines_from_dis_columns(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let existing: std::collections::HashSet<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(disciplines)")?;
            let cols = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            cols.into_iter().collect()
        };
        let additions = [
            ("codename", "TEXT"),
            ("icon_apc_game_id", "TEXT"),
            ("mod_tree_apc_game_id", "TEXT"),
            ("signature_ability_game_id", "TEXT"),
        ];
        for (name, ty) in additions {
            if !existing.contains(name) {
                let sql = format!("ALTER TABLE disciplines ADD COLUMN {name} {ty}");
                conn.execute(&sql, [])?;
            }
        }
        // 8 tiers x 3 choices = 24 mods per discipline. (discipline_fqn_prefix,
        // mod_index) is the PK; (discipline_fqn_prefix, tier_ordinal,
        // ui_position) is also unique but not enforced as a constraint.
        // `target_game_id` is NULL when the dis.* CF E0 ref doesn't resolve
        // (versioned-only ability category -- issue #179 investigates).
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS discipline_mods (
                discipline_fqn_prefix     TEXT NOT NULL,
                mod_index                 INTEGER NOT NULL,
                tier_ordinal              INTEGER NOT NULL,
                ui_position               INTEGER NOT NULL,
                level_required            INTEGER NOT NULL,
                target_guid               TEXT NOT NULL,
                target_game_id            TEXT,
                is_default                INTEGER NOT NULL,
                PRIMARY KEY (discipline_fqn_prefix, mod_index),
                FOREIGN KEY (discipline_fqn_prefix) REFERENCES disciplines(fqn_prefix)
            );
            "#,
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_discipline_mods_target ON discipline_mods(target_game_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_discipline_mods_tier ON discipline_mods(discipline_fqn_prefix, tier_ordinal)",
            [],
        )?;
        Ok(())
    }

    /// Populate `ability_stats` from `abl.*` GOM payloads (#74, refines #69).
    ///
    /// The dominant ability template (~86% of abl.* objects) writes properties
    /// as a contiguous run of 6-byte `[u16 LE prop_id][f32 LE value]` records
    /// starting at the sentinel `01 04 00 00 80 BF` (0x0401 = -1.0, an uninit
    /// marker). The block ends where the next 2 bytes are not in the 0x04xx
    /// range. Walking only that block eliminates the false positives the
    /// brute-force v1 scan produced (e.g. spurious force_cost=1 on warrior
    /// abilities from bytes outside the prop block).
    ///
    /// `resource_pool` is derived from the FQN class segment, not from the
    /// payload. Cost columns apply a value threshold (Force cost >= 5,
    /// resource cost >= 1) since 0x0403 is reused at low values as a scaling
    /// coefficient on tech abilities.
    ///
    /// Abilities on the secondary template (4000000002754EE0, ~459 rows
    /// including shock, endure_pain, takedown, companion abilities, racials,
    /// abl.space_combat.* on-rails missions, and abl.spvp.* Galactic
    /// Starfighter) have no sentinel-anchored block; they get a row only if
    /// their FQN resolves a `resource_pool` and no stats fields populate.
    pub fn populate_ability_stats(&self) -> Result<u64> {
        crate::queries::ability::populate_ability_stats(&self.conn.lock().unwrap())
    }

    /// Populate `talent_details` from `tal.*` payloads (#70).
    ///
    /// Three pieces of metadata per talent:
    ///   - `resource_pool` derived from FQN class (rage/focus/force/heat/
    ///     ammo/energy/gsf) — same vocabulary as `ability_stats`
    ///   - `tier` is the FQN's last segment (tier1, tier_3a, base, passive)
    ///   - `script_hook` is the length-prefixed ASCII tail string at the
    ///     end of the payload, identifying the ability mod the talent
    ///     triggers. ~94% of talents have one per vault MAPPINGS.md.
    ///
    /// A row is written for every `tal.*` object — the columns are NULL
    /// when not derivable.
    pub fn populate_talent_details(&self) -> Result<u64> {
        crate::queries::ability::populate_talent_details(&self.conn.lock().unwrap())
    }

    /// Populate `gsf_talent_stats` from `tal.spvp.*` payloads (#80).
    ///
    /// GSF talent stat values are encoded distinctly from ground-ability
    /// `0x04xx` records -- they live as `[c9 01]? <stat_id> 01 04 <f32 LE>`
    /// records terminated by the signature `cb 19 d7 4b ?? 03`. The existing
    /// ability-stat extractor anchors on a sentinel that GSF talents do not
    /// carry, which is why GSF stat values were absent from spice.sqlite
    /// despite the talents themselves being present.
    ///
    /// Empirically, 250/350 GSF talents (71%) carry at least one record;
    /// 100 talents are flag-only (effects implemented on the parent ability
    /// or via script hook).
    pub fn populate_gsf_talent_stats(&self) -> Result<u64> {
        crate::queries::ability::populate_gsf_talent_stats(&self.conn.lock().unwrap())
    }

    /// Populate `gsf_ability_stats` from `abl.spvp.*` payloads (#78).
    ///
    /// GSF base abilities reuse the ground `[u16 LE prop_id][f32 LE value]`
    /// layout but skip the `01 04 00 00 80 BF` cooldown sentinel that anchors
    /// `scan_ability_props`, and they scatter records across the payload
    /// rather than packing them contiguously. The decoder walks every 6-byte
    /// window and emits any record whose value is finite, non-zero, and in a
    /// reasonable magnitude range (subnormal-ish and huge values are
    /// byte-alignment noise from GUID/hash bytes).
    ///
    /// Empirical coverage: 112/131 abl.spvp.* abilities (85%) emit at least
    /// one record. Uncovered abilities are passive auras whose effects live
    /// on a parent activator or in script hooks.
    pub fn populate_gsf_ability_stats(&self) -> Result<u64> {
        crate::queries::ability::populate_gsf_ability_stats(&self.conn.lock().unwrap())
    }

    /// Populate `ability_desc_tokens` -- the type of each `<<N>>` token in every
    /// ability's description, decoded from the payload SpecParam list (#309).
    /// Returns (abilities_with_tokens, total_tokens).
    pub fn populate_ability_desc_tokens(&self) -> Result<(u64, u64)> {
        use crate::schema::desc_tokens::decode_desc_tokens;

        let payloads = self.object_payloads("abl.%")?;

        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_desc_tokens \
             (ability_fqn, token_index, token_type) VALUES (?1, ?2, ?3)",
        )?;
        let (mut abilities, mut tokens) = (0u64, 0u64);
        for (_game_id, fqn, payload) in &payloads {
            let decoded = decode_desc_tokens(payload);
            if decoded.is_empty() {
                continue;
            }
            abilities += 1;
            for t in decoded {
                stmt.execute(params![fqn, t.index as i64, t.token_type])?;
                tokens += 1;
            }
        }
        drop(stmt);
        tx.commit()?;
        Ok((abilities, tokens))
    }

    /// Populate `disciplines` and `discipline_abilities` from `abl.{class}.skill.{discipline}.*` FQNs.
    ///
    /// Discipline FQN structure:
    ///   abl.{class}.skill.{discipline}.{name}              -> base/core ability
    ///   abl.{class}.skill.{discipline}.mods.passive.{name} -> passive
    ///   abl.{class}.skill.{discipline}.mods.tier2.{name}   -> choice (lvl 23)
    ///   abl.{class}.skill.{discipline}.mods.tier3.{name}   -> choice (lvl 39+)
    ///   abl.{class}.skill.{discipline}.mods.special.{name} -> special
    ///   abl.{class}.skill.utility.{name}                   -> utility (shared)
    ///   abl.{class}.skill.mods.tier1.{name}                -> shared mod
    ///   abl.{class}.{name}                                 -> CLASS-SHARED, fanned out to every discipline of {class}
    ///                                                        (e.g. abl.jedi_knight.saber_reflect, abl.jedi_knight.force_leap)
    pub fn populate_disciplines(&self) -> Result<(u64, u64, u64)> {
        self.flush()?;
        crate::queries::ability::populate_disciplines(&self.conn.lock().unwrap())
    }

    /// Populate authoritative discipline data from `dis.*` PBUK records
    /// (issue #170). Updates the new columns on `disciplines` (codename,
    /// icon_apc_game_id, mod_tree_apc_game_id, signature_ability_game_id)
    /// and fills `discipline_mods` with the 8-tier x 3-choice mod tree per
    /// docs/probes/dis-payload-format.md.
    ///
    /// Returns (disciplines_updated, mods_inserted).
    pub fn populate_disciplines_from_dis(&self) -> Result<(u64, u64)> {
        self.flush()?;
        crate::queries::ability::populate_disciplines_from_dis(&self.conn.lock().unwrap())
    }

    /// Populate `gsf_requisition_costs` from the GSF cost singletons
    /// (`scFFComponentsCostPrototype` for unlock costs and
    /// `scFFComponentUpgradesCostPrototype` for per-tier upgrade costs).
    /// Closes #115; first per-singleton decoder on top of the #171
    /// singleton pipeline.
    ///
    /// Returns (component_unlock_rows, tier_upgrade_rows).
    pub fn populate_gsf_requisition_costs(&self) -> Result<(u64, u64)> {
        use crate::schema::gsf_costs::{decode_component_upgrades_cost, decode_components_cost};
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        // Load the two singleton payloads from the singletons table.
        let payloads: std::collections::HashMap<String, Vec<u8>> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT fqn, payload_b64 FROM singletons \
                 WHERE fqn IN ('scFFComponentsCostPrototype', 'scFFComponentUpgradesCostPrototype')",
            )?;
            let rows: std::collections::HashMap<String, Vec<u8>> = stmt
                .query_map([], |row| {
                    let fqn: String = row.get(0)?;
                    let b64: String = row.get(1)?;
                    Ok((fqn, b64))
                })?
                .filter_map(|r| r.ok())
                .filter_map(|(fqn, b64)| BASE64.decode(&b64).ok().map(|b| (fqn, b)))
                .collect();
            rows
        };

        // Resolve target GUIDs to game_ids + fqns up front (avoid per-row SQL
        // in the hot loop).
        let guid_to_object: std::collections::HashMap<String, (String, String)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt =
                conn.prepare("SELECT guid, game_id, fqn FROM objects WHERE guid IS NOT NULL")?;
            let rows: std::collections::HashMap<String, (String, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        (row.get::<_, String>(1)?, row.get::<_, String>(2)?),
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut component_rows = 0u64;
        let mut tier_rows = 0u64;

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO gsf_requisition_costs \
                   (target_guid, cost_kind, tier, cost, target_game_id, target_fqn) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut insert_costs =
                |costs: Vec<crate::schema::gsf_costs::GsfCost>, counter: &mut u64| -> Result<()> {
                    for cost in costs {
                        let resolved = guid_to_object.get(&cost.target_guid);
                        insert.execute(params![
                            cost.target_guid,
                            cost.kind.as_sql(),
                            cost.tier,
                            cost.cost,
                            resolved.map(|(gid, _)| gid.as_str()),
                            resolved.map(|(_, fqn)| fqn.as_str()),
                        ])?;
                        *counter += 1;
                    }
                    Ok(())
                };

            if let Some(payload) = payloads.get("scFFComponentsCostPrototype") {
                insert_costs(decode_components_cost(payload)?, &mut component_rows)?;
            }
            if let Some(payload) = payloads.get("scFFComponentUpgradesCostPrototype") {
                insert_costs(decode_component_upgrades_cost(payload)?, &mut tier_rows)?;
            }
        }
        tx.commit()?;
        Ok((component_rows, tier_rows))
    }

    /// Populate `gsf_ships` -- the GSF premium starter-ship roster, the 10
    /// `itm.spvp.ships.premium.*` objects with display name, faction, and class.
    /// Pure relational pass (no byte decode); the ship -> loadout-template
    /// binding is client-side and not stored in these payloads. Issue #115
    /// lineage. Returns the row count.
    pub fn populate_gsf_ships(&self) -> Result<u64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut rows = 0u64;
        {
            // (fqn, game_id, name, string_id) for the 10 premium ships.
            let ships: Vec<(String, String, Option<String>)> = {
                let mut stmt = tx.prepare(
                    "SELECT o.fqn, o.game_id, \
                            (SELECT s.text FROM strings s \
                              WHERE s.id2 = o.string_id AND s.locale = 'en-us' AND s.id1 = 0 \
                              LIMIT 1) AS name \
                       FROM objects o \
                      WHERE o.is_canonical = 1 \
                        AND o.fqn LIKE 'itm.spvp.ships.premium.%'",
                )?;
                let ships: Vec<(String, String, Option<String>)> = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                ships
            };

            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO gsf_ships \
                   (fqn, game_id, name, faction, ship_class) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (fqn, game_id, name) in ships {
                // Segment after 'itm.spvp.ships.premium.', e.g. 'imp_gunship_02'.
                let seg = fqn.rsplit('.').next().unwrap_or("").to_string();
                let (faction, rest) = if let Some(r) = seg.strip_prefix("imp_") {
                    (Some("empire"), r)
                } else if let Some(r) = seg.strip_prefix("rep_") {
                    (Some("republic"), r)
                } else {
                    (None, seg.as_str())
                };
                // Class = `rest` minus a trailing `_NN` variant suffix.
                let ship_class = rest
                    .rsplit_once('_')
                    .map(|(c, _)| c)
                    .unwrap_or(rest)
                    .to_string();
                insert.execute(params![fqn, game_id, name, faction, ship_class,])?;
                rows += 1;
            }
        }
        tx.commit()?;
        Ok(rows)
    }

    /// Populate `gsf_loadout_slots` from the `conSpec_scff_equip_*` slot-template
    /// singletons. One row per distinct component slot a template declares.
    /// Issue #115 lineage. Returns the row count.
    pub fn populate_gsf_loadout_slots(&self) -> Result<u64> {
        use crate::schema::gsf_loadout::decode_loadout_slots;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        // Load every loadout-template singleton payload.
        let templates: Vec<(String, Vec<u8>)> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT fqn, payload_b64 FROM singletons \
                  WHERE fqn LIKE 'conSpec_scff_equip_%'",
            )?;
            let templates: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .filter_map(|(fqn, b64)| BASE64.decode(&b64).ok().map(|b| (fqn, b)))
                .collect();
            templates
        };

        let mut rows = 0u64;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO gsf_loadout_slots \
                   (template_code, slot_kind, slot_type, slot_ordinal) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (fqn, payload) in templates {
                let template_code = fqn
                    .strip_prefix("conSpec_scff_equip_")
                    .unwrap_or(&fqn)
                    .to_string();
                let slot_kind = if template_code.starts_with("maj_") {
                    "major"
                } else if template_code.starts_with("min_") {
                    "minor"
                } else {
                    continue; // skip quickslot / other shapes (no slots)
                };
                for slot in decode_loadout_slots(&payload) {
                    insert.execute(params![
                        template_code,
                        slot_kind,
                        slot.slot_type,
                        slot.slot_ordinal,
                    ])?;
                    rows += 1;
                }
            }
        }
        tx.commit()?;
        Ok(rows)
    }

    /// Resolve `gsf_requisition_costs.target_guid` to art_path + component_kind
    /// via the `data` singleton (#217). Each cost target_guid (8 bytes) is
    /// `<6-byte content_guid_tail><0x04><0x03>`. The 6-byte tail appears in
    /// the `data` singleton next to a length-prefixed ASCII art path like
    /// `art/dynamic/space_pvp/ships/imp_scout/sweapon/imp_scout_a_sweapon_03.gr2`.
    ///
    /// component_kind is derived from the art path: ship faction prefix
    /// (imp_/rep_/spvp_neu_), ship class, slot (sweapon/pweapon/engine/
    /// shield/reactor/sensors/etc.), variant letter, and tier number.
    ///
    /// Returns rows updated.
    pub fn populate_gsf_cost_targets(&self) -> Result<u64> {
        self.flush()?;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let data_payload: Option<Vec<u8>> = {
            let conn = self.conn.lock().unwrap();
            let row: Option<String> = conn
                .query_row(
                    "SELECT payload_b64 FROM singletons WHERE fqn = 'data'",
                    [],
                    |r| r.get(0),
                )
                .ok();
            row.and_then(|b64| BASE64.decode(b64).ok())
        };
        let Some(data_payload) = data_payload else {
            return Ok(0);
        };

        // Walk the data singleton: for each CF E0 00 marker, take the next
        // 6 bytes as the content_guid_tail and the nearest following ASCII
        // string (>=8 chars containing an art-path hint) as the art_path.
        let mut guid_to_path: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut i = 0;
        while i + 9 <= data_payload.len() {
            if data_payload[i] == 0xCF && data_payload[i + 1] == 0xE0 && data_payload[i + 2] == 0x00
            {
                let tail = hex::encode_upper(&data_payload[i + 3..i + 9]);
                // Scan up to 300 bytes ahead for an art-path-shaped ASCII run.
                let scan_end = (i + 300).min(data_payload.len());
                let mut buf: Vec<u8> = Vec::new();
                let mut found_path: Option<String> = None;
                for &b in &data_payload[(i + 9)..scan_end] {
                    if (0x20..0x7F).contains(&b) {
                        buf.push(b);
                    } else {
                        if buf.len() >= 8 {
                            if let Ok(s) = std::str::from_utf8(&buf) {
                                if s.contains("art/")
                                    || s.contains(".gr2")
                                    || s.contains("space_pvp")
                                    || s.contains(".fxspec")
                                {
                                    // Strip leading non-art length-prefix byte
                                    let clean = s
                                        .find("art/")
                                        .or_else(|| s.find("space_pvp"))
                                        .map(|idx| &s[idx..])
                                        .unwrap_or(s)
                                        .to_string();
                                    found_path = Some(clean);
                                    break;
                                }
                            }
                        }
                        buf.clear();
                    }
                }
                if let Some(p) = found_path {
                    guid_to_path.entry(tail).or_insert(p);
                }
                i += 9;
            } else {
                i += 1;
            }
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut updated = 0u64;
        {
            let rows: Vec<(String, String, i64)> = {
                let mut stmt =
                    tx.prepare("SELECT target_guid, cost_kind, tier FROM gsf_requisition_costs")?;
                let collected: Vec<(String, String, i64)> = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                collected
            };
            let mut update = tx.prepare_cached(
                "UPDATE gsf_requisition_costs \
                    SET art_path = ?1, component_kind = ?2 \
                  WHERE target_guid = ?3 AND cost_kind = ?4 AND tier = ?5",
            )?;
            for (target_guid, kind, tier) in &rows {
                // target_guid is 16 hex chars; tail = first 12 chars (6 bytes)
                let tail = &target_guid[..12.min(target_guid.len())];
                if let Some(path) = guid_to_path.get(tail) {
                    let component_kind = derive_gsf_component_kind(path);
                    update.execute(params![path, component_kind, target_guid, kind, tier])?;
                    updated += 1;
                }
            }
        }
        tx.commit()?;
        Ok(updated)
    }

    /// Populate `ability_effect_blocks` from abl.* + tal.* payloads (#173).
    ///
    /// Walks every canonical ability/talent payload for indexed CF E0
    /// effect-block references and writes one row per indexed ref. The
    /// parent self-reference (first CF E0 marker in the payload) is
    /// skipped by the decoder. Unresolved block GUIDs (versioned-only
    /// ability category, #179) leave `block_game_id` NULL but keep the raw
    /// GUID for downstream visibility.
    ///
    /// Returns (rows_written, unresolved_count).
    pub fn populate_ability_effect_blocks(&self) -> Result<(u64, u64)> {
        use crate::schema::effect_block::extract_effect_block_refs;
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let conn = self.conn.lock().unwrap();

        let payloads: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT game_id, json_extract(json, '$.payload_b64') \
                 FROM objects \
                 WHERE (fqn LIKE 'abl.%' OR fqn LIKE 'tal.%') AND is_canonical = 1",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let guid_to_game_id: std::collections::HashMap<String, String> = {
            let mut stmt =
                conn.prepare("SELECT guid, game_id FROM objects WHERE guid IS NOT NULL")?;
            let rows: std::collections::HashMap<String, String> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_effect_blocks \
               (parent_game_id, block_index, block_guid, block_game_id) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;

        let mut written: u64 = 0;
        let mut unresolved: u64 = 0;
        for (parent_game_id, payload_b64) in &payloads {
            let Ok(payload) = BASE64.decode(payload_b64) else {
                continue;
            };
            for r in extract_effect_block_refs(&payload) {
                let resolved = guid_to_game_id.get(&r.block_guid);
                if resolved.is_none() {
                    unresolved += 1;
                }
                insert.execute(params![
                    parent_game_id,
                    r.block_index,
                    r.block_guid,
                    resolved,
                ])?;
                written += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((written, unresolved))
    }

    /// Populate `ability_damage_params` from effAction_Damage CC parameters.
    ///
    /// Damage actions encode 6 core parameters (and 2 optional) via CC
    /// markers inside the action tail: `CC + 4-byte ID + 1-byte i8`.
    /// CC ID names are unknown (Bioware hash namespace, #144 spike); values
    /// are surfaced as raw bytes for downstream consumer use.
    ///
    /// Returns (abilities_with_damage_params, total_rows).
    pub fn populate_ability_damage_params(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::{BTreeMap, HashSet};

        const E251D1CC: [u8; 4] = [0xE2, 0x51, 0xD1, 0xCC];
        // The 8 CC IDs known to live inside Damage action tails (6 core + 2
        // optional). Filter to these to avoid noise from random 0xCC bytes
        // in surrounding markers.
        let damage_cc_ids: HashSet<[u8; 4]> = [
            [0x01, 0x35, 0xC0, 0xE0],
            [0x01, 0x74, 0x59, 0xAB],
            [0x39, 0x28, 0x54, 0x72],
            [0x0B, 0xB0, 0xD0, 0x6E],
            [0x01, 0x76, 0xE2, 0x1B],
            [0x01, 0x1A, 0x6E, 0x3E],
            [0x3C, 0x0E, 0xB2, 0x3D],
            [0x0B, 0x9B, 0xBB, 0xDA],
        ]
        .into_iter()
        .collect();

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'abl.%'",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        // Aggregate per (FQN, effect_ordinal) preserving order; dedup
        // identical (effect_ordinal, cc_id, value) tuples across variants.
        let mut params_per_fqn: BTreeMap<String, Vec<(i64, [u8; 4], i8)>> = BTreeMap::new();

        for (fqn, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };

            // Find each effAction_Damage marker (E251D1CC tag with enum_index 3).
            let mut effect_ord: i64 = 0;
            let mut i = 0;
            while i + 11 <= payload.len() {
                let is_marker = payload[i] == 0xCF
                    && payload[i + 1] == 0x40
                    && payload[i + 2] == 0x00
                    && payload[i + 3] == 0x00
                    && payload[i + 5..i + 9] == E251D1CC
                    && payload[i + 9] == 0x05
                    && payload[i + 10] == 0x03; // effAction_Damage = index 3
                if !is_marker {
                    i += 1;
                    continue;
                }
                // Tail to next CF40 or end-of-payload
                let tail_start = i + 11;
                let mut tail_end = payload.len();
                let mut k = tail_start;
                while k + 4 <= payload.len() {
                    if payload[k] == 0xCF
                        && payload[k + 1] == 0x40
                        && payload[k + 2] == 0x00
                        && payload[k + 3] == 0x00
                    {
                        tail_end = k;
                        break;
                    }
                    k += 1;
                }
                let tail = &payload[tail_start..tail_end];
                // Scan tail for CC markers matching known damage CC IDs.
                let mut t = 0;
                while t + 6 <= tail.len() {
                    if tail[t] == 0xCC {
                        let cc_id: [u8; 4] = tail[t + 1..t + 5].try_into().unwrap();
                        if damage_cc_ids.contains(&cc_id) {
                            let val = tail[t + 5] as i8;
                            params_per_fqn
                                .entry(fqn.clone())
                                .or_default()
                                .push((effect_ord, cc_id, val));
                            t += 6;
                            continue;
                        }
                    }
                    t += 1;
                }
                effect_ord += 1;
                i += 11;
            }
        }

        // Dedup preserving first-seen order
        for v in params_per_fqn.values_mut() {
            let mut seen = HashSet::new();
            v.retain(|t| seen.insert(*t));
        }

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_damage_params \
               (ability_fqn, effect_ordinal, param_ordinal, cc_id_hex, value_i8) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut abilities = 0u64;
        let mut total = 0u64;
        for (fqn, list) in &params_per_fqn {
            if list.is_empty() {
                continue;
            }
            abilities += 1;
            let mut counters: std::collections::HashMap<i64, i64> =
                std::collections::HashMap::new();
            for (eff_ord, cc_id, val) in list {
                let p_ord = counters.entry(*eff_ord).or_insert(0);
                let cc_hex = hex::encode_upper(cc_id);
                insert.execute(params![fqn, eff_ord, *p_ord, cc_hex, *val as i64])?;
                *p_ord += 1;
                total += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((abilities, total))
    }

    /// Populate `ability_action_params` from effAction parameter arrays.
    ///
    /// Each effAction (CF40 E251D1CC) marker is followed by a sequence of
    /// typed property records. Numeric (f32) parameters appear in records
    /// of the form `[01|07] 08 05 04 <count_a:u8> <count_b:u8>
    /// <count_a × (key_u8, f32_LE)>` -- a value-tag-04 (float32) array
    /// keyed by an enum_ref (effParam by convention). This populator scans
    /// each effAction's tail (up to the next CF40 marker), collects every
    /// such f32 record found, and inserts each (key, value) pair.
    ///
    /// Captures float parameters across many action types
    /// (BallisticImpulse, Heal, WeaponDamage, Stun, EnvironmentalDamage,
    /// etc.). Int8/int16 parameter shapes have additional framing
    /// variance and are deferred to a future populator.
    ///
    /// Returns (abilities_with_params, total_param_rows).
    pub fn populate_ability_action_params(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::BTreeMap;

        const E251D1CC: [u8; 4] = [0xE2, 0x51, 0xD1, 0xCC];
        let effparam_enum = crate::gom_schema::enum_for_name("effParam");

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'abl.%'",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        // Aggregate params per (FQN, effect_ordinal) preserving discovery order
        // and dedup across variants by exact (effect_ordinal, effparam_idx, value).
        let mut params_per_fqn: BTreeMap<String, Vec<(i64, u8, f32)>> = BTreeMap::new();

        for (fqn, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };

            // Find each E251D1CC effAction marker, ordinal = index of action.
            let mut effect_ord: i64 = 0;
            let mut i = 0;
            while i + 9 <= payload.len() {
                let is_marker = payload[i] == 0xCF
                    && payload[i + 1] == 0x40
                    && payload[i + 2] == 0x00
                    && payload[i + 3] == 0x00;
                if !is_marker || payload[i + 5..i + 9] != E251D1CC {
                    i += 1;
                    continue;
                }
                // Tail to next CF40 marker (the next typed property record).
                let tail_start = i + 9;
                let mut tail_end = payload.len();
                let mut k = tail_start + 9;
                while k + 4 <= payload.len() {
                    if payload[k] == 0xCF
                        && payload[k + 1] == 0x40
                        && payload[k + 2] == 0x00
                        && payload[k + 3] == 0x00
                    {
                        tail_end = k;
                        break;
                    }
                    k += 1;
                }
                let tail = &payload[tail_start..tail_end];

                // Scan tail for `[01|07] 08 05 04 <c1> <c2> <c1 × (u8,f32)>`
                // records. Multiple records may appear per effAction; capture
                // every one. The `[01|07] 08 05` prefix is the typed-property
                // wrapper, 04 is the f32 value-tag, and c1 is the item count.
                let mut p = 0;
                while p + 6 <= tail.len() {
                    let wrapper_ok = tail[p] == 0x01 || tail[p] == 0x07;
                    if !wrapper_ok
                        || tail[p + 1] != 0x08
                        || tail[p + 2] != 0x05
                        || tail[p + 3] != 0x04
                    {
                        p += 1;
                        continue;
                    }
                    let count = tail[p + 4] as usize;
                    // Sanity: count_b must equal count_a in every verified
                    // sample. Skip mismatches rather than misframe the array.
                    if count == 0 || count > 32 || tail[p + 5] != tail[p + 4] {
                        p += 1;
                        continue;
                    }
                    let items_start = p + 6;
                    let items_end = items_start + count * 5;
                    if items_end > tail.len() {
                        p += 1;
                        continue;
                    }
                    let mut q = items_start;
                    let mut all_good = true;
                    let mut pairs: Vec<(u8, f32)> = Vec::with_capacity(count);
                    for _ in 0..count {
                        let idx = tail[q];
                        let f = f32::from_le_bytes(tail[q + 1..q + 5].try_into().unwrap());
                        if !f.is_finite() || f.abs() > 1e9 {
                            all_good = false;
                            break;
                        }
                        pairs.push((idx, f));
                        q += 5;
                    }
                    if all_good {
                        let entry = params_per_fqn.entry(fqn.clone()).or_default();
                        for (idx, f) in pairs {
                            entry.push((effect_ord, idx, f));
                        }
                        p = items_end;
                    } else {
                        p += 1;
                    }
                }
                effect_ord += 1;
                i += 9;
            }
        }

        // Dedup per FQN (since variants overlap) preserving first-seen order.
        for v in params_per_fqn.values_mut() {
            let mut seen = std::collections::HashSet::new();
            v.retain(|t| {
                let key = (t.0, t.1, t.2.to_bits());
                seen.insert(key)
            });
        }

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_action_params \
               (ability_fqn, effect_ordinal, param_ordinal, effparam_index, effparam_name, value_f32) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut abilities = 0u64;
        let mut total = 0u64;
        for (fqn, params) in &params_per_fqn {
            if params.is_empty() {
                continue;
            }
            abilities += 1;
            // Group by effect_ordinal to assign per-effect param_ordinal
            let mut counters: std::collections::HashMap<i64, i64> =
                std::collections::HashMap::new();
            for (eff_ord, idx, val) in params {
                let p_ord = counters.entry(*eff_ord).or_insert(0);
                let name = effparam_enum.and_then(|e| e.members.get(*idx as usize).cloned());
                insert.execute(params![
                    fqn,
                    eff_ord,
                    *p_ord,
                    *idx as i64,
                    name,
                    *val as f64
                ])?;
                *p_ord += 1;
                total += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((abilities, total))
    }

    /// Populate `object_cc_refs` by walking every canonical PBUK object
    /// payload for CC marker bytes whose 4-byte ID matches a KNOWN CC ID.
    /// Filters to known IDs only because a naive "any 0xCC byte" scan
    /// produces 19.5M rows with ~96% false positives (0xCC appears
    /// frequently inside GUID payloads + value bytes that don't open a
    /// marker). Restricting to known IDs gives ~827K real records.
    ///
    /// Per-CC-ID grammar (value length, value type) is not yet decoded
    /// across the corpus; this captures up to 16 sample value bytes per
    /// occurrence so the data is at least visible in spice. As new CC
    /// IDs are identified (#144 hash crack or per-byte-search work),
    /// expand the `known` table below.
    ///
    /// Returns (objects_with_cc_refs, total_cc_records).
    pub fn populate_object_cc_refs(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::HashMap;

        // Known CC IDs (LE order, as stored in payloads).
        let known: HashMap<&[u8; 4], &str> = [
            (b"\x37\xAE\x6F\x6F" as &[u8; 4], "stringRef"),
            (b"\x0B\x84\xE2\x17", "abilityRef"),
            (b"\x03\xDD\xAF\xE4", "unknown_E4AFDD03"),
            (b"\x2D\x31\xCD\x0C", "unknown_0CCD312D"),
            (b"\x19\xD7\x4B\x9D", "unknown_9D4BD719"),
            (b"\x19\xD7\x4B\x96", "unknown_964BD719"),
        ]
        .into_iter()
        .collect();

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT game_id, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE is_canonical = 1",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO object_cc_refs \
               (object_game_id, ordinal, cc_id_hex, cc_known_name, value_bytes_hex) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut objects = 0u64;
        let mut total = 0u64;
        for (game_id, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };
            let mut ordinal: i64 = 0;
            let mut i = 0;
            while i + 5 <= payload.len() {
                if payload[i] != 0xCC {
                    i += 1;
                    continue;
                }
                let cc_id_bytes: [u8; 4] = payload[i + 1..i + 5].try_into().unwrap();
                let Some(known_name) = known.get(&cc_id_bytes).map(|s| s.to_string()) else {
                    i += 1;
                    continue;
                };
                let cc_id_hex = hex::encode_upper(cc_id_bytes);
                // Capture up to 16 sample value bytes, stopping at the next
                // recognized marker family byte (CB/CC/CD/CE/CF) to avoid
                // spilling into the next record.
                let value_start = i + 5;
                let mut value_end = (value_start + 16).min(payload.len());
                for (offset, &byte) in payload[value_start..value_end].iter().enumerate() {
                    if (0xCB..=0xCF).contains(&byte) {
                        value_end = value_start + offset;
                        break;
                    }
                }
                let value_hex = hex::encode_upper(&payload[value_start..value_end]);
                insert.execute(params![game_id, ordinal, cc_id_hex, &known_name, value_hex])?;
                ordinal += 1;
                total += 1;
                // Advance past the marker only; per-ID length grammar would
                // let us skip the full value cleanly, but for now stepping
                // by 5 (CC + 4-byte ID) is safe since the next CC byte
                // inside this value would be re-detected as a new marker.
                i += 5;
            }
            if ordinal > 0 {
                objects += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((objects, total))
    }

    /// Populate `ability_effect_params` by extracting every `04 01 01 <flag>
    /// <float32_LE>` triplet inside the E251D1CE/CF parameter-list tails of
    /// every abl.* payload. Aggregates per FQN across canonical and
    /// non-canonical variants (rich parameter data lives on the longer
    /// non-canonical variant per the PR #174 tags pattern).
    ///
    /// Returns (abilities_with_params, total_param_triplets).
    pub fn populate_ability_effect_params(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::BTreeMap;

        const E251D1CE: [u8; 4] = [0xE2, 0x51, 0xD1, 0xCE];
        const E251D1CF: [u8; 4] = [0xE2, 0x51, 0xD1, 0xCF];

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'abl.%'",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        // Aggregate per FQN. Dedup identical (source, flag, value) tuples
        // since the same parameter may appear in multiple variant payloads.
        let mut params_per_fqn: BTreeMap<String, Vec<(&'static str, u8, f32)>> = BTreeMap::new();

        for (fqn, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };
            // Walk CF40 markers and inspect E251D1CE / E251D1CF tails.
            let mut i = 0;
            while i + 9 <= payload.len() {
                let is_marker = payload[i] == 0xCF
                    && payload[i + 1] == 0x40
                    && payload[i + 2] == 0x00
                    && payload[i + 3] == 0x00;
                if !is_marker {
                    i += 1;
                    continue;
                }
                let target = &payload[i + 5..i + 9];
                let source: Option<&'static str> = if target == E251D1CE {
                    Some("CE")
                } else if target == E251D1CF {
                    Some("CF")
                } else {
                    None
                };
                let Some(source) = source else {
                    i += 9;
                    continue;
                };
                // Tail extends until the next CF40 marker (or EOF).
                let tail_start = i + 9;
                let mut tail_end = payload.len();
                let mut k = tail_start;
                while k + 4 <= payload.len() {
                    if payload[k] == 0xCF
                        && payload[k + 1] == 0x40
                        && payload[k + 2] == 0x00
                        && payload[k + 3] == 0x00
                    {
                        tail_end = k;
                        break;
                    }
                    k += 1;
                }
                let tail = &payload[tail_start..tail_end];
                // Scan tail for `04 01 01 <flag> <f32_LE>` triplets.
                let mut t = 0;
                while t + 8 <= tail.len() {
                    if tail[t] == 0x04 && tail[t + 1] == 0x01 && tail[t + 2] == 0x01 {
                        let flag = tail[t + 3];
                        let bytes: [u8; 4] = tail[t + 4..t + 8].try_into().unwrap();
                        let val = f32::from_le_bytes(bytes);
                        if val.is_finite() && (val.abs() < 1e6 || val == 0.0) {
                            params_per_fqn
                                .entry(fqn.clone())
                                .or_default()
                                .push((source, flag, val));
                        }
                        t += 8;
                    } else {
                        t += 1;
                    }
                }
                i += 9;
            }
        }

        // Dedup per FQN preserving order.
        for v in params_per_fqn.values_mut() {
            let mut seen = std::collections::HashSet::new();
            v.retain(|t| {
                let key = (t.0, t.1, t.2.to_bits());
                seen.insert(key)
            });
        }

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_effect_params \
               (ability_fqn, ordinal, source, param_flag, value_f32) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut abilities = 0u64;
        let mut total = 0u64;
        for (fqn, list) in &params_per_fqn {
            if list.is_empty() {
                continue;
            }
            abilities += 1;
            for (ord, (src, flag, val)) in list.iter().enumerate() {
                insert.execute(params![fqn, ord as i64, src, *flag as i64, *val as f64])?;
                total += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((abilities, total))
    }

    /// Populate `ability_effects` by walking every abl.* payload (canonical
    /// and non-canonical, since the rich effect data lives on the longer
    /// non-canonical variant per PR #174 tags) for the four effect-record
    /// marker hi32s. Aggregates per FQN so consumers see one stable set of
    /// effects per ability identity, regardless of which variant happens
    /// to be canonical.
    ///
    /// Returns (abilities_with_effects, total_effects_inserted).
    pub fn populate_ability_effects(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::BTreeMap;

        // The four effect-record marker hi32s + their kind label + enum name.
        const MARKERS: &[(u32, &str, &str)] = &[
            (0xE251D1CC, "action", "effAction"),
            (0xE251D1CD, "condition", "effCondition"),
            (0xE251D1D0, "initializer", "effInitializer"),
            (0xE71E2F92, "logic_op", "effLogicOp"),
        ];

        // Resolve enum once
        let mut enums: Vec<(u32, &str, Option<&crate::gom_schema::GomEnum>)> = Vec::new();
        for (h, k, name) in MARKERS {
            enums.push((*h, k, crate::gom_schema::enum_for_name(name)));
        }

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'abl.%'",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        // Aggregate across canonical + non-canonical variants per FQN.
        // Key: ability_fqn. Value: ordered list of (kind, enum_index, member).
        // Use a deduping set to avoid repeats when the same variant is seen
        // multiple times, then collapse into ordinal-ordered rows.
        let mut effects_per_fqn: BTreeMap<String, Vec<(String, i64, String)>> = BTreeMap::new();

        for (fqn, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };
            let mut i = 0;
            while i + 11 <= payload.len() {
                if !(payload[i] == 0xCF
                    && payload[i + 1] == 0x40
                    && payload[i + 2] == 0x00
                    && payload[i + 3] == 0x00)
                {
                    i += 1;
                    continue;
                }
                let hi32 = u32::from_be_bytes(payload[i + 5..i + 9].try_into().unwrap());
                let matched = enums.iter().find(|(h, _, _)| *h == hi32);
                let Some((_, kind, e_opt)) = matched else {
                    i += 9;
                    continue;
                };
                let Some(e) = e_opt else {
                    i += 9;
                    continue;
                };
                // Tail is 05 <enum_index_u8> ...
                if payload[i + 9] != 0x05 {
                    i += 9;
                    continue;
                }
                let idx = payload[i + 10] as usize;
                if idx >= e.members.len() {
                    i += 9;
                    continue;
                }
                let member = e.members[idx].clone();
                effects_per_fqn.entry(fqn.clone()).or_default().push((
                    kind.to_string(),
                    idx as i64,
                    member,
                ));
                i += 9;
            }
        }

        // Dedup: collapse identical (kind, enum_index, member) tuples per FQN
        // while preserving first-seen order.
        for v in effects_per_fqn.values_mut() {
            let mut seen = std::collections::HashSet::new();
            v.retain(|t| seen.insert(t.clone()));
        }

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO ability_effects \
               (ability_fqn, ordinal, kind, enum_index, enum_member) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut abilities = 0u64;
        let mut total = 0u64;
        for (fqn, list) in &effects_per_fqn {
            if list.is_empty() {
                continue;
            }
            abilities += 1;
            for (ord, (kind, idx, member)) in list.iter().enumerate() {
                insert.execute(params![fqn, ord as i64, kind, idx, member])?;
                total += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((abilities, total))
    }

    /// Populate `talent_stat_effects` by walking every talent payload for
    /// CF40 D954FB02 markers. Each marker encodes (STAT enum index, float32
    /// magnitude). Per docs/probes/typed-value-encoding.md the format is
    /// `<05><stat_idx_u8><01><04><float32_LE>`.
    ///
    /// Returns (talents_with_effects, total_effects_inserted).
    pub fn populate_talent_stat_effects(&self) -> Result<(u64, u64)> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        const D954FB02_HI32: u32 = 0xD954FB02;
        let stat_enum = match crate::gom_schema::enum_for_name("STAT") {
            Some(e) => e,
            None => return Ok((0, 0)),
        };

        self.flush()?;
        let conn = self.conn.lock().unwrap();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'tal.%' AND is_canonical = 1",
            )?;
            let collected: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        let tx = conn.unchecked_transaction()?;
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO talent_stat_effects \
               (talent_fqn, ordinal, stat_index, stat_name, magnitude) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut talents_with_effects = 0u64;
        let mut total_effects = 0u64;
        let target_hi32_bytes = D954FB02_HI32.to_be_bytes();

        for (fqn, b64) in &rows {
            let Ok(payload) = BASE64.decode(b64) else {
                continue;
            };
            let mut ordinal: i64 = 0;
            let mut i = 0;
            while i + 9 + 8 <= payload.len() {
                let is_marker = payload[i] == 0xCF
                    && payload[i + 1] == 0x40
                    && payload[i + 2] == 0x00
                    && payload[i + 3] == 0x00
                    && payload[i + 5..i + 9] == target_hi32_bytes;
                if !is_marker {
                    i += 1;
                    continue;
                }
                // Tail: <05><stat_idx_u8><01><04><float32_LE>
                let tail = &payload[i + 9..];
                if tail.len() < 8 || tail[0] != 0x05 || tail[2] != 0x01 || tail[3] != 0x04 {
                    i += 9;
                    continue;
                }
                let stat_idx = tail[1] as usize;
                let mag = f32::from_le_bytes(tail[4..8].try_into().unwrap());
                if !mag.is_finite() || stat_idx >= stat_enum.members.len() {
                    i += 9;
                    continue;
                }
                let stat_name = &stat_enum.members[stat_idx];
                insert.execute(params![
                    fqn,
                    ordinal,
                    stat_idx as i64,
                    stat_name,
                    mag as f64
                ])?;
                ordinal += 1;
                total_effects += 1;
                i += 9;
            }
            if ordinal > 0 {
                talents_with_effects += 1;
            }
        }
        drop(insert);
        tx.commit()?;
        Ok((talents_with_effects, total_effects))
    }

    /// Populate `tags`, `ability_tags`, and `talent_tags` from the
    /// `tagTablePrototype` singleton + ability/talent payload scan (#174).
    ///
    /// Returns `(tag_count, ability_edge_count, talent_edge_count)`.
    ///
    /// Critical: scans BOTH canonical and non-canonical rows per abl/tal FQN
    /// because tag-hash references typically live on the longer non-canonical
    /// variant that kessel's content-GUID deduplication marks as
    /// `is_canonical = 0`.
    pub fn populate_tags_and_edges(&self) -> Result<(u64, u64, u64)> {
        use crate::schema::tag_table::{decode_tag_table, TagIndex};
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
        use std::collections::{BTreeMap, BTreeSet};

        // Load the tagTablePrototype singleton payload.
        let tag_payload: Option<Vec<u8>> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT payload_b64 FROM singletons WHERE fqn = 'tagTablePrototype'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|b64| BASE64.decode(&b64).ok())
        };
        let Some(tag_payload) = tag_payload else {
            return Ok((0, 0, 0));
        };

        let records = decode_tag_table(&tag_payload);
        let index = TagIndex::build(&records);
        let fqn_to_hash: BTreeMap<String, String> = records
            .iter()
            .map(|r| (r.tag_fqn.clone(), r.tag_hash.clone()))
            .collect();

        // Pull every abl/tal row (canonical + non-canonical) and the canonical
        // game_id keyed by FQN.
        let conn = self.conn.lock().unwrap();
        let payloads: Vec<(String, Vec<u8>)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE fqn LIKE 'abl.%' OR fqn LIKE 'tal.%'",
            )?;
            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .filter_map(|(fqn, b64): (String, String)| {
                    BASE64.decode(&b64).ok().map(|p| (fqn, p))
                })
                .collect();
            rows
        };
        let canonical_game_id: BTreeMap<String, String> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, game_id FROM objects \
                 WHERE (fqn LIKE 'abl.%' OR fqn LIKE 'tal.%') AND is_canonical = 1",
            )?;
            let rows: BTreeMap<String, String> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        // Aggregate tag FQNs per ability/talent FQN across all variants.
        let mut tags_by_fqn: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (fqn, payload) in &payloads {
            let hits = index.scan_payload(payload);
            if hits.is_empty() {
                continue;
            }
            tags_by_fqn.entry(fqn.clone()).or_default().extend(hits);
        }

        let tx = conn.unchecked_transaction()?;
        let tag_count = records.len() as u64;
        {
            let mut insert_tag = tx.prepare_cached(
                "INSERT OR REPLACE INTO tags (tag_hash, tag_fqn, hash_marker) \
                 VALUES (?1, ?2, ?3)",
            )?;
            for r in &records {
                insert_tag.execute(params![r.tag_hash, r.tag_fqn, r.hash_marker])?;
            }
        }

        let mut ability_edges = 0u64;
        let mut talent_edges = 0u64;
        {
            let mut insert_abl_tag = tx.prepare_cached(
                "INSERT OR REPLACE INTO ability_tags \
                   (ability_fqn, ability_game_id, tag_hash, tag_fqn) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut insert_tal_tag = tx.prepare_cached(
                "INSERT OR REPLACE INTO talent_tags \
                   (talent_fqn, talent_game_id, tag_hash, tag_fqn) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (fqn, tag_fqns) in &tags_by_fqn {
                let game_id = canonical_game_id.get(fqn);
                let is_talent = fqn.starts_with("tal.");
                for tag_fqn in tag_fqns {
                    let Some(tag_hash) = fqn_to_hash.get(tag_fqn) else {
                        continue;
                    };
                    if is_talent {
                        insert_tal_tag.execute(params![fqn, game_id, tag_hash, tag_fqn])?;
                        talent_edges += 1;
                    } else {
                        insert_abl_tag.execute(params![fqn, game_id, tag_hash, tag_fqn])?;
                        ability_edges += 1;
                    }
                }
            }
        }
        tx.commit()?;
        Ok((tag_count, ability_edges, talent_edges))
    }

    /// Populate `discipline_talents` by FQN pattern.
    ///
    /// Mechanical mapping:
    /// - `tal.<class>.skill.<discipline>.<rest>` belongs to that one
    ///   combat discipline (`abl.<class>.skill.<discipline>`).
    /// - `tal.<class>.skill.utility.<rest>` is a per-class SHARED utility
    ///   talent. Fanned out: one row per combat discipline of that class,
    ///   plus its own row under the utility discipline. This mirrors how
    ///   SWTOR exposes utility talents in the discipline UI -- a
    ///   utility-talent pick is visible on every combat-tree page for that
    ///   class.
    ///
    /// No tier_level / column coordinates: SWTOR's editorial tree layout
    /// (which talent sits at what tier on screen) is not encoded in tal.*
    /// payloads or FQNs. This function only emits mechanical membership.
    pub fn populate_discipline_talents(&self) -> Result<(u64, u64)> {
        self.flush()?;
        crate::queries::ability::populate_discipline_talents(&self.conn.lock().unwrap())
    }

    /// Populate `talent_abilities` by decoding GUID refs from `tal.*` payloads.
    ///
    /// Pattern (from MAPPINGS.md): CC 17E2840B D001 CF E000 [8-byte GUID BE]
    /// 37% of talents reference 1-3 abilities this way.
    pub fn populate_talent_abilities(&self) -> Result<u64> {
        self.flush()?;
        crate::queries::ability::populate_talent_abilities(&self.conn.lock().unwrap())
    }
}

/// Derive a structured GSF component identifier from an art file path
/// like `art/dynamic/space_pvp/ships/imp_scout/sweapon/imp_scout_a_sweapon_03.gr2`.
///
/// Returns a slash-separated identifier `<faction>/<class>/<slot>/<variant>/<tier>`
/// like `imp/scout/sweapon/a/03`. Returns None if the path doesn't match the
/// expected GSF asset shape.
pub(crate) fn derive_gsf_component_kind(path: &str) -> Option<String> {
    // Strip directory prefix and .gr2/.fxspec suffix to get the basename.
    let basename = path.rsplit('/').next()?;
    let stem = basename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(basename);
    // Stem shape: <faction>_<class>_<variant>_<slot>_<tier>
    // - faction: imp | rep | spvp (3+ char)
    // - class: scout | strike | gunship | bomber | starship_<name> (1+ segs)
    // - variant: a | b | c
    // - slot: sweapon | pweapon | engine | shield | reactor | sensors | armor |
    //         magazine | capacitor | thrusters | copilot
    // - tier: 01..09
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() < 4 {
        return None;
    }
    // Find the trailing 2-digit tier
    let last = parts.last()?;
    if last.len() != 2 || !last.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let tier = *last;
    // Slot is the segment before the tier
    let slot = parts.get(parts.len() - 2)?;
    // Variant is the segment before the slot (single char a/b/c)
    let variant = parts.get(parts.len() - 3)?;
    if variant.len() != 1 {
        return None;
    }
    // Faction is the first segment; class is everything between
    let faction = parts.first()?;
    let class_parts = &parts[1..parts.len() - 3];
    if class_parts.is_empty() {
        return None;
    }
    let class = class_parts.join("_");
    Some(format!("{faction}/{class}/{slot}/{variant}/{tier}"))
}

/// Create the ability tables (idempotent).
pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        r#"
            -- Disciplines: one row per combat discipline. After the PR3
            -- rework (#94 follow-up), per-origin shared and utility pools
            -- no longer live here -- they go to combat_style_shared_abilities
            -- and class_utility_talents respectively.
            --
            -- Two keys: origin_codename (mechanical from FQN -- abl.<origin>.skill.*)
            -- and combat_style_codename (from a hardcoded 48-row map: stable
            -- since 4.0, e.g. vengeance->juggernaut, watchman->sentinel).
            -- huttspawn ETL uses origin for faction routing + nav grouping
            -- and combat_style for editorial joins to combat_style_shared_abilities
            -- / class_utility_talents.
            CREATE TABLE IF NOT EXISTS disciplines (
                origin_codename        TEXT NOT NULL,
                discipline_name        TEXT NOT NULL,
                fqn_prefix             TEXT NOT NULL UNIQUE,  -- e.g. "abl.jedi_knight.skill.defense"
                combat_style_codename  TEXT NOT NULL,
                PRIMARY KEY (origin_codename, discipline_name),
                FOREIGN KEY (combat_style_codename) REFERENCES combat_styles(fqn_segment)
            );
            CREATE INDEX IF NOT EXISTS idx_disciplines_origin ON disciplines(origin_codename);
            CREATE INDEX IF NOT EXISTS idx_disciplines_combat_style ON disciplines(combat_style_codename);
            -- Discipline abilities: every abl.* that belongs to a discipline,
            -- with tier level and slot type derived from FQN segments.
            -- tier_level: NULL for base abilities (no mods segment), else
            --   15/23/27/35/39/43/47/51/60/64/68/73/78 from tal.* payload.
            -- slot_type: 'core' | 'choice' | 'utility' | 'special' | 'passive' | 'base'
            CREATE TABLE IF NOT EXISTS discipline_abilities (
                discipline_fqn_prefix  TEXT NOT NULL,
                ability_game_id        TEXT NOT NULL,
                ability_fqn            TEXT NOT NULL,
                tier_level             INTEGER,
                slot_type              TEXT NOT NULL,
                PRIMARY KEY (discipline_fqn_prefix, ability_game_id),
                FOREIGN KEY (ability_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_discipline_abilities_disc ON discipline_abilities(discipline_fqn_prefix);
            CREATE INDEX IF NOT EXISTS idx_discipline_abilities_abl  ON discipline_abilities(ability_game_id);
            -- Discipline talents: every tal.* that belongs to a discipline.
            -- Mapping is mechanical from FQN: a talent at
            --   tal.<class>.skill.<segment>.<name>
            -- maps to the discipline whose fqn_prefix is
            --   abl.<class>.skill.<segment>
            -- This includes the per-class shared utility discipline
            -- (tal.<class>.skill.utility.*) -- consumers fold those into
            -- combat-discipline trees editorially if they want.
            --
            -- No tier_level column. SWTOR's discipline-tree tier coordinates
            -- (which talent sits at which level/column on screen) are not
            -- encoded in tal.* payloads or FQN segments -- that's editorial
            -- tree layout that lives outside kessel's source data. The
            -- junction is mechanical membership only; trees rendered from
            -- it need a separate curated layout layer.
            CREATE TABLE IF NOT EXISTS discipline_talents (
                discipline_fqn_prefix  TEXT NOT NULL,
                talent_game_id         TEXT NOT NULL,
                talent_fqn             TEXT NOT NULL,
                PRIMARY KEY (discipline_fqn_prefix, talent_game_id),
                FOREIGN KEY (talent_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_discipline_talents_disc ON discipline_talents(discipline_fqn_prefix);
            CREATE INDEX IF NOT EXISTS idx_discipline_talents_tal  ON discipline_talents(talent_game_id);
            -- Combat-style-level utility talent pool (#94 PR3 rework).
            -- Replaces the per-discipline fan-out previously emitted into
            -- discipline_talents for `tal.<origin>.skill.utility.*`. Each
            -- origin's utility talents fan to BOTH combat styles of that
            -- origin -- huttspawn ETL just reads, no FQN re-derivation.
            CREATE TABLE IF NOT EXISTS class_utility_talents (
                combat_style_codename  TEXT NOT NULL,
                talent_game_id         TEXT NOT NULL,
                talent_fqn             TEXT NOT NULL,
                PRIMARY KEY (combat_style_codename, talent_game_id),
                FOREIGN KEY (talent_game_id) REFERENCES objects(game_id),
                FOREIGN KEY (combat_style_codename) REFERENCES combat_styles(fqn_segment)
            );
            CREATE INDEX IF NOT EXISTS idx_class_utility_talents_style
                ON class_utility_talents(combat_style_codename);
            CREATE INDEX IF NOT EXISTS idx_class_utility_talents_talent
                ON class_utility_talents(talent_game_id);
            -- Talent → ability links: GUID refs decoded from tal.* payloads.
            -- 37% of talents reference 1-3 abilities via CC 17E2840B + CF GUID pattern.
            CREATE TABLE IF NOT EXISTS talent_abilities (
                talent_game_id   TEXT NOT NULL,
                talent_fqn       TEXT NOT NULL,
                ability_game_id  TEXT NOT NULL,
                ability_fqn      TEXT,           -- NULL if GUID not in our object set
                PRIMARY KEY (talent_game_id, ability_game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_talent_abilities_talent  ON talent_abilities(talent_game_id);
            CREATE INDEX IF NOT EXISTS idx_talent_abilities_ability ON talent_abilities(ability_game_id);
            -- Talent classification + script-hook decode from tal.* GOM
            -- payload (#70). resource_pool mirrors ability_stats (rage,
            -- focus, force, heat, ammo, energy, gsf). tier is the FQN's
            -- last segment (tier1, tier_3a, base, passive, etc) — useful
            -- for grouping discipline-tree tiers and GSF upgrade tiers
            -- without a second lookup. script_hook is the length-prefixed
            -- ASCII tail string at the end of the talent payload (vault
            -- MAPPINGS.md lines 339-365); it identifies the underlying
            -- ability mod the talent triggers (e.g. abl_bh_me_kolto_shot,
            -- spvp_reducedcooldown, iamilitaryofficer). 94% of talents
            -- have one.
            CREATE TABLE IF NOT EXISTS talent_details (
                talent_game_id  TEXT PRIMARY KEY,
                resource_pool   TEXT,    -- force | rage | focus | heat | ammo | energy | gsf | NULL
                tier            TEXT,    -- FQN last segment
                script_hook     TEXT,    -- payload tail string, NULL if none
                FOREIGN KEY (talent_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_talent_details_pool ON talent_details(resource_pool);
            CREATE INDEX IF NOT EXISTS idx_talent_details_hook ON talent_details(script_hook);
            -- Ability stats decoded from abl.* GOM payload (#69, refined #74).
            -- Properties live as a contiguous run of [u16 LE propId][f32 LE
            -- value] records starting at the sentinel 01 04 00 00 80 BF
            -- (= 0x0401 = -1.0, an uninit marker). The block ends where the
            -- next 2 bytes are not in 0x04xx. Walker is sentinel-anchored,
            -- which eliminates the brute-force false positives the v1 scan
            -- produced (force_cost=1 on warrior/agent abilities, etc).
            -- Resource costs use a value threshold (>=5) since 0x0403 also
            -- appears at low values for non-Force tech abilities as a
            -- scaling coefficient, not a cost.
            CREATE TABLE IF NOT EXISTS ability_stats (
                ability_game_id     TEXT PRIMARY KEY,
                resource_pool       TEXT,    -- force | rage | focus | heat | ammo | energy | NULL
                cooldown            REAL,    -- 0x0401, seconds
                cast_time           REAL,    -- 0x041b, seconds (cast or channel)
                channel_duration    REAL,    -- 0x0406, seconds
                hard_cast_time      REAL,    -- 0x041a, alternate cast prop
                force_cost          INTEGER, -- 0x0403, Force-pool cost (sorcerer/sage)
                resource_cost       INTEGER, -- 0x041e, heat/energy/ammo cost (tech)
                raw_props           TEXT,    -- JSON {hex_id: f32} for all in-block 0x04xx records
                FOREIGN KEY (ability_game_id) REFERENCES objects(game_id)
            );
            -- GSF talent stats decoded from tal.spvp.* GOM payloads (#80).
            -- Records have shape `[c9 01]? <stat_id:u8> 01 04 <f32 LE>` and end
            -- at the signature `cb 19 d7 4b ?? 03`. Stat IDs are decoded via
            -- the embedded gsf_stat_dictionary.toml; rows ship with plain
            -- labels and units so consumers query
            --   WHERE label = 'cooldown_delta_seconds'
            -- rather than a hex byte. `confidence` is verified | guess |
            -- unknown -- callers can filter to verified-only data.
            -- `rank` preserves rank-progression ordering when a single talent
            -- payload encodes multiple records of the same stat (e.g.
            -- engine_power_regen.upgrade emits +4/+8/+12 as three rows).
            CREATE TABLE IF NOT EXISTS gsf_talent_stats (
                talent_game_id  TEXT NOT NULL,
                label           TEXT NOT NULL,
                unit            TEXT NOT NULL,
                rank            INTEGER NOT NULL,
                value           REAL NOT NULL,
                confidence      TEXT NOT NULL,
                stat_id         INTEGER NOT NULL,  -- raw byte for forensics
                PRIMARY KEY (talent_game_id, label, rank),
                FOREIGN KEY (talent_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gsf_talent_stats_label
                ON gsf_talent_stats(label);
            -- GSF base ability stats decoded from abl.spvp.* payloads (#78).
            -- Same shape as gsf_talent_stats: labels and units come from the
            -- embedded dictionary. prop_id semantics differ from ground
            -- abilities (0x0402 = cooldown for GSF, animation marker for
            -- ground), so the dictionary has a separate ability_stats section.
            CREATE TABLE IF NOT EXISTS gsf_ability_stats (
                ability_game_id TEXT NOT NULL,
                label           TEXT NOT NULL,
                unit            TEXT NOT NULL,
                rank            INTEGER NOT NULL,
                value           REAL NOT NULL,
                confidence      TEXT NOT NULL,
                prop_id         INTEGER NOT NULL,  -- raw u16 for forensics
                payload_ordinal INTEGER,           -- 0-based payload emission order of this record; the index a <<N>> description template anchors to (distinct from per-label `rank`). #309.
                PRIMARY KEY (ability_game_id, label, rank),
                FOREIGN KEY (ability_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gsf_ability_stats_label
                ON gsf_ability_stats(label);
            -- Ability description tokens (#309). One row per <<N>> token in an
            -- ability's description, naming WHAT the token represents
            -- (damage / healing / duration / rank / absorption / bindpoint /
            -- stat), decoded from the payload SpecParam list via the client.gom
            -- ablDescriptionTokenType enum. token_index is 0-based (<<1>> -> 0).
            -- The numeric value for damage/healing tokens is runtime-scaled and
            -- not stored here; this table resolves the durable token MEANING.
            CREATE TABLE IF NOT EXISTS ability_desc_tokens (
                ability_fqn TEXT NOT NULL,
                token_index INTEGER NOT NULL,
                token_type  TEXT NOT NULL,
                PRIMARY KEY (ability_fqn, token_index)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_desc_tokens_type
                ON ability_desc_tokens(token_type);
            -- Talent typed columns (#140) -- 7 props from Talent schema.
            CREATE TABLE IF NOT EXISTS talent_details (
                fqn               TEXT PRIMARY KEY,
                discipline_code   TEXT,
                tree_position     INTEGER,
                tier              INTEGER
            );
            -- GSF requisition costs (#115 / #172). Decoded from the
            -- scFFComponentsCostPrototype + scFFComponentUpgradesCostPrototype
            -- singletons. (target_guid, cost_kind, tier) is the natural key;
            -- a single component appears once with cost_kind = 'component_unlock'
            -- and tier = 0, then five times with cost_kind = 'tier_upgrade' and
            -- tier = 1..5. target_game_id resolves the GUID into kessel's
            -- objects table (NULL when the component's content GUID isn't in
            -- the extracted set).
            CREATE TABLE IF NOT EXISTS gsf_requisition_costs (
                target_guid       TEXT NOT NULL,
                cost_kind         TEXT NOT NULL CHECK (cost_kind IN ('component_unlock', 'tier_upgrade')),
                tier              INTEGER NOT NULL,
                cost              INTEGER NOT NULL,
                target_game_id    TEXT,
                target_fqn        TEXT,
                art_path          TEXT,
                component_kind    TEXT,
                PRIMARY KEY (target_guid, cost_kind, tier)
            );
            CREATE INDEX IF NOT EXISTS idx_gsf_req_costs_target ON gsf_requisition_costs(target_game_id);
            CREATE INDEX IF NOT EXISTS idx_gsf_req_costs_kind ON gsf_requisition_costs(component_kind);
            -- GSF premium starter-ship roster (#115 lineage). The 10
            -- itm.spvp.ships.premium.* objects with display name, faction, and
            -- class. The ship -> loadout-template binding is NOT in the archive
            -- (client-side); consumers map it by ship_class.
            CREATE TABLE IF NOT EXISTS gsf_ships (
                fqn         TEXT PRIMARY KEY,
                game_id     TEXT,
                name        TEXT,
                faction     TEXT,
                ship_class  TEXT
            );
            -- GSF loadout slot templates (#115 lineage). Decoded from the
            -- conSpec_scff_equip_* singletons; one row per distinct component
            -- slot a template declares. Major templates carry the
            -- weapon/shield/engine slots, minor templates the four
            -- armor/capacitor/magazine/reactor/sensor/thruster-class slots.
            CREATE TABLE IF NOT EXISTS gsf_loadout_slots (
                template_code  TEXT NOT NULL,
                slot_kind      TEXT NOT NULL CHECK (slot_kind IN ('major', 'minor')),
                slot_type      TEXT NOT NULL,
                slot_ordinal   INTEGER NOT NULL,
                PRIMARY KEY (template_code, slot_type, slot_ordinal)
            );
            -- Ability/talent -> effect block linkage (#173). One row per
            -- indexed CF E0 sub-record in the parent's payload. The parent
            -- self-reference is NOT included; this table is the structural
            -- linkage to the parent's effect-block sub-records that carry
            -- per-block typed properties (Weapon Damage, Modify Meta Stat,
            -- Play Appearance, Call Effect).
            --
            -- block_game_id is NULL when the effect block's GUID doesn't
            -- resolve to an extracted object (versioned-only ability
            -- category, issue #179). The raw block_guid is preserved so the
            -- unresolved edge stays visible in spice instead of being
            -- silently dropped.
            CREATE TABLE IF NOT EXISTS ability_effect_blocks (
                parent_game_id    TEXT NOT NULL,
                block_index       INTEGER NOT NULL,
                block_guid        TEXT NOT NULL,
                block_game_id     TEXT,
                PRIMARY KEY (parent_game_id, block_index),
                FOREIGN KEY (parent_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_effect_blocks_block
                ON ability_effect_blocks(block_game_id);
            CREATE INDEX IF NOT EXISTS idx_ability_effect_blocks_guid
                ON ability_effect_blocks(block_guid);
            -- Ability action records. Decoded from CF40 markers
            -- E251D1CC (effAction), E251D1CD (effCondition),
            -- E251D1D0 (effInitializer), E71E2F92 (effLogicOp) in
            -- ability payloads. Scans BOTH canonical and non-canonical
            -- variants per FQN -- the rich effect data lives on the
            -- longer non-canonical variant (same pattern as PR #174 tags).
            -- Per-action parameter decode (damage values, multipliers)
            -- is deferred to the E251D1CE/CF parameter-list grammar work.
            CREATE TABLE IF NOT EXISTS ability_effects (
                ability_fqn       TEXT NOT NULL,
                ordinal           INTEGER NOT NULL,
                kind              TEXT NOT NULL CHECK (kind IN ('action','condition','initializer','logic_op')),
                enum_index        INTEGER NOT NULL,
                enum_member       TEXT NOT NULL,
                PRIMARY KEY (ability_fqn, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_effects_kind ON ability_effects(kind);
            CREATE INDEX IF NOT EXISTS idx_ability_effects_member ON ability_effects(enum_member);
            -- Ability effect numeric parameters. Decoded from the
            -- E251D1CE and E251D1CF parameter-list CF40 markers in abl
            -- payloads. Each row is one (param_flag, float_value) pair
            -- found inside the parameter-list tail.
            --
            -- Format inside the param list: `04 01 01 <flag_u8> <f32_LE>`
            -- (a typed-value triplet introducing a single float). param_flag
            -- empirically clusters around 0x10 (~90% of triplets) and 0x20
            -- (~10%); the semantic meaning of each flag varies per action
            -- and is not yet enum-resolved.
            --
            -- Floats outside the `04 01 01 XX <f32>` pattern (other
            -- parameter shapes) are NOT yet decoded -- this captures the
            -- subset that uses the well-formed primitive pattern.
            CREATE TABLE IF NOT EXISTS ability_effect_params (
                ability_fqn       TEXT NOT NULL,
                ordinal           INTEGER NOT NULL,
                source            TEXT NOT NULL CHECK (source IN ('CE','CF')),
                param_flag        INTEGER NOT NULL,
                value_f32         REAL NOT NULL,
                PRIMARY KEY (ability_fqn, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_effect_params_flag
                ON ability_effect_params(param_flag);
            -- Object CC marker references. CC is the second-layer marker
            -- family per docs/probes/dis-payload-format.md: every CC byte
            -- is followed by a 4-byte property-name hash + a variable-
            -- length value. CC markers are 4-10x more common than CF40
            -- markers per object (Quest: 474 CC vs 83 CF40; NPC: 11 CC
            -- vs 3 CF40). They're the alternative storage layer for the
            -- per-object typed data the CF40 walker doesn't catch.
            --
            -- The 4-byte CC ID is stored LE in payloads; the MAPPINGS.md
            -- known IDs are BE strings, so they're byte-reversed here.
            -- Known names (per MAPPINGS.md + legion 019e4e2a):
            --   37AE6F6F = stringRef        (BE: 6F6FAE37)
            --   0B84E217 = abilityRef       (BE: 17E2840B)
            --   03DDAFE4 = ?                (BE: E4AFDD03)
            --   2D31CD0C = ?                (BE: 0CCD312D)
            --   19D74B9D = ?                (BE: 9D4BD719)
            --   19D74B96 = ?                (BE: 964BD719)
            -- The full CC-hash-to-name dictionary is in a proprietary
            -- Bioware namespace; spike #144 to crack it (6 known names
            -- so far; ~700+ distinct IDs observed corpus-wide).
            --
            -- value_bytes_hex captures up to 16 bytes from the CC marker's
            -- value tail (limited because per-ID length grammar is unknown);
            -- a future PR per-ID grammar will extract typed values cleanly.
            CREATE TABLE IF NOT EXISTS object_cc_refs (
                object_game_id     TEXT NOT NULL,
                ordinal            INTEGER NOT NULL,
                cc_id_hex          TEXT NOT NULL,
                cc_known_name      TEXT,
                value_bytes_hex    TEXT NOT NULL,
                PRIMARY KEY (object_game_id, ordinal),
                FOREIGN KEY (object_game_id) REFERENCES objects(game_id)
            );
            CREATE INDEX IF NOT EXISTS idx_object_cc_refs_id ON object_cc_refs(cc_id_hex);
            CREATE INDEX IF NOT EXISTS idx_object_cc_refs_name ON object_cc_refs(cc_known_name);
            -- effAction parameter values. Within each effAction's parameter
            -- array, parameters are encoded as `<effParam_idx_u8><f32_LE>`.
            -- This is the per-action numeric data: damage coefficients,
            -- modifier amounts, condition values.
            --
            -- Verified for Massacre (effAction_BallisticImpulse):
            --   effParam_StandardHealthPercentMin = 0.1543
            --   effParam_IgnoreDualWieldModifier  = 1.54
            -- The 1.54 is the coefficient parsely displays as ~1.47.
            --
            -- Detection heuristic: inside the bytes after an effAction
            -- marker (until the next CF40 marker), scan for the pattern
            -- `<5-byte segment header><param_id_u8><f32_LE>` repeating.
            -- The segment header has the form `14 ED 0D 1E 3E ...` (more
            -- complex param shapes are not yet decoded).
            CREATE TABLE IF NOT EXISTS ability_action_params (
                ability_fqn       TEXT NOT NULL,
                effect_ordinal    INTEGER NOT NULL,
                param_ordinal     INTEGER NOT NULL,
                effparam_index    INTEGER NOT NULL,
                effparam_name     TEXT,
                value_f32         REAL NOT NULL,
                PRIMARY KEY (ability_fqn, effect_ordinal, param_ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_action_params_name
                ON ability_action_params(effparam_name);
            -- Damage-action CC parameter values. Per-effAction_Damage
            -- record, captures the 6 core CC parameter IDs + 2 optional
            -- ones, each with a 1-byte int8 value (CC + 4-byte ID + i8).
            --
            -- Verified across 1,452 occurrences each for the 6 core IDs:
            --   0135C0E0, 017459AB, 39285472, 0BB0D06E, 0176E21B, 011A6E3E
            -- Plus optional 3C0EB23D (342 occ), 0B9BBBDA (318 occ).
            --
            -- CC ID names are unknown (separate Bioware hash namespace per
            -- spike #144). The 1-byte values likely encode damage_type,
            -- modifier_type, target flag, etc. -- semantic resolution
            -- requires the hash crack or consumer-side reverse-mapping.
            --
            -- NOTE: the float damage coefficient (Massacre 1.47 per
            -- parsely) is NOT in these CC fields. It lives in a different
            -- encoding layer not yet characterized.
            CREATE TABLE IF NOT EXISTS ability_damage_params (
                ability_fqn       TEXT NOT NULL,
                effect_ordinal    INTEGER NOT NULL,
                param_ordinal     INTEGER NOT NULL,
                cc_id_hex         TEXT NOT NULL,
                value_i8          INTEGER NOT NULL,
                PRIMARY KEY (ability_fqn, effect_ordinal, param_ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_damage_params_cc
                ON ability_damage_params(cc_id_hex);
            -- Talent stat effects (PR: wire-typed-value-decoder).
            -- Decoded from CF40 D954FB02 markers in talent payloads.
            -- Each row: which stat the talent modifies and by how much.
            -- Verified format: <05><stat_idx_u8><01><04><float32_LE>.
            -- stat_name resolves to a member of the STAT enum (517 members);
            -- magnitude is a multiplier (e.g. 0.30 = +30%) per modStatType
            -- semantics. modStatType is in the D954FB04 effect-block class
            -- but is not yet decoded per-row here (deferred to per-property
            -- element grammar work).
            CREATE TABLE IF NOT EXISTS talent_stat_effects (
                talent_fqn        TEXT NOT NULL,
                ordinal           INTEGER NOT NULL,
                stat_index        INTEGER NOT NULL,
                stat_name         TEXT NOT NULL,
                magnitude         REAL NOT NULL,
                PRIMARY KEY (talent_fqn, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_talent_stat_effects_stat
                ON talent_stat_effects(stat_name);
            -- Tag dictionary (#174). Decoded from the `tagTablePrototype`
            -- singleton. ~6750 entries, all in the `tag.abl.*` namespace.
            -- Two marker forms:
            --   CE  → 7-byte hash (44 legacy records)
            --   CF  → 8-byte hash (6706 records)
            -- The hash is what abilities/talents reference in their payloads;
            -- `tag_fqn` is the human-readable name.
            CREATE TABLE IF NOT EXISTS tags (
                tag_hash       TEXT PRIMARY KEY,
                tag_fqn        TEXT NOT NULL,
                hash_marker    TEXT NOT NULL CHECK (hash_marker IN ('CE', 'CF'))
            );
            CREATE INDEX IF NOT EXISTS idx_tags_fqn ON tags(tag_fqn);
            -- Ability ↔ tag edges (#174). One row per (ability FQN, tag FQN)
            -- pair where the tag's hash bytes appear in any payload variant
            -- of that ability (canonical OR non-canonical -- tags often live
            -- on the longer non-canonical variant that kessel deduplicates).
            -- Aggregated per FQN so users get a stable tag set regardless of
            -- which variant happens to be canonical.
            CREATE TABLE IF NOT EXISTS ability_tags (
                ability_fqn        TEXT NOT NULL,
                ability_game_id    TEXT,
                tag_hash           TEXT NOT NULL,
                tag_fqn            TEXT NOT NULL,
                PRIMARY KEY (ability_fqn, tag_hash),
                FOREIGN KEY (tag_hash) REFERENCES tags(tag_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_ability_tags_tag ON ability_tags(tag_hash);
            CREATE INDEX IF NOT EXISTS idx_ability_tags_game_id ON ability_tags(ability_game_id);
            -- Talent ↔ tag edges (#174). Same shape as ability_tags.
            CREATE TABLE IF NOT EXISTS talent_tags (
                talent_fqn         TEXT NOT NULL,
                talent_game_id     TEXT,
                tag_hash           TEXT NOT NULL,
                tag_fqn            TEXT NOT NULL,
                PRIMARY KEY (talent_fqn, tag_hash),
                FOREIGN KEY (tag_hash) REFERENCES tags(tag_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_talent_tags_tag ON talent_tags(tag_hash);
            CREATE INDEX IF NOT EXISTS idx_talent_tags_game_id ON talent_tags(talent_game_id);
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::*;
    #[test]
    fn populate_disciplines_emits_combat_style_codename_and_routes_shared_pool() {
        let path = temp_db_path("disc_rework");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        seed_combat_styles_for(&db, "sith_warrior");

        // Combat-discipline ability for sith_warrior (vengeance is a juggernaut spec).
        insert_obj(
            &db,
            "v1",
            "abl.sith_warrior.skill.vengeance.ravage",
            "Ability",
        );
        // Class-shared ability (3-segment FQN) -- should fan to BOTH combat styles.
        insert_obj(&db, "fl", "abl.sith_warrior.force_leap", "Ability");
        // Skill-utility ability -- per-origin pool, fans to both styles.
        insert_obj(
            &db,
            "th",
            "abl.sith_warrior.skill.utility.thwart",
            "Ability",
        );
        // Shared-mod ability -- per-origin pool, fans to both styles.
        insert_obj(
            &db,
            "sm",
            "abl.sith_warrior.skill.mods.tier1.savagery",
            "Ability",
        );
        // Non-player-origin abl.* should be ignored entirely.
        insert_obj(&db, "co", "abl.companion.attack", "Ability");

        let (disc_count, disc_abl_count, css_abl_count) = db.populate_disciplines().unwrap();

        // One discipline (vengeance), one discipline_abilities row (ravage).
        assert_eq!(disc_count, 1);
        assert_eq!(disc_abl_count, 1);
        // Three shared abilities (force_leap, thwart, savagery) x 2 combat styles = 6 rows.
        assert_eq!(css_abl_count, 6);

        let conn = db.conn.lock().unwrap();

        // disciplines: combat_style_codename resolved correctly.
        let (origin, combat_style): (String, String) = conn
            .query_row(
                "SELECT origin_codename, combat_style_codename FROM disciplines \
                 WHERE discipline_name = 'vengeance'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(origin, "sith_warrior");
        assert_eq!(combat_style, "juggernaut");

        // No utility / shared rows in disciplines.
        let leftovers: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM disciplines WHERE discipline_name IN ('utility', 'shared')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(leftovers, 0);

        // combat_style_shared_abilities: each ability fanned to BOTH styles.
        let leap_styles: Vec<String> = conn
            .prepare(
                "SELECT combat_style_codename FROM combat_style_shared_abilities \
                 WHERE ability_game_id = 'fl' ORDER BY combat_style_codename",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(leap_styles, vec!["juggernaut", "marauder"]);

        // discipline_abilities does NOT contain class_shared / utility / shared_mod rows.
        let post_rework_slots: Vec<String> = conn
            .prepare("SELECT DISTINCT slot_type FROM discipline_abilities")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(!post_rework_slots.iter().any(|s| s == "class_shared"));
        assert!(!post_rework_slots.iter().any(|s| s == "utility"));
        assert!(!post_rework_slots.iter().any(|s| s == "shared_mod"));

        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn populate_discipline_talents_routes_utility_to_class_utility_talents() {
        let path = temp_db_path("disc_tal_rework");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        seed_combat_styles_for(&db, "sith_warrior");

        // Combat-discipline talent (no fan-out).
        insert_obj(
            &db,
            "t1",
            "tal.sith_warrior.skill.vengeance.unyielding",
            "Talent",
        );
        // Per-origin utility talent: must fan to BOTH styles, NOT enter discipline_talents.
        insert_obj(
            &db,
            "t2",
            "tal.sith_warrior.skill.utility.interloper",
            "Talent",
        );

        // Disciplines must exist before discipline_talents -- run prerequisite.
        insert_obj(
            &db,
            "v1",
            "abl.sith_warrior.skill.vengeance.ravage",
            "Ability",
        );
        db.populate_disciplines().unwrap();

        let (disc_tal_count, util_count) = db.populate_discipline_talents().unwrap();
        assert_eq!(disc_tal_count, 1);
        assert_eq!(util_count, 2); // fanned to juggernaut + marauder

        let conn = db.conn.lock().unwrap();

        // discipline_talents: only the discipline-specific talent. No utility fan-out.
        let dt_rows: u64 = conn
            .query_row("SELECT COUNT(*) FROM discipline_talents", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(dt_rows, 1);

        // class_utility_talents: utility talent fanned to both styles.
        let util_styles: Vec<String> = conn
            .prepare(
                "SELECT combat_style_codename FROM class_utility_talents \
                 WHERE talent_game_id = 't2' ORDER BY combat_style_codename",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(util_styles, vec!["juggernaut", "marauder"]);

        drop(conn);
        let _ = std::fs::remove_file(&path);
    }
}
