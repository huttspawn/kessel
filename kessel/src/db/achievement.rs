//! Achievements, their reference graph, and Ventures (8.0).
//!
//! Achievements (`ach.*` objects) carry their text in `str.ach` (keyed by the
//! object's string_id: id1 0 = name, 1 = description, 256+ = one line per
//! task) and their wiring as CF + 8-byte GUID refs in the payload: the gating
//! condition, target quests, the exact NPC templates a kill task counts, and
//! child achievements (a category's `complete_all` lists its tasks; a stage's
//! `set_N_meta` lists its categories).
//!
//! Ventures are defined by the `lgcVentures_Prototype` singleton: a map of
//! venture id -> record. Each record holds stages; each stage points at its
//! `set_N_meta` achievement and holds categories; each category lists the
//! achievements that are its objectives. Names are string refs into
//! `gui/achievementcategories.stb`, packed as `(id2 << 32) | id1`.
//!
//! Field ids (low32) were read off the 7.9.1d and 8.0 PTS tables; they are
//! identical across both builds (8.0 adds the class and kind fields).

use super::*;
use crate::gom_reader::{read_first_field, GomValue};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::collections::HashMap;

/// `lgcVentures_Prototype` record fields.
mod venture_field {
    pub const NAME: u32 = 0x018082d5;
    pub const TYPE: u32 = 0x018082d6;
    pub const CLASS: u32 = 0xfaef2fb6;
    pub const STAGES: u32 = 0x04131494;
    pub const STAGE_NUMBER: u32 = 0x04131492;
    pub const STAGE_META: u32 = 0x04131493;
    pub const STAGE_THEME: u32 = 0x064b2a40;
    pub const STAGE_CATEGORIES: u32 = 0x5d2604c7;
    pub const CATEGORY_NAME: u32 = 0x5d2604c2;
    pub const CATEGORY_ICON: u32 = 0x5d2604c3;
    pub const CATEGORY_ID: u32 = 0x5d2604c5;
    pub const CATEGORY_OBJECTIVES: u32 = 0x5d2604c6;
}

/// `achRewardsTable_Prototype` entry fields.
mod reward_field {
    pub const POINTS: u32 = 0x0f72cd20;
    pub const ITEMS: u32 = 0x0f72cd25;
    pub const ITEM: u32 = 0x0f72cd22;
    pub const QUANTITY: u32 = 0x0f72cd23;
}

/// The raw 64 bits of a numeric GOM value. Ids and GUIDs arrive as either
/// UInt64 or Int64 depending on the field; both are the same bits.
fn bits(v: &GomValue) -> Option<u64> {
    match v {
        GomValue::U64(n) => Some(*n),
        GomValue::I64(n) => Some(*n as u64),
        _ => None,
    }
}

fn field_bits(v: &GomValue, id: u32) -> Option<u64> {
    v.embedded_field(id).and_then(bits)
}

/// Venture kind from the record's type code. 6 = Classic (Veteran), 9 =
/// Expert (Master), 11 = the Basilisk Prototype companion venture.
pub(crate) fn venture_kind(type_code: i64) -> &'static str {
    match type_code {
        6 => "classic",
        9 => "expert",
        11 => "companion",
        _ => "unknown",
    }
}

/// Class name from the record's class index. Only meaningful for Classic and
/// Expert ventures; the companion venture carries a placeholder.
pub(crate) fn venture_class(class_index: i64) -> Option<&'static str> {
    match class_index {
        1 => Some("jedi_knight"),
        2 => Some("jedi_consular"),
        3 => Some("smuggler"),
        4 => Some("trooper"),
        5 => Some("sith_warrior"),
        6 => Some("sith_inquisitor"),
        7 => Some("agent"),
        8 => Some("bounty_hunter"),
        _ => None,
    }
}

