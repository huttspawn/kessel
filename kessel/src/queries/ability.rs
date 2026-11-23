//! Abilities, talents and discipline trees: load `abl.*` / `tal.*` / `dis.*`
//! payloads, decode them with `crate::model`, write the existing tables.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rusqlite::{params, Connection};

use crate::gsf_stat_dictionary::StatDictionary;
use crate::model::ability::{decode_ability_stats, decode_gsf_ability};
use crate::model::discipline::{
    decode_discipline_tree, origin_combat_styles, place_ability, place_talent, AbilityPlacement,
    TalentPlacement,
};
use crate::model::talent::{
    decode_gsf_talent, decode_talent_details, extract_ability_guids_from_talent,
};
use crate::queries::{object_payloads, object_payloads_optional};

/// Load `(column0, column1)` string pairs from a parameterless query,
/// skipping rows that fail to read.
fn string_pairs(conn: &Connection, sql: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// `ability_stats` from `abl.*` payloads (#74). Returns rows written.
///
/// # Errors
/// On any SQL failure or when the embedded stat dictionary fails to parse.
pub fn populate_ability_stats(conn: &Connection) -> Result<u64> {
    let dict = StatDictionary::from_embedded()?;
    let payloads = object_payloads(conn, "abl.%")?;

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO ability_stats \
         (ability_game_id, resource_pool, cooldown, cast_time, \
          channel_duration, hard_cast_time, force_cost, resource_cost, \
          raw_props) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    let mut count: u64 = 0;
    for (game_id, fqn, payload) in &payloads {
        let Some(stats) = decode_ability_stats(fqn, payload, &dict) else {
            continue;
        };
        let p = &stats.props;
        let raw_props = if p.raw_props.is_empty() {
            None
        } else {
            serde_json::to_string(&p.raw_props).ok()
        };
        stmt.execute(params![
            game_id,
            stats.resource_pool,
            p.cooldown,
            p.cast_time,
            p.channel_duration,
            p.hard_cast_time,
            p.force_cost,
            p.resource_cost,
            raw_props,
        ])?;
        count += 1;
    }
    drop(stmt);
    tx.commit()?;
    Ok(count)
}

/// `talent_details` for every `tal.*` object (#70). Returns rows written.
///
/// # Errors
/// On any SQL failure.
pub fn populate_talent_details(conn: &Connection) -> Result<u64> {
    let payloads = object_payloads_optional(conn, "tal.%")?;

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO talent_details \
         (talent_game_id, resource_pool, tier, script_hook) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut count: u64 = 0;
    for (game_id, fqn, payload) in &payloads {
        let d = decode_talent_details(fqn, payload.as_deref());
        stmt.execute(params![game_id, d.resource_pool, d.tier, d.script_hook])?;
        count += 1;
    }
    drop(stmt);
    tx.commit()?;
    Ok(count)
}

/// `gsf_talent_stats` from `tal.spvp.*` payloads (#80). Returns rows written.
///
/// # Errors
/// On any SQL failure or when the embedded stat dictionary fails to parse.
pub fn populate_gsf_talent_stats(conn: &Connection) -> Result<u64> {
    let dict = StatDictionary::from_embedded()?;
    let payloads = object_payloads(conn, "tal.spvp.%")?;

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO gsf_talent_stats \
         (talent_game_id, label, unit, rank, value, confidence, stat_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut count: u64 = 0;
    for (game_id, fqn, payload) in &payloads {
        for s in decode_gsf_talent(fqn, payload, &dict) {
            stmt.execute(params![
                game_id,
                s.label,
                s.unit,
                s.rank,
                f64::from(s.value),
                s.confidence,
                i64::from(s.stat_id),
            ])?;
            count += 1;
        }
    }
    drop(stmt);
    tx.commit()?;
    Ok(count)
}

/// `gsf_ability_stats` from `abl.spvp.*` payloads (#78). Returns rows written.
///
/// # Errors
/// On any SQL failure or when the embedded stat dictionary fails to parse.
pub fn populate_gsf_ability_stats(conn: &Connection) -> Result<u64> {
    let dict = StatDictionary::from_embedded()?;
    let payloads = object_payloads(conn, "abl.spvp.%")?;

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare_cached(
        "INSERT OR REPLACE INTO gsf_ability_stats \
         (ability_game_id, label, unit, rank, value, confidence, prop_id, payload_ordinal) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut count: u64 = 0;
    for (game_id, fqn, payload) in &payloads {
        for s in decode_gsf_ability(fqn, payload, &dict) {
            stmt.execute(params![
                game_id,
                s.label,
                s.unit,
                s.rank,
                f64::from(s.value),
                s.confidence,
                i64::from(s.stat_id),
                i64::from(s.ordinal),
            ])?;
            count += 1;
        }
    }
    drop(stmt);
    tx.commit()?;
    Ok(count)
}

/// `talent_abilities`: the abilities each `Talent` payload references,
/// resolved to game ids (the raw GUID when unresolved). Returns rows written.
///
/// # Errors
/// On any SQL failure.
pub fn populate_talent_abilities(conn: &Connection) -> Result<u64> {
    let talents: Vec<(String, String, Vec<u8>)> = {
        let mut stmt = conn.prepare(
            "SELECT game_id, fqn, json FROM objects WHERE kind = 'Talent' AND is_canonical = 1",
        )?;
        let raw: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        raw.into_iter()
            .filter_map(|(game_id, fqn, json_str)| {
                let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;
                let payload = BASE64.decode(v.get("payload_b64")?.as_str()?).ok()?;
                Some((game_id, fqn, payload))
            })
            .collect()
    };

    // guid -> (game_id, fqn) over all objects.
    let guid_map: HashMap<String, (String, String)> = {
        let mut stmt = conn.prepare("SELECT guid, game_id, fqn FROM objects")?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        rows.into_iter()
            .map(|(guid, game_id, fqn)| (guid.to_uppercase(), (game_id, fqn)))
            .collect()
    };

    let tx = conn.unchecked_transaction()?;
    let mut stmt = tx.prepare_cached(
        "INSERT OR IGNORE INTO talent_abilities (talent_game_id, talent_fqn, ability_game_id, ability_fqn) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut count = 0u64;
    for (talent_game_id, talent_fqn, payload) in &talents {
        for guid_hex in extract_ability_guids_from_talent(payload) {
            let entry = guid_map.get(&guid_hex);
            let ability_game_id = entry.map_or(guid_hex.as_str(), |(gid, _)| gid.as_str());
            let ability_fqn = entry.map(|(_, fqn)| fqn.as_str());
            stmt.execute(params![
                talent_game_id,
                talent_fqn,
                ability_game_id,
                ability_fqn
            ])?;
            count += 1;
        }
    }
    drop(stmt);
    tx.commit()?;
    Ok(count)
}

/// `disciplines`, `discipline_abilities` and `combat_style_shared_abilities`
/// from `abl.*` FQN placements. Returns (disciplines, discipline abilities,
/// shared ability rows).
///
/// # Errors
/// On any SQL failure.
pub fn populate_disciplines(conn: &Connection) -> Result<(u64, u64, u64)> {
    let rows = string_pairs(
        conn,
        "SELECT fqn, game_id FROM objects \
         WHERE fqn LIKE 'abl.%' AND kind = 'Ability' AND is_canonical = 1",
    )?;

    // (origin, discipline, fqn_prefix, combat_style)
    let mut disc_set: HashSet<(String, String, String, &'static str)> = HashSet::new();
    let mut abl_rows: Vec<(String, &str, &str, Option<u8>, &'static str)> = Vec::new();
    // (combat_style, game_id, fqn, slot_type): per-origin shared pools fanned
    // to both combat styles of the origin.
    let mut shared_rows: Vec<(&'static str, &str, &str, &'static str)> = Vec::new();
    for (fqn, game_id) in &rows {
        match place_ability(fqn) {
            Some(AbilityPlacement::Discipline {
                origin,
                discipline_name,
                fqn_prefix,
                combat_style,
                tier_level,
                slot_type,
            }) => {
                disc_set.insert((origin, discipline_name, fqn_prefix.clone(), combat_style));
                abl_rows.push((fqn_prefix, game_id, fqn, tier_level, slot_type));
            }
            Some(AbilityPlacement::Shared { origin, slot_type }) => {
                for combat_style in origin_combat_styles(&origin) {
                    shared_rows.push((combat_style, game_id, fqn, slot_type));
                }
            }
            None => {}
        }
    }

    let tx = conn.unchecked_transaction()?;
    let mut disc_count = 0u64;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO disciplines (origin_codename, discipline_name, fqn_prefix, combat_style_codename) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (origin, discipline_name, fqn_prefix, combat_style) in &disc_set {
            stmt.execute(params![origin, discipline_name, fqn_prefix, combat_style])?;
            disc_count += 1;
        }
    }
    let mut abl_count = 0u64;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO discipline_abilities (discipline_fqn_prefix, ability_game_id, ability_fqn, tier_level, slot_type) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (disc_prefix, game_id, fqn, tier, slot) in &abl_rows {
            stmt.execute(params![disc_prefix, game_id, fqn, tier, slot])?;
            abl_count += 1;
        }
    }
    let mut shared_count = 0u64;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO combat_style_shared_abilities \
               (combat_style_codename, ability_game_id, ability_fqn, slot_type) \
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (combat_style, game_id, fqn, slot) in &shared_rows {
            stmt.execute(params![combat_style, game_id, fqn, slot])?;
            shared_count += 1;
        }
    }
    tx.commit()?;
    Ok((disc_count, abl_count, shared_count))
}

/// `disciplines` codename/icon/mod-tree/signature columns and
/// `discipline_mods` from `dis.*` payloads (#170). Returns (disciplines
/// updated, mods inserted).
///
/// # Errors
/// On any SQL failure.
pub fn populate_disciplines_from_dis(conn: &Connection) -> Result<(u64, u64)> {
    let payloads: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT fqn, json_extract(json, '$.payload_b64') FROM objects \
             WHERE fqn LIKE 'dis.%' AND is_canonical = 1",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        rows
    };
    let guid_to_game_id: HashMap<String, String> = string_pairs(
        conn,
        "SELECT guid, game_id FROM objects WHERE guid IS NOT NULL",
    )?
    .into_iter()
    .collect();
    // Combat-style `base` apc objects: the icon fallback for disciplines
    // whose icon apc is absent from the archives.
    let base_apc_lookup: HashMap<String, String> = string_pairs(
        conn,
        "SELECT fqn, game_id FROM objects \
         WHERE fqn LIKE 'apc.%.base' AND is_canonical = 1",
    )?
    .into_iter()
    .collect();

    let tx = conn.unchecked_transaction()?;
    let mut disc_count = 0u64;
    let mut mod_count = 0u64;
    {
        let mut update_disc = tx.prepare_cached(
            "UPDATE disciplines SET \
                codename = ?1, \
                icon_apc_game_id = ?2, \
                mod_tree_apc_game_id = ?3, \
                signature_ability_game_id = ?4 \
             WHERE fqn_prefix = ?5",
        )?;
        let mut insert_mod = tx.prepare_cached(
            "INSERT OR REPLACE INTO discipline_mods \
               (discipline_fqn_prefix, mod_index, tier_ordinal, ui_position, \
                level_required, target_guid, target_game_id, is_default) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        let mut fqn_prefix_stmt = tx.prepare_cached(
            "SELECT fqn_prefix FROM disciplines \
             WHERE combat_style_codename = ?1 AND discipline_name = ?2",
        )?;

        for (dis_fqn, payload_b64) in &payloads {
            let Ok(payload) = BASE64.decode(payload_b64) else {
                continue;
            };
            let tree = match decode_discipline_tree(dis_fqn, &payload) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("dis decode failed for {dis_fqn}: {e}");
                    continue;
                }
            };
            // The codename alone is not a key: shadow.combat and
            // sentinel.combat share `sent_combat`.
            let fqn_prefix: Option<String> = fqn_prefix_stmt
                .query_row(params![tree.combat_style, tree.discipline_name], |row| {
                    row.get(0)
                })
                .ok();
            let Some(fqn_prefix) = fqn_prefix else {
                tracing::warn!(
                    "dis {dis_fqn} has no matching disciplines row (combat_style={}, name={})",
                    tree.combat_style,
                    tree.discipline_name
                );
                continue;
            };

            let icon_game_id: Option<&String> = guid_to_game_id
                .get(&tree.icon_apc_guid)
                .or_else(|| base_apc_lookup.get(&tree.fallback_icon_apc_fqn(&fqn_prefix)?));
            update_disc.execute(params![
                tree.codename,
                icon_game_id,
                guid_to_game_id.get(&tree.mod_tree_apc_guid),
                guid_to_game_id.get(&tree.signature_ability_guid),
                fqn_prefix,
            ])?;
            disc_count += 1;

            for m in &tree.mods {
                insert_mod.execute(params![
                    fqn_prefix,
                    m.mod_index,
                    m.tier_ordinal,
                    m.ui_position,
                    m.level_required,
                    m.target_guid,
                    guid_to_game_id.get(&m.target_guid),
                    i32::from(m.is_default),
                ])?;
                mod_count += 1;
            }
        }
    }
    tx.commit()?;
    Ok((disc_count, mod_count))
}

