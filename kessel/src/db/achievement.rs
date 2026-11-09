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

/// Achievement object fields read for earn chains (FR-KESSEL-025).
mod ach_field {
    /// achActions: task number -> task.
    pub const ACTIONS: u32 = 0x8ce6_ac36;
    /// achEvents: event code -> (unread) per task.
    pub const EVENTS: u32 = 0x8ce6_ac37;
    /// achMaxCounter: the count a task needs.
    pub const MAX_COUNTER: u32 = 0xdeaa_a366;
    /// locTextRetrieverMap: task number -> text ref (str.ach id1 256+).
    pub const TEXT: u32 = 0xa787_ee87;
    pub const TEXT_STRING_ID: u32 = 0x5ce8_7488;
    /// Conditions, and on each: type, owner (2 = the event's target),
    /// integer-list params (key 8 = effect tag hashes), condition refs.
    pub const CONDITIONS: u32 = 0x42cc_2550;
    pub const COND_TYPE: u32 = 0xb8dc_994b;
    pub const COND_OWNER: u32 = 0x7fbd_af73;
    pub const COND_ID_LISTS: u32 = 0xb8dc_9943;
    pub const COND_REFS: u32 = 0xb8dc_9946;
}

/// Hydra hook pairs on quests and achievements: the script and the custom
/// event they send it.
const HOOK_SCRIPT: u32 = 0x4fa1_4a1f;
const HOOK_EVENT: u32 = 0xcdeb_4375;

/// Condition type 18 tests that a participant carries an effect.
const COND_HAS_EFFECT: i64 = 18;
const COND_EFFECT_TAGS: i64 = 8;
/// The log prints an effect as its owning ability's string id << 32 plus the
/// effect's index; an ability's first effect is index 256 (str.abl id1 256,
/// Party Jawa's 3044178330124544).
const FIRST_EFFECT_INDEX: i64 = 256;

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

/// One task of an achievement as its payload states it.
#[derive(Debug, PartialEq)]
struct Task {
    /// achievement_tasks.task_index: the task's text line (str.ach id1 - 256),
    /// else its number - 1.
    index: i64,
    count_needed: Option<i64>,
    event_codes: Vec<u64>,
}

/// One achievement condition.
#[derive(Debug, PartialEq)]
struct Condition {
    kind: Option<i64>,
    on_target: bool,
    effect_tags: Vec<u64>,
    refs: Vec<u64>,
}

fn achievement_tasks(obj: &GomValue) -> Vec<Task> {
    use ach_field as f;
    let text: HashMap<i64, i64> = obj
        .embedded_field(f::TEXT)
        .and_then(GomValue::as_map)
        .unwrap_or_default()
        .iter()
        .filter_map(|(k, v)| {
            let id1 = (field_bits(v, f::TEXT_STRING_ID)? & 0xFFFF_FFFF) as i64;
            Some((k.as_i64()?, id1))
        })
        .collect();
    obj.embedded_field(f::ACTIONS)
        .and_then(GomValue::as_map)
        .unwrap_or_default()
        .iter()
        .filter_map(|(k, task)| {
            let n = k.as_i64()?;
            Some(Task {
                index: text
                    .get(&n)
                    .filter(|id1| **id1 >= 256)
                    .map_or(n - 1, |id1| id1 - 256),
                count_needed: task
                    .embedded_field(f::MAX_COUNTER)
                    .and_then(GomValue::as_i64),
                event_codes: task
                    .embedded_field(f::EVENTS)
                    .and_then(GomValue::as_map)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|(code, _)| bits(code))
                    .collect(),
            })
        })
        .collect()
}