/// Every CF + 8-byte value in `payload` that names a known object, in first-
/// seen order, deduplicated, excluding `self_guid`.
pub(crate) fn payload_refs(
    payload: &[u8],
    guids: &HashMap<u64, usize>,
    self_guid: u64,
) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= payload.len() {
        if payload[i] == 0xCF {
            let g = u64::from_be_bytes(payload[i + 1..i + 9].try_into().unwrap());
            if g != self_guid {
                if let Some(&idx) = guids.get(&g) {
                    if !out.contains(&idx) {
                        out.push(idx);
                    }
                    i += 9;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

impl Database {
    /// Populate `achievements`, `achievement_tasks` and `achievement_refs`
    /// from the canonical `ach.*` objects. Returns (achievements, tasks, refs).
    pub fn populate_achievements(&self) -> Result<(u64, u64, u64)> {
        self.flush()?;
        let conn = self.conn.lock().unwrap();

        // GUID -> (fqn, kind) for every canonical object: the ref targets.
        let mut targets: Vec<(String, String)> = Vec::new();
        let mut guids: HashMap<u64, usize> = HashMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT guid, fqn, kind FROM objects WHERE is_canonical = 1")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for (guid, fqn, kind) in rows.filter_map(|r| r.ok()) {
                if let Ok(g) = u64::from_str_radix(&guid, 16) {
                    guids.insert(g, targets.len());
                    targets.push((fqn, kind));
                }
            }
        }

        // str.ach text by string id: id1 -> text.
        let mut text: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT id2, id1, text FROM strings \
                 WHERE fqn LIKE 'str.ach.%' AND locale = 'en-us' AND text <> ''",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for (id2, id1, t) in rows.filter_map(|r| r.ok()) {
                text.entry(id2).or_default().push((id1, t));
            }
        }

        let achievements: Vec<(String, String, String, Option<i64>, String)> = {
            let mut stmt = conn.prepare(
                "SELECT fqn, game_id, guid, string_id, json_extract(json, '$.payload_b64') \
                 FROM objects WHERE kind = 'Achievement' AND is_canonical = 1",
            )?;
            let collected = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM achievement_refs", [])?;
        tx.execute("DELETE FROM achievement_tasks", [])?;
        tx.execute("DELETE FROM achievements", [])?;
        let mut ins_ach = tx.prepare_cached(
            "INSERT OR REPLACE INTO achievements \
               (fqn, game_id, string_id, name, description, task_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut ins_task = tx.prepare_cached(
            "INSERT OR REPLACE INTO achievement_tasks (achievement_fqn, task_index, text) \
             VALUES (?1, ?2, ?3)",
        )?;
        let mut ins_ref = tx.prepare_cached(
            "INSERT OR REPLACE INTO achievement_refs \
               (achievement_fqn, ordinal, ref_fqn, ref_kind) VALUES (?1, ?2, ?3, ?4)",
        )?;

        let (mut n_ach, mut n_task, mut n_ref) = (0u64, 0u64, 0u64);
        for (fqn, game_id, guid, string_id, payload_b64) in &achievements {
            let lines = string_id.and_then(|s| text.get(&s));
            let line = |id1: i64| {
                lines.and_then(|l| l.iter().find(|(i, _)| *i == id1).map(|(_, t)| t.clone()))
            };
            let mut tasks: Vec<&(i64, String)> = lines
                .map(|l| l.iter().filter(|(i, _)| *i >= 256).collect())
                .unwrap_or_default();
            tasks.sort_by_key(|(i, _)| *i);

            ins_ach.execute(params![
                fqn,
                game_id,
                string_id,
                line(0),
                line(1),
                tasks.len() as i64
            ])?;
            n_ach += 1;
            for (id1, t) in tasks {
                ins_task.execute(params![fqn, id1 - 256, t])?;
                n_task += 1;
            }

            let Ok(payload) = BASE64.decode(payload_b64) else {
                continue;
            };
            let self_guid = u64::from_str_radix(guid, 16).unwrap_or(0);
            for (ordinal, idx) in payload_refs(&payload, &guids, self_guid)
                .into_iter()
                .enumerate()
            {
                let (ref_fqn, ref_kind) = &targets[idx];
                ins_ref.execute(params![fqn, ordinal as i64, ref_fqn, ref_kind])?;
                n_ref += 1;
            }
        }
        drop(ins_ach);
        drop(ins_task);
        drop(ins_ref);
        tx.commit()?;
        Ok((n_ach, n_task, n_ref))
    }

    /// Populate `ventures`, `venture_stages`, `venture_categories` and
    /// `venture_objectives` from `lgcVentures_Prototype`. Returns
    /// (ventures, objectives). Missing singleton (pre-Ventures builds) -> 0.
    pub fn populate_ventures(&self) -> Result<(u64, u64)> {
        use venture_field as f;

        let Some(payload) = self.load_singleton_payload("lgcVentures_Prototype") else {
            return Ok((0, 0));
        };
        let table = read_first_field(&payload)?;

        self.with_tx(|tx| {
            let guid_fqn = achievement_guid_map(tx)?;
            let fqn_of =
                |v: Option<&GomValue>| v.and_then(bits).and_then(|g| guid_fqn.get(&g).cloned());

            for t in [
                "venture_objectives",
                "venture_categories",
                "venture_stages",
                "ventures",
            ] {
                tx.execute(&format!("DELETE FROM {t}"), [])?;
            }
            let mut ins_venture = tx.prepare_cached(
                "INSERT OR REPLACE INTO ventures \
                   (venture_id, name, kind, type_code, class, class_index, stage_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            let mut ins_stage = tx.prepare_cached(
                "INSERT OR REPLACE INTO venture_stages \
                   (venture_id, stage, meta_achievement_fqn, name, theme) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut ins_category = tx.prepare_cached(
                "INSERT OR REPLACE INTO venture_categories \
                   (venture_id, stage, ordinal, category_id, name, icon) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut ins_objective = tx.prepare_cached(
                "INSERT OR REPLACE INTO venture_objectives \
                   (venture_id, stage, category_id, ordinal, achievement_fqn) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            let (mut n_venture, mut n_objective) = (0u64, 0u64);
            for (key, record) in table.as_map().unwrap_or(&[]) {
                let Some(venture_id) = bits(key).map(|b| b as i64) else {
                    continue;
                };
                let type_code = record
                    .embedded_field(f::TYPE)
                    .and_then(GomValue::as_i64)
                    .unwrap_or(0);
                let kind = venture_kind(type_code);
                let class_index = record.embedded_field(f::CLASS).and_then(GomValue::as_i64);
                let class = match kind {
                    "classic" | "expert" => class_index.and_then(venture_class),
                    _ => None,
                };
                let stages = record
                    .embedded_field(f::STAGES)
                    .and_then(GomValue::as_list)
                    .unwrap_or(&[]);
                ins_venture.execute(params![
                    venture_id,
                    field_bits(record, f::NAME).and_then(|r| gui_string(tx, r)),
                    kind,
                    type_code,
                    class,
                    class_index,
                    stages.len() as i64
                ])?;
                n_venture += 1;

                for (si, stage) in stages.iter().enumerate() {
                    let number = stage
                        .embedded_field(f::STAGE_NUMBER)
                        .and_then(GomValue::as_i64)
                        .unwrap_or(si as i64 + 1);
                    let meta = fqn_of(stage.embedded_field(f::STAGE_META));
                    let stage_name = meta.as_deref().and_then(|m| achievement_name(tx, m));
                    let theme = match stage.embedded_field(f::STAGE_THEME) {
                        Some(GomValue::Str(s)) => Some(s.clone()),
                        _ => None,
                    };
                    ins_stage.execute(params![venture_id, number, meta, stage_name, theme])?;

                    let categories = stage
                        .embedded_field(f::STAGE_CATEGORIES)
                        .and_then(GomValue::as_list)
                        .unwrap_or(&[]);
                    for (ci, cat) in categories.iter().enumerate() {
                        let category_id = field_bits(cat, f::CATEGORY_ID).map(|b| b as i64);
                        let icon = match cat.embedded_field(f::CATEGORY_ICON) {
                            Some(GomValue::Str(s)) => Some(s.clone()),
                            _ => None,
                        };
                        ins_category.execute(params![
                            venture_id,
                            number,
                            ci as i64,
                            category_id,
                            field_bits(cat, f::CATEGORY_NAME).and_then(|r| gui_string(tx, r)),
                            icon
                        ])?;
                        let objectives = cat
                            .embedded_field(f::CATEGORY_OBJECTIVES)
                            .and_then(GomValue::as_list)
                            .unwrap_or(&[]);
                        for (oi, obj) in objectives.iter().enumerate() {
                            let Some(ach) = fqn_of(Some(obj)) else {
                                continue;
                            };
                            ins_objective.execute(params![
                                venture_id,
                                number,
                                category_id,
                                oi as i64,
                                ach
                            ])?;
                            n_objective += 1;
                        }
                    }
                }
            }
            Ok((n_venture, n_objective))
        })
    }

    /// Populate `achievement_reward_items` from `achRewardsTable_Prototype`:
    /// reward id -> points + item grants. Returns rows written.
    pub fn populate_achievement_rewards(&self) -> Result<u64> {
        use reward_field as f;

        let Some(payload) = self.load_singleton_payload("achRewardsTable_Prototype") else {
            return Ok(0);
        };
        let table = read_first_field(&payload)?;

        self.with_tx(|tx| {
            let guid_fqn = object_guid_map(tx)?;
            tx.execute("DELETE FROM achievement_reward_items", [])?;
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO achievement_reward_items \
                   (reward_id, points, ordinal, item_fqn, quantity) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut written = 0u64;
            for (key, entry) in table.as_map().unwrap_or(&[]) {
                let Some(reward_id) = bits(key).map(|b| b as i64) else {
                    continue;
                };
                let points = entry.embedded_field(f::POINTS).and_then(GomValue::as_i64);
                let items = entry
                    .embedded_field(f::ITEMS)
                    .and_then(GomValue::as_list)
                    .unwrap_or(&[]);
                if items.is_empty() {
                    insert.execute(params![
                        reward_id,
                        points,
                        0i64,
                        None::<String>,
                        None::<i64>
                    ])?;
                    written += 1;
                }
                for (ordinal, item) in items.iter().enumerate() {
                    let item_fqn =
                        field_bits(item, f::ITEM).and_then(|g| guid_fqn.get(&g).cloned());
                    let quantity = item.embedded_field(f::QUANTITY).and_then(GomValue::as_i64);
                    insert.execute(params![
                        reward_id,
                        points,
                        ordinal as i64,
                        item_fqn,
                        quantity
                    ])?;
                    written += 1;
                }
            }
            Ok(written)
        })
    }
}

/// GUID -> FQN for canonical achievements.
fn achievement_guid_map(tx: &Transaction) -> Result<HashMap<u64, String>> {
    guid_map(
        tx,
        "SELECT guid, fqn FROM objects WHERE kind = 'Achievement' AND is_canonical = 1",
    )
}

/// GUID -> FQN for every canonical object.
fn object_guid_map(tx: &Transaction) -> Result<HashMap<u64, String>> {
    guid_map(tx, "SELECT guid, fqn FROM objects WHERE is_canonical = 1")
}

fn guid_map(tx: &Transaction, sql: &str) -> Result<HashMap<u64, String>> {
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows
        .filter_map(|r| r.ok())
        .filter_map(|(g, f)| u64::from_str_radix(&g, 16).ok().map(|g| (g, f)))
        .collect())
}

/// Resolve a packed `(id2 << 32) | id1` string ref, preferring `str.gui.*`
/// (venture, stage and category names live in gui/achievementcategories.stb).
fn gui_string(tx: &Transaction, packed: u64) -> Option<String> {
    let (id2, id1) = ((packed >> 32) as i64, (packed & 0xFFFF_FFFF) as i64);
    tx.query_row(
        "SELECT text FROM strings WHERE id2 = ?1 AND id1 = ?2 AND locale = 'en-us' AND text <> '' \
         ORDER BY (fqn LIKE 'str.gui.%') DESC LIMIT 1",
        params![id2, id1],
        |r| r.get(0),
    )
    .ok()
}

/// An achievement's display name (str.ach id1 0) by FQN.
fn achievement_name(tx: &Transaction, fqn: &str) -> Option<String> {
    tx.query_row(
        "SELECT s.text FROM objects o JOIN strings s ON s.id2 = o.string_id \
         WHERE o.fqn = ?1 AND o.is_canonical = 1 AND s.id1 = 0 AND s.fqn LIKE 'str.ach.%' \
           AND s.text <> '' LIMIT 1",
        [fqn],
        |r| r.get(0),
    )
    .ok()
}

/// Create the achievement and venture tables (idempotent).
pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        r#"
            -- Achievements (ach.* objects). Text from str.ach by string_id:
            -- id1 0 = name, 1 = description, 256+ = task lines.
            CREATE TABLE IF NOT EXISTS achievements (
                fqn         TEXT PRIMARY KEY,
                game_id     TEXT NOT NULL,
                string_id   INTEGER,
                name        TEXT,
                description TEXT,
                task_count  INTEGER NOT NULL DEFAULT 0
            );
            -- One row per task line (str.ach id1 - 256), e.g. "Black Sun
            -- Thugs Defeated" / "Black Sun Minions Defeated".
            CREATE TABLE IF NOT EXISTS achievement_tasks (
                achievement_fqn TEXT NOT NULL,
                task_index      INTEGER NOT NULL,
                text            TEXT NOT NULL,
                PRIMARY KEY (achievement_fqn, task_index)
            );
            -- Payload GUID refs, first-seen order, deduplicated: gating
            -- conditions (cnd), target quests (qst), counted NPC templates
            -- (npc), child achievements (ach), and anything else named.
            CREATE TABLE IF NOT EXISTS achievement_refs (
                achievement_fqn TEXT NOT NULL,
                ordinal         INTEGER NOT NULL,
                ref_fqn         TEXT NOT NULL,
                ref_kind        TEXT NOT NULL,
                PRIMARY KEY (achievement_fqn, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_achievement_refs_ref ON achievement_refs(ref_fqn);
            CREATE INDEX IF NOT EXISTS idx_achievement_refs_kind ON achievement_refs(ref_kind);

            -- Ventures (lgcVentures_Prototype). kind: classic (type 6,
            -- Veteran), expert (type 9, Master), companion (type 11, the
            -- Basilisk Prototype). class only for classic/expert.
            CREATE TABLE IF NOT EXISTS ventures (
                venture_id  INTEGER PRIMARY KEY,
                name        TEXT,
                kind        TEXT NOT NULL,
                type_code   INTEGER NOT NULL,
                class       TEXT,
                class_index INTEGER,
                stage_count INTEGER NOT NULL
            );
            -- A stage completes when its meta achievement (set_N_meta) does.
            CREATE TABLE IF NOT EXISTS venture_stages (
                venture_id           INTEGER NOT NULL,
                stage                INTEGER NOT NULL,
                meta_achievement_fqn TEXT,
                name                 TEXT,
                theme                TEXT,
                PRIMARY KEY (venture_id, stage)
            );
            -- Categories within a stage (Classic: Story/Exploration/Combat/
            -- General; Expert: Exploration/Heroic Missions/High Value Targets
            -- and expansion-era groups). icon is the UI icon key.
            CREATE TABLE IF NOT EXISTS venture_categories (
                venture_id  INTEGER NOT NULL,
                stage       INTEGER NOT NULL,
                ordinal     INTEGER NOT NULL,
                category_id INTEGER,
                name        TEXT,
                icon        TEXT,
                PRIMARY KEY (venture_id, stage, ordinal)
            );
            -- Objective achievements per category, in table order. Join to
            -- achievements / achievement_tasks / achievement_refs for text,
            -- target quests and counted NPCs.
            CREATE TABLE IF NOT EXISTS venture_objectives (
                venture_id      INTEGER NOT NULL,
                stage           INTEGER NOT NULL,
                category_id     INTEGER,
                ordinal         INTEGER NOT NULL,
                achievement_fqn TEXT NOT NULL,
                PRIMARY KEY (venture_id, stage, category_id, ordinal)
            );
            CREATE INDEX IF NOT EXISTS idx_venture_objectives_ach ON venture_objectives(achievement_fqn);

            -- achRewardsTable_Prototype: reward id -> points + item grants.
            -- The link from an achievement to its reward id is not decoded
            -- yet; rows are keyed by the table's own id.
            CREATE TABLE IF NOT EXISTS achievement_reward_items (
                reward_id INTEGER NOT NULL,
                points    INTEGER,
                ordinal   INTEGER NOT NULL,
                item_fqn  TEXT,
                quantity  INTEGER,
                PRIMARY KEY (reward_id, ordinal)
            );
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venture_kind_maps_type_codes() {
        assert_eq!(venture_kind(6), "classic");
        assert_eq!(venture_kind(9), "expert");
        assert_eq!(venture_kind(11), "companion");
        assert_eq!(venture_kind(3), "unknown");
    }

    #[test]
    fn venture_class_maps_indices() {
        assert_eq!(venture_class(1), Some("jedi_knight"));
        assert_eq!(venture_class(7), Some("agent"));
        assert_eq!(venture_class(8), Some("bounty_hunter"));
        assert_eq!(venture_class(0), None);
    }

    #[test]
    fn payload_refs_dedupes_in_order_and_skips_self() {
        let guids: HashMap<u64, usize> = [
            (0xE000_0000_0000_0001, 0),
            (0xE000_0000_0000_0002, 1),
            (0xE000_0000_0000_0003, 2),
        ]
        .into_iter()
        .collect();
        let mut payload = vec![0x00, 0x01];
        for g in [3u64, 1, 3, 2, 9] {
            payload.push(0xCF);
            payload.extend_from_slice(&(0xE000_0000_0000_0000u64 | g).to_be_bytes());
            payload.push(0x05);
        }
        // guid ...02 is "self"; ...09 is unknown.
        assert_eq!(
            payload_refs(&payload, &guids, 0xE000_0000_0000_0002),
            vec![2, 0]
        );
    }
}