/// `discipline_talents` and `class_utility_talents` from `tal.*.skill.*`
/// FQN placements. Returns (discipline talents, utility talent rows).
///
/// # Errors
/// On any SQL failure.
pub fn populate_discipline_talents(conn: &Connection) -> Result<(u64, u64)> {
    let talents = string_pairs(
        conn,
        "SELECT fqn, game_id FROM objects \
         WHERE kind = 'Talent' AND fqn LIKE 'tal.%.skill.%' AND is_canonical = 1",
    )?;

    let tx = conn.unchecked_transaction()?;
    let mut disc_count = 0u64;
    let mut util_count = 0u64;
    {
        let mut disc_stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO discipline_talents \
             (discipline_fqn_prefix, talent_game_id, talent_fqn) VALUES (?1, ?2, ?3)",
        )?;
        let mut util_stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO class_utility_talents \
             (combat_style_codename, talent_game_id, talent_fqn) VALUES (?1, ?2, ?3)",
        )?;
        for (fqn, game_id) in &talents {
            match place_talent(fqn) {
                Some(TalentPlacement::Discipline { fqn_prefix }) => {
                    disc_stmt.execute(params![fqn_prefix, game_id, fqn])?;
                    disc_count += 1;
                }
                Some(TalentPlacement::Utility { origin }) => {
                    for combat_style in origin_combat_styles(&origin) {
                        util_stmt.execute(params![combat_style, game_id, fqn])?;
                        util_count += 1;
                    }
                }
                None => {}
            }
        }
    }
    tx.commit()?;
    Ok((disc_count, util_count))
}