fn achievement_conditions(obj: &GomValue) -> Vec<Condition> {
    use ach_field as f;
    let entries = |c: &GomValue, id: u32| -> Vec<(GomValue, GomValue)> {
        c.embedded_field(id)
            .and_then(GomValue::as_map)
            .unwrap_or_default()
            .to_vec()
    };
    obj.embedded_field(f::CONDITIONS)
        .and_then(GomValue::as_list)
        .unwrap_or_default()
        .iter()
        .map(|c| Condition {
            kind: c.embedded_field(f::COND_TYPE).and_then(GomValue::as_i64),
            on_target: c.embedded_field(f::COND_OWNER).and_then(GomValue::as_i64) == Some(2),
            effect_tags: entries(c, f::COND_ID_LISTS)
                .iter()
                .filter(|(k, _)| k.as_i64() == Some(COND_EFFECT_TAGS))
                .flat_map(|(_, v)| v.as_list().unwrap_or_default().iter().filter_map(bits))
                .collect(),
            refs: entries(c, f::COND_REFS)
                .iter()
                .filter_map(|(_, v)| bits(v))
                .collect(),
        })
        .collect()
}

/// What an event code counts, by the kind of object or name that hashes to it.
struct EventName {
    kind: Option<&'static str>,
    name: String,
    ref_fqn: Option<String>,
}

/// The task-count kind of an object fqn, by its prefix. Kinds outside the
/// table's list (rdd, nco ...) keep their name with no kind.
fn fqn_count_kind(fqn: &str) -> Option<&'static str> {
    match fqn.split('.').next()? {
        "npc" => Some("npc"),
        "qst" => Some("quest"),
        "ach" => Some("achievement"),
        "cdx" => Some("codex"),
        "itm" => Some("item"),
        "abl" => Some("ability"),
        "plc" => Some("placeable"),
        _ => None,
    }
}

/// Whether a combat log can show a task's progress, and why not. The log
/// prints NPC kills and effects, never an achievement event: a task counted
/// only by NPC kills, under no condition but a has-effect test, is visible.
fn log_visibility(
    counted: &[Option<&str>],
    conditions: &[Condition],
) -> (bool, Option<&'static str>) {
    let reason = counted.iter().find_map(|kind| match kind {
        Some("npc") => None,
        Some("quest") => Some("quest event"),
        Some("codex") => Some("codex unlock"),
        Some("item") => Some("item"),
        _ => Some("script event"),
    });
    match reason {
        Some(r) => (false, Some(r)),
        None if counted.is_empty()
            || conditions.iter().any(|c| c.kind != Some(COND_HAS_EFFECT)) =>
        {
            (false, Some("condition"))
        }
        None => (true, None),
    }
}

/// How a script action is reached: hooked by a quest or achievement, or the
/// root handlers (On Use, On Death ...) that reach it; `uncalled` when an On
/// Custom handler on the way has no decoded caller.
#[derive(Debug, Default, PartialEq)]
struct Reach {
    hooked: bool,
    triggers: std::collections::BTreeSet<String>,
    uncalled: bool,
}

/// Follow `script`'s `handler` back to its roots. `callers` maps a lowercased
/// `script:custom event` to the (script, handler) pairs that execute it;
/// `hooks` holds the lowercased `script:custom event` pairs quests and
/// achievements send.
fn reach(
    script: &str,
    handler: &str,
    callers: &HashMap<String, Vec<(String, String)>>,
    hooks: &std::collections::HashSet<String>,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Reach,
) {
    let Some(event) = handler.strip_prefix("On Custom:") else {
        out.triggers.insert(handler.to_string());
        return;
    };
    let key = format!("{script}:{event}").to_lowercase();
    if !seen.insert(key.clone()) {
        return;
    }
    if hooks.contains(&key) {
        out.hooked = true;
    }
    match callers.get(&key) {
        Some(from) => {
            for (s, h) in from {
                reach(s, h, callers, hooks, seen, out);
            }
        }
        None if !hooks.contains(&key) => out.uncalled = true,
        None => {}
    }
}

/// Every `script:custom event` (lowercased) a hook pair anywhere in `v` sends.
fn hook_pairs(
    v: &GomValue,
    guid_fqn: &HashMap<u64, String>,
    out: &mut std::collections::HashSet<String>,
) {
    match v {
        GomValue::Embedded(fields) => {
            let script = v.embedded_field(HOOK_SCRIPT).and_then(|s| match s {
                GomValue::Str(s) => Some(s.trim_start_matches('@').to_string()),
                other => bits(other).and_then(|g| guid_fqn.get(&g).cloned()),
            });
            if let (Some(script), Some(GomValue::Str(event))) =
                (script, v.embedded_field(HOOK_EVENT))
            {
                out.insert(format!("{script}:{event}").to_lowercase());
            }
            fields
                .iter()
                .for_each(|(_, x)| hook_pairs(x, guid_fqn, out));
        }
        GomValue::List(items) => items.iter().for_each(|x| hook_pairs(x, guid_fqn, out)),
        GomValue::Map(entries) => entries
            .iter()
            .for_each(|(_, x)| hook_pairs(x, guid_fqn, out)),
        _ => {}
    }
}

impl Database {
    /// Populate `achievement_task_rules`, `achievement_task_counts` and
    /// `hidden_earnables` (FR-KESSEL-025 c1, c2, c3, c7). Runs after
    /// achievements, `script_actions` and `ability_tags`. Returns (task rules,
    /// task counts, hidden earnables).
    pub(crate) fn populate_earn_chains(&self) -> Result<(u64, u64, u64)> {
        self.with_tx(|tx| {
            let guid_fqn = object_guid_map(tx)?;

            // Event code -> the name that hashes to it: object fqns first, then
            // the names scripts raise.
            let mut names: HashMap<u64, EventName> = HashMap::new();
            {
                let mut stmt = tx.prepare("SELECT fqn FROM objects WHERE is_canonical = 1")?;
                for fqn in stmt.query_map([], |r| r.get::<_, String>(0))? {
                    let fqn = fqn?;
                    names
                        .entry(crate::hash::achievement_event_code(&fqn))
                        .or_insert_with(|| EventName {
                            kind: fqn_count_kind(&fqn),
                            name: fqn.clone(),
                            ref_fqn: Some(fqn),
                        });
                }
                let mut stmt = tx.prepare(
                    "SELECT DISTINCT value FROM script_actions \
                     WHERE action = 'Achievement Event' AND value IS NOT NULL",
                )?;
                for name in stmt.query_map([], |r| r.get::<_, String>(0))? {
                    let name = name?;
                    names
                        .entry(crate::hash::achievement_event_code(&name))
                        .or_insert(EventName {
                            kind: Some("script_event"),
                            name,
                            ref_fqn: None,
                        });
                }
            }

            // Effect tag -> (effect ability, the id the log prints for it):
            // the ability the tag is named after (tag.abl.x -> abl.x), else
            // an ability ability_tags finds carrying it. The default extract
            // keeps one object per fqn, so the tag's name is the surer key.
            let mut effects: HashMap<String, (String, Option<i64>)> = HashMap::new();
            {
                let mut stmt = tx.prepare(
                    "SELECT t.tag_hash, COALESCE(o.fqn, a.ability_fqn, substr(t.tag_fqn, 5)), \
                            COALESCE(o.string_id, ao.string_id) \
                     FROM tags t \
                     LEFT JOIN objects o ON o.fqn = substr(t.tag_fqn, 5) AND o.is_canonical = 1 \
                     LEFT JOIN ability_tags a ON a.tag_hash = t.tag_hash \
                     LEFT JOIN objects ao ON ao.fqn = a.ability_fqn AND ao.is_canonical = 1 \
                     ORDER BY a.ability_fqn",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                })?;
                for row in rows {
                    let (tag, ability, string_id) = row?;
                    effects
                        .entry(tag)
                        .or_insert((ability, string_id.map(|s| (s << 32) | FIRST_EFFECT_INDEX)));
                }
            }

            for t in ["achievement_task_rules", "achievement_task_counts", "hidden_earnables"] {
                tx.execute(&format!("DELETE FROM {t}"), [])?;
            }
            let mut ins_rule = tx.prepare(
                "INSERT OR REPLACE INTO achievement_task_rules \
                   (achievement_fqn, task_index, count_needed, log_visible, log_reason, \
                    buff_effect_fqn, buff_log_id, buff_on) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            let mut ins_count = tx.prepare(
                "INSERT OR REPLACE INTO achievement_task_counts \
                   (achievement_fqn, task_index, kind, ref_fqn, event_name, event_code) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            let mut counted_by: HashMap<u64, Vec<String>> = HashMap::new();
            let mut hooks = std::collections::HashSet::new();
            for (fqn, obj) in super::world::decoded_objects(tx, "Achievement")? {
                hook_pairs(&obj, &guid_fqn, &mut hooks);
                let conditions = achievement_conditions(&obj);
                let buff = conditions
                    .iter()
                    .filter(|c| c.kind == Some(COND_HAS_EFFECT))
                    .find_map(|c| {
                        c.effect_tags.iter().find_map(|t| {
                            effects
                                .get(&format!("{t:016X}"))
                                .map(|(abl, log_id)| (abl.clone(), *log_id, c.on_target))
                        })
                    });
                for task in achievement_tasks(&obj) {
                    let mut kinds = Vec::new();
                    for code in &task.event_codes {
                        let named = names.get(code);
                        kinds.push(named.and_then(|n| n.kind));
                        counted_by.entry(*code).or_default().push(fqn.clone());
                        ins_count.execute(params![
                            fqn,
                            task.index,
                            named.and_then(|n| n.kind),
                            named.and_then(|n| n.ref_fqn.clone()),
                            named.map(|n| n.name.clone()),
                            format!("{code:016X}"),
                        ])?;
                    }
                    for cnd in conditions.iter().flat_map(|c| &c.refs) {
                        ins_count.execute(params![
                            fqn,
                            task.index,
                            "condition",
                            guid_fqn.get(cnd),
                            None::<String>,
                            None::<String>,
                        ])?;
                    }
                    let (visible, reason) = log_visibility(&kinds, &conditions);
                    let (effect, log_id, on) = match &buff {
                        Some((abl, log_id, target)) => (
                            Some(abl.as_str()),
                            *log_id,
                            Some(if *target { "target" } else { "player" }),
                        ),
                        None => (None, None, None),
                    };
                    ins_rule.execute(params![
                        fqn,
                        task.index,
                        task.count_needed,
                        visible,
                        reason,
                        effect,
                        log_id,
                        on
                    ])?;
                }
            }
            for (_, obj) in super::world::decoded_objects(tx, "Quest")? {
                hook_pairs(&obj, &guid_fqn, &mut hooks);
            }

            // Custom events: who executes each `script:event`.
            let mut callers: HashMap<String, Vec<(String, String)>> = HashMap::new();
            let mut raised = Vec::new();
            {
                let mut stmt = tx.prepare(
                    "SELECT script_fqn, handler, action, value, selector FROM script_actions \
                     WHERE action IN ('Execute Hydra Script', 'Achievement Event') \
                       AND value IS NOT NULL",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                })?;
                for row in rows {
                    let (script, handler, action, value, selector) = row?;
                    if action == "Achievement Event" {
                        raised.push((script, handler, value, selector));
                    } else {
                        callers
                            .entry(value.to_lowercase())
                            .or_default()
                            .push((script, handler));
                    }
                }
            }

            let mut ins_hidden = tx.prepare(
                "INSERT OR REPLACE INTO hidden_earnables \
                   (event_name, event_code, script_fqn, handler, trigger, credits, achievement_fqn) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (script, handler, name, selector) in &raised {
                let mut r = Reach::default();
                reach(script, handler, &callers, &hooks, &mut Default::default(), &mut r);
                if r.hooked {
                    continue;
                }
                let trigger = r.triggers.iter().next().map_or("On Custom", String::as_str);
                let code = crate::hash::achievement_event_code(name);
                let achievements = counted_by.get(&code);
                let achievements: Vec<Option<&str>> = match achievements {
                    Some(a) => a.iter().map(|a| Some(a.as_str())).collect(),
                    None => vec![None],
                };
                for ach in achievements {
                    ins_hidden.execute(params![
                        name,
                        format!("{code:016X}"),
                        script,
                        handler,
                        trigger,
                        selector,
                        ach
                    ])?;
                }
            }
            // Rows written, not inserts: an action repeated in one handler
            // lands on the same hidden_earnables row.
            let rows = |t: &str| -> Result<u64> {
                Ok(tx.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?)
            };
            Ok((
                rows("achievement_task_rules")?,
                rows("achievement_task_counts")?,
                rows("hidden_earnables")?,
            ))
        })
    }
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
            -- Earn chains (FR-KESSEL-025), joined to achievement_tasks on
            -- (achievement_fqn, task_index). A task's rule: the count it
            -- needs; whether a combat log can show its progress (NPC kills,
            -- logged effects) and when not why (script event, quest event,
            -- codex unlock, item, condition); and the effect a has-effect
            -- condition requires, with the id the log prints for its
            -- ApplyEffect and whom it must sit on (player or target).
            CREATE TABLE IF NOT EXISTS achievement_task_rules (
                achievement_fqn TEXT NOT NULL,
                task_index      INTEGER NOT NULL,
                count_needed    INTEGER,
                log_visible     INTEGER NOT NULL,
                log_reason      TEXT,
                buff_effect_fqn TEXT,
                buff_log_id     INTEGER,
                buff_on         TEXT,
                PRIMARY KEY (achievement_fqn, task_index)
            );
            -- What a task counts: each achievement event code (E000 + the
            -- 48-bit fold of FNV-1a 64 over the uppercased name) with the
            -- name and kind that hash to it (npc, quest, achievement, codex,
            -- item, ability, placeable, script_event), and each condition
            -- the achievement names (kind condition, no code).
            CREATE TABLE IF NOT EXISTS achievement_task_counts (
                achievement_fqn TEXT NOT NULL,
                task_index      INTEGER NOT NULL,
                kind            TEXT,
                ref_fqn         TEXT,
                event_name      TEXT,
                event_code      TEXT,
                PRIMARY KEY (achievement_fqn, task_index, event_code)
            );
            CREATE INDEX IF NOT EXISTS idx_achievement_task_counts_ref ON achievement_task_counts(ref_fqn);
            CREATE INDEX IF NOT EXISTS idx_achievement_task_counts_code ON achievement_task_counts(event_code);
            -- Every Achievement Event a script raises that no quest or
            -- achievement hook reaches: the root handler that does (On Use,
            -- On Death ..., On Custom when no caller is decoded), the
            -- selector it credits, and each achievement counting the event.
            CREATE TABLE IF NOT EXISTS hidden_earnables (
                event_name      TEXT NOT NULL,
                event_code      TEXT NOT NULL,
                script_fqn      TEXT NOT NULL,
                handler         TEXT NOT NULL,
                trigger         TEXT NOT NULL,
                credits         TEXT,
                achievement_fqn TEXT,
                PRIMARY KEY (event_code, script_fqn, handler, achievement_fqn)
            );
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

    fn emb(fields: Vec<(u32, GomValue)>) -> GomValue {
        GomValue::Embedded(
            fields
                .into_iter()
                .map(|(id, v)| (0x4000_0000_0000_0000 | u64::from(id), v))
                .collect(),
        )
    }

    #[test]
    fn achievement_tasks_read_count_codes_and_text_line() {
        use ach_field as f;
        let meatbag = 0xE000_39BA_78F5_3C4C_u64;
        let task = |codes: Vec<u64>, count: i64| {
            emb(vec![
                (
                    f::EVENTS,
                    GomValue::Map(
                        codes
                            .into_iter()
                            .map(|c| (GomValue::I64(c as i64), GomValue::I64(0)))
                            .collect(),
                    ),
                ),
                (f::MAX_COUNTER, GomValue::I64(count)),
            ])
        };
        let obj = emb(vec![
            (
                f::ACTIONS,
                GomValue::Map(vec![
                    (GomValue::I64(1), task(vec![meatbag], 1)),
                    (GomValue::I64(2), task(vec![1, 2], 25)),
                ]),
            ),
            (
                f::TEXT,
                GomValue::Map(vec![(
                    GomValue::I64(2),
                    emb(vec![(f::TEXT_STRING_ID, GomValue::I64((7 << 32) | 259))]),
                )]),
            ),
        ]);
        assert_eq!(
            achievement_tasks(&obj),
            [
                Task {
                    index: 0,
                    count_needed: Some(1),
                    event_codes: vec![meatbag]
                },
                Task {
                    index: 3,
                    count_needed: Some(25),
                    event_codes: vec![1, 2]
                },
            ]
        );
    }

    #[test]
    fn achievement_conditions_read_effect_tags_and_owner() {
        use ach_field as f;
        let party_jawa = 0x151C_0E05_3876_C9C6_u64;
        let obj = emb(vec![(
            f::CONDITIONS,
            GomValue::List(vec![
                emb(vec![
                    (f::COND_TYPE, GomValue::Enum(18)),
                    (f::COND_OWNER, GomValue::Enum(2)),
                    (
                        f::COND_ID_LISTS,
                        GomValue::Map(vec![(
                            GomValue::Enum(8),
                            GomValue::List(vec![GomValue::U64(party_jawa)]),
                        )]),
                    ),
                ]),
                emb(vec![
                    (f::COND_TYPE, GomValue::Enum(20)),
                    (
                        f::COND_REFS,
                        GomValue::Map(vec![(
                            GomValue::Enum(20),
                            GomValue::U64(0xE000_0000_0000_0009),
                        )]),
                    ),
                ]),
            ]),
        )]);
        assert_eq!(
            achievement_conditions(&obj),
            [
                Condition {
                    kind: Some(18),
                    on_target: true,
                    effect_tags: vec![party_jawa],
                    refs: vec![]
                },
                Condition {
                    kind: Some(20),
                    on_target: false,
                    effect_tags: vec![],
                    refs: vec![0xE000_0000_0000_0009]
                },
            ]
        );
    }

    #[test]
    fn log_visibility_shows_only_npc_kills_under_effect_conditions() {
        let cond = |kind| Condition {
            kind: Some(kind),
            on_target: false,
            effect_tags: vec![],
            refs: vec![],
        };
        assert_eq!(log_visibility(&[Some("npc")], &[]), (true, None));
        assert_eq!(log_visibility(&[Some("npc")], &[cond(18)]), (true, None));
        assert_eq!(
            log_visibility(&[Some("npc")], &[cond(29)]),
            (false, Some("condition"))
        );
        assert_eq!(
            log_visibility(&[Some("script_event")], &[]),
            (false, Some("script event"))
        );
        assert_eq!(
            log_visibility(&[Some("npc"), Some("quest")], &[]),
            (false, Some("quest event"))
        );
        assert_eq!(
            log_visibility(&[Some("codex")], &[]),
            (false, Some("codex unlock"))
        );
        assert_eq!(log_visibility(&[Some("item")], &[]), (false, Some("item")));
        assert_eq!(log_visibility(&[None], &[]), (false, Some("script event")));
    }

    #[test]
    fn reach_follows_custom_handlers_to_roots_and_hooks() {
        let callers: HashMap<String, Vec<(String, String)>> = [
            (
                "hyd.a:found".to_string(),
                vec![("hyd.b".to_string(), "On Use".to_string())],
            ),
            (
                "hyd.c:step".to_string(),
                vec![("hyd.c".to_string(), "On Custom:quest".to_string())],
            ),
        ]
        .into_iter()
        .collect();
        let hooks: std::collections::HashSet<String> = ["hyd.c:quest".to_string()].into();
        let run = |script: &str, handler: &str| {
            let mut r = Reach::default();
            reach(
                script,
                handler,
                &callers,
                &hooks,
                &mut Default::default(),
                &mut r,
            );
            r
        };
        let r = run("hyd.a", "On Custom:Found");
        assert!(!r.hooked && !r.uncalled);
        assert_eq!(r.triggers.into_iter().collect::<Vec<_>>(), ["On Use"]);
        assert!(run("hyd.c", "On Custom:step").hooked);
        let r = run("hyd.d", "On Custom:nobody");
        assert!(r.uncalled && r.triggers.is_empty() && !r.hooked);
        assert_eq!(run("hyd.d", "On Death").triggers.len(), 1);
    }

    #[test]
    fn earn_chain_tables_exist_after_schema_init() {
        let path = crate::db::testutil::temp_db_path("earn_chain_tables");
        let db = Database::with_grammar(&path, None).unwrap();
        db.init_schema().unwrap();
        assert_eq!(db.populate_earn_chains().unwrap(), (0, 0, 0));
    }
}
