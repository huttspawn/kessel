//! What each conversation node does to the player (#284, FR-KESSEL-025 c4/c9): the quests it
//! grants, advances or ends, the quest hook it fires, the condition that gates it, the items,
//! credits, influence and alignment it gives, the codex entries it shows, and the scripts it
//! calls. Every value is read from the node's own fields after the 33-byte preamble.
//!
//! Without an fqn: a census over every conversation (how many nodes carry each hook, every
//! object reference by field and kind, every Hydra action name). With an fqn: one JSON line per
//! node that does something.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_node_grants -- <spice.sqlite> [fqn]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_object_fields, GomValue, Reader};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

const EMBEDDED: u8 = 0x09;
const PREAMBLE: usize = 33;

const NODES: u32 = 0x6683_c66d;
const LINK_NODES: u32 = 0x6683_c66e;
const NODE_ID: u32 = 0x24c9_78e5;
const CHILDREN: u32 = 0x24c9_78b9;
const LINK_TARGET: u32 = 0x24cd_478e;
const IS_PC: u32 = 0x259e_02f8;
const TEXT_MAP: u32 = 0xa787_ee87;
const TEXT_REF: u32 = 0x5ce8_7488;
const SPEAKER: u32 = 0xada8_873c;
const QUEST_GRANTS: u32 = 0xc458_52d0;
const QUEST_ENDS: u32 = 0xc458_52d1;
const QUEST_PROGRESS: u32 = 0xc458_52d2;
const QUEST_HOOK_IDS: u32 = 0x6428_709a;
const ACTION_QUEST: u32 = 0xb21f_4106;
const ACTION_HOOK: u32 = 0x0336_685a;
const ACTION_HOOK_FLAGS: u32 = 0x0336_685b;
const REWARD_QUEST: u32 = 0x6f48_5733;
const CONDITIONAL: u32 = 0x6584_d6d0;
const DEBIT: u32 = 0x946e_6bf1;
const CREDIT: u32 = 0x946e_6bf2;
const TX_ITEMS: u32 = 0x9a7b_a580;
const TX_INFLUENCE: u32 = 0x9a7b_a581;
const TX_CREDITS: u32 = 0x946e_6bfb;
const FORCE_TYPE: u32 = 0xa744_d62a;
const FORCE_AMOUNT: u32 = 0x8c27_9053;
const REWARD_CREDITS: u32 = 0x8c27_9058;
const NODE_ACTION: u32 = 0x0e06_2ca2;
const NODE_ACTION_ARG: u32 = 0x0e06_2ca3;
const SCRIPT_CALLS: u32 = 0x2bd6_139d;
const SCRIPT_CALL_HYDRA: u32 = 0x2bd6_1399;
const SCRIPT_CALL_EVENT: u32 = 0x2bd6_139a;
const HYD_ACTION: u32 = 0xe534_6fa0;
const HYD_VALUE: u32 = 0xe534_6faf;

// Quest hook list (kessel/src/db/quest.rs): entries keyed by variable id, with a state id
// (the value a hook sets) and a name.
const QST_HOOK_LIST: u32 = 0xad64_0052;
const QST_HOOK_KEY: u32 = 0xb21e_56a4;
const QST_HOOK_STATE: u32 = 0x6428_709b;
const QST_HOOK_NAME: u32 = 0x2ade_c3c7;
const QST_BRANCHES: u32 = 0x05a2_a6d3;
const QST_STEPS: u32 = 0x05a2_a6d1;
const QST_TASKS: u32 = 0x05a2_a6d0;
const QST_TASK_HOOK: u32 = 0x6428_7098;

type Objects = HashMap<u64, (String, String)>;

fn guid_of(v: &GomValue) -> Option<u64> {
    match v {
        GomValue::U64(n) => Some(*n),
        GomValue::I64(n) => Some(*n as u64),
        _ => None,
    }
}

fn name(objects: &Objects, id: u64) -> Value {
    match objects.get(&id) {
        Some((kind, fqn)) => json!(format!("{kind}:{fqn}")),
        None => json!(format!("?{id:016X}")),
    }
}

fn map_keys(node: &GomValue, field: u32) -> Vec<u64> {
    node.embedded_field(field)
        .and_then(GomValue::as_map)
        .into_iter()
        .flatten()
        .filter_map(|(k, _)| guid_of(k))
        .collect()
}

/// Every numeric value anywhere under `v` that names an object, with the low 32 bits of the
/// field that holds it.
fn refs(v: &GomValue, field: u32, objects: &Objects, out: &mut Vec<(u32, u64)>) {
    match v {
        GomValue::U64(_) | GomValue::I64(_) => {
            if let Some(id) = guid_of(v).filter(|id| objects.contains_key(id)) {
                out.push((field, id));
            }
        }
        GomValue::List(items) => items.iter().for_each(|i| refs(i, field, objects, out)),
        GomValue::Map(pairs) => pairs.iter().for_each(|(k, val)| {
            refs(k, field, objects, out);
            refs(val, field, objects, out);
        }),
        GomValue::Embedded(fields) => fields
            .iter()
            .for_each(|(id, val)| refs(val, *id as u32, objects, out)),
        _ => {}
    }
}

/// Every Hydra action name (with its value) anywhere under `v`.
fn hydra_actions(v: &GomValue, out: &mut Vec<(String, String)>) {
    match v {
        GomValue::List(items) => items.iter().for_each(|i| hydra_actions(i, out)),
        GomValue::Map(pairs) => pairs.iter().for_each(|(_, val)| hydra_actions(val, out)),
        GomValue::Embedded(fields) => {
            if let Some(GomValue::Str(action)) = v.embedded_field(HYD_ACTION) {
                let value = match v.embedded_field(HYD_VALUE) {
                    Some(GomValue::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                out.push((action.clone(), value));
            }
            fields.iter().for_each(|(_, val)| hydra_actions(val, out));
        }
        _ => {}
    }
}

fn transaction(node: &GomValue, field: u32, objects: &Objects) -> Value {
    let Some(tx) = node.embedded_field(field) else {
        return Value::Null;
    };
    let pairs = |id: u32| -> Vec<Value> {
        tx.embedded_field(id)
            .and_then(GomValue::as_map)
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| Some(json!([name(objects, guid_of(k)?), v.as_i64()])))
            .collect()
    };
    let items = pairs(TX_ITEMS);
    let influence = pairs(TX_INFLUENCE);
    let credits = tx.embedded_field(TX_CREDITS).and_then(GomValue::as_i64);
    if items.is_empty() && influence.is_empty() && credits.is_none() {
        return Value::Null;
    }
    json!({"items": items, "influence": influence, "credits": credits})
}

fn conversation_root(payload: &[u8]) -> Option<GomValue> {
    let mut reader = Reader::new(payload, PREAMBLE);
    let root = reader.read_value(EMBEDDED).ok()?;
    (reader.pos() == payload.len()).then_some(root)
}

struct Hooks {
    /// hook state id -> (quest fqn, hook name)
    by_state: HashMap<i64, Vec<(String, String)>>,
    /// (hydra guid, custom event) -> the grant actions its 'On Custom:<event>' entry runs.
    handlers: HashMap<(u64, String), Vec<(String, String)>>,
}

const GRANT_ACTIONS: &[&str] = &[
    "Grant Codex",
    "Achievement Event",
    "Grant Title",
    "Grant Suffix Title",
    "Give Items",
    "Give Quest Item",
    "Grant Mount",
    "Grant Collection Unlock",
    "Set Quest State",
    "Set Quest Granted",
    "Play Conversation",
];

/// Hydra field ids: the event-handler map, keyed by event name ('On Custom:<name>').
const HYD_HANDLERS: u32 = 0xe389_e2e3;

fn load_handlers(conn: &Connection) -> Result<HashMap<(u64, String), Vec<(String, String)>>> {
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select guid, json_extract(json,'$.payload_b64') from objects \
         where kind='Hydra' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (guid, b64) = row?;
        let Ok(id) = u64::from_str_radix(&guid, 16) else {
            continue;
        };
        let Ok(obj) = read_object_fields(&engine.decode(b64)?) else {
            continue;
        };
        for (k, v) in obj
            .embedded_field(HYD_HANDLERS)
            .and_then(GomValue::as_map)
            .into_iter()
            .flatten()
        {
            let GomValue::Str(event) = k else { continue };
            let Some(custom) = event.strip_prefix("On Custom:") else {
                continue;
            };
            let mut acts = Vec::new();
            hydra_actions(v, &mut acts);
            acts.retain(|(a, _)| GRANT_ACTIONS.contains(&a.as_str()));
            out.insert((id, custom.to_string()), acts);
        }
    }
    Ok(out)
}

fn load_hooks(conn: &Connection) -> Result<Hooks> {
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Quest' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    let mut by_state: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    for row in rows {
        let (fqn, b64) = row?;
        let Some(b64) = b64 else { continue };
        let Ok(obj) = read_object_fields(&engine.decode(b64)?) else {
            continue;
        };
        for hook in obj
            .embedded_field(QST_HOOK_LIST)
            .and_then(GomValue::as_list)
            .into_iter()
            .flatten()
        {
            let state = hook
                .embedded_field(QST_HOOK_STATE)
                .and_then(GomValue::as_i64);
            let key = hook.embedded_field(QST_HOOK_KEY).and_then(GomValue::as_i64);
            let label = match hook.embedded_field(QST_HOOK_NAME) {
                Some(GomValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            for id in [state, key].into_iter().flatten() {
                by_state
                    .entry(id)
                    .or_default()
                    .push((fqn.clone(), label.clone()));
            }
        }
        // A task's own hook id (qstHookId): the id a conversation node fires to complete it.
        let list = |v: &GomValue, id: u32| -> Vec<GomValue> {
            v.embedded_field(id)
                .and_then(GomValue::as_list)
                .map(<[GomValue]>::to_vec)
                .unwrap_or_default()
        };
        for (b, branch) in list(&obj, QST_BRANCHES).iter().enumerate() {
            let steps = list(branch, QST_STEPS);
            for (s, step) in steps.iter().enumerate() {
                for (t, task) in list(step, QST_TASKS).iter().enumerate() {
                    if let Some(id) = task
                        .embedded_field(QST_TASK_HOOK)
                        .and_then(GomValue::as_i64)
                    {
                        let last = if s + 1 == steps.len() { "/last" } else { "" };
                        by_state
                            .entry(id)
                            .or_default()
                            .push((fqn.clone(), format!("task b{b}.s{s}.t{t}{last}")));
                    }
                }
            }
        }
    }
    Ok(Hooks {
        by_state,
        handlers: load_handlers(conn)?,
    })
}

/// The hook name `id` has in `quest`, or a marker for a hook no quest names.
fn hook_name(hooks: &Hooks, quest: Option<&str>, id: i64) -> String {
    match hooks.by_state.get(&id) {
        Some(found) => match found
            .iter()
            .filter(|(q, _)| Some(q.as_str()) == quest)
            .max_by_key(|(_, n)| n.starts_with("task"))
        {
            Some((_, n)) => n.clone(),
            None => format!(
                "id {id}@not in this quest, e.g. {} {}",
                found[0].0, found[0].1
            ),
        },
        None => format!("#{id}"),
    }
}

fn describe(node: &GomValue, objects: &Objects, hooks: &Hooks) -> Value {
    let quest_fqn = |id: u64| objects.get(&id).map(|(_, f)| f.clone());
    let mut out = serde_json::Map::new();
    for (label, field) in [
        ("grants", QUEST_GRANTS),
        ("progresses", QUEST_PROGRESS),
        ("ends", QUEST_ENDS),
    ] {
        let quests: Vec<Value> = map_keys(node, field)
            .into_iter()
            .map(|q| name(objects, q))
            .collect();
        if !quests.is_empty() {
            out.insert(label.into(), json!(quests));
        }
    }
    let hook_ids: Vec<Value> = node
        .embedded_field(QUEST_HOOK_IDS)
        .and_then(GomValue::as_map)
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| {
            let quest = quest_fqn(guid_of(k)?);
            let names: Vec<String> = v
                .as_list()
                .into_iter()
                .flatten()
                .filter_map(GomValue::as_i64)
                .map(|id| hook_name(hooks, quest.as_deref(), id))
                .collect();
            Some(json!([quest, names]))
        })
        .collect();
    if !hook_ids.is_empty() {
        out.insert("hooks".into(), json!(hook_ids));
    }
    if let Some(q) = node.embedded_field(ACTION_QUEST).and_then(guid_of) {
        let hook = node
            .embedded_field(ACTION_HOOK)
            .and_then(GomValue::as_i64)
            .map(|id| hook_name(hooks, quest_fqn(q).as_deref(), id));
        let flags = node
            .embedded_field(ACTION_HOOK_FLAGS)
            .and_then(GomValue::as_i64);
        out.insert(
            "action".into(),
            json!({"quest": name(objects, q), "hook": hook, "flags": flags}),
        );
    }
    if let Some(q) = node.embedded_field(REWARD_QUEST).and_then(guid_of) {
        out.insert("reward_quest".into(), name(objects, q));
    }
    if let Some(c) = node.embedded_field(CONDITIONAL).and_then(guid_of) {
        out.insert("condition".into(), name(objects, c));
    }
    for (label, field) in [("debit", DEBIT), ("credit", CREDIT)] {
        let tx = transaction(node, field, objects);
        if !tx.is_null() {
            out.insert(label.into(), tx);
        }
    }
    if let Some(GomValue::Enum(t)) = node.embedded_field(FORCE_TYPE) {
        out.insert(
            "force".into(),
            json!({"type": t, "amount_id": node.embedded_field(FORCE_AMOUNT).and_then(GomValue::as_i64)}),
        );
    }
    if let Some(c) = node
        .embedded_field(REWARD_CREDITS)
        .and_then(GomValue::as_i64)
    {
        out.insert("reward_credits_id".into(), json!(c));
    }
    if let Some(GomValue::Str(a)) = node.embedded_field(NODE_ACTION) {
        let arg = match node.embedded_field(NODE_ACTION_ARG) {
            Some(GomValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        out.insert("node_action".into(), json!([a, arg]));
    }
    let calls: Vec<Value> = node
        .embedded_field(SCRIPT_CALLS)
        .and_then(GomValue::as_list)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let hyd = c.embedded_field(SCRIPT_CALL_HYDRA).and_then(guid_of)?;
            let event = match c.embedded_field(SCRIPT_CALL_EVENT) {
                Some(GomValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let grants = hooks.handlers.get(&(hyd, event.clone()));
            Some(json!([name(objects, hyd), event, grants]))
        })
        .collect();
    if !calls.is_empty() {
        out.insert("script_calls".into(), json!(calls));
    }
    let mut found = Vec::new();
    refs(node, 0, objects, &mut found);
    let shown: Vec<Value> = found
        .iter()
        .filter(|(_, id)| {
            matches!(
                objects.get(id).map(|(k, _)| k.as_str()),
                Some("Codex" | "Achievement" | "Item")
            )
        })
        .map(|(f, id)| json!([format!("{f:08x}"), name(objects, *id)]))
        .collect();
    if !shown.is_empty() {
        out.insert("refs".into(), json!(shown));
    }
    Value::Object(out)
}

fn render(v: &GomValue, objects: &Objects) -> Value {
    match v {
        GomValue::Null => Value::Null,
        GomValue::U64(_) | GomValue::I64(_) => match guid_of(v).and_then(|g| objects.get(&g)) {
            Some((k, f)) => json!(format!("{k}:{f}")),
            None => match v {
                GomValue::U64(n) => json!(n),
                GomValue::I64(n) => json!(n),
                _ => Value::Null,
            },
        },
        GomValue::Bool(b) => json!(b),
        GomValue::F32(f) => json!(f),
        GomValue::Enum(i) => json!({ "enum": i }),
        GomValue::Str(s) => json!(s),
        GomValue::List(l) => Value::Array(l.iter().map(|x| render(x, objects)).collect()),
        GomValue::Map(m) => Value::Array(
            m.iter()
                .map(|(k, v)| json!({ "k": render(k, objects), "v": render(v, objects) }))
                .collect(),
        ),
        GomValue::Embedded(fields) => Value::Object(
            fields
                .iter()
                .map(|(id, val)| (format!("{:08x}", *id as u32), render(val, objects)))
                .collect(),
        ),
        GomValue::ClassRef(c) => json!({ "class": format!("{c:016x}") }),
        GomValue::Vector3(v) => json!(v),
    }
}

/// Every Hydra script: its action names census, and for actions that grant (codex, achievement,
/// title, quest state, items) the action, its value and the objects it names.
fn hydra_census(conn: &Connection, objects: &Objects) -> Result<()> {
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Hydra' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut names: BTreeMap<String, u64> = BTreeMap::new();
    let mut unread = 0;
    for row in rows {
        let (fqn, b64) = row?;
        let Ok(obj) = read_object_fields(&engine.decode(b64)?) else {
            unread += 1;
            continue;
        };
        let mut acts = Vec::new();
        hydra_actions(&obj, &mut acts);
        for (a, v) in &acts {
            *names.entry(a.clone()).or_default() += 1;
            let l = a.to_lowercase();
            if l.contains("codex") || l.contains("achiev") || l.contains("title") {
                println!("GRANT {fqn} | {a} | {v}");
            }
        }
        let mut found = Vec::new();
        refs(&obj, 0, objects, &mut found);
        for (f, id) in found {
            if let Some((k, target)) = objects.get(&id) {
                if k == "Codex" || k == "Achievement" {
                    println!("REF {fqn} | {f:08x} | {k}:{target}");
                }
            }
        }
    }
    println!("unread hydra: {unread}");
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (a, n) in names {
        println!("ACTION {n} {a}");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).context("usage: <spice.sqlite> [fqn]")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let objects: Objects = {
        let mut stmt = conn.prepare("select guid, kind, fqn from objects where is_canonical=1")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut m = HashMap::new();
        for row in rows {
            let (g, k, f) = row?;
            if let Ok(id) = u64::from_str_radix(&g, 16) {
                m.insert(id, (k, f));
            }
        }
        m
    };
    if args.get(2).is_some_and(|a| a == "--obj") {
        let fqn = args.get(3).context("--obj <fqn>")?;
        let b64: String = conn.query_row(
            "select json_extract(json,'$.payload_b64') from objects where fqn=?1 and is_canonical=1",
            [fqn],
            |r| r.get(0),
        )?;
        let obj = read_object_fields(&engine.decode(b64)?)?;
        println!("{}", render(&obj, &objects));
        return Ok(());
    }
    let hooks = load_hooks(&conn)?;
    let only = args.get(2);
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1 order by fqn",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut ref_census: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut actions: BTreeMap<String, u64> = BTreeMap::new();
    let mut unread = 0u64;
    for row in rows {
        let (fqn, b64) = row?;
        if only.is_some_and(|o| *o != fqn) {
            continue;
        }
        let Some(root) = conversation_root(&engine.decode(b64)?) else {
            unread += 1;
            continue;
        };
        *counts.entry("conversations".into()).or_default() += 1;
        let roots: Vec<i64> = root
            .embedded_field(0x5028_1a20)
            .and_then(|r| r.embedded_field(CHILDREN))
            .and_then(GomValue::as_list)
            .into_iter()
            .flatten()
            .filter_map(GomValue::as_i64)
            .collect();
        for field in [NODES, LINK_NODES] {
            for (_, node) in root
                .embedded_field(field)
                .and_then(GomValue::as_map)
                .into_iter()
                .flatten()
            {
                let did = describe(node, &objects, &hooks);
                if let Value::Object(m) = &did {
                    let is_root = node
                        .embedded_field(NODE_ID)
                        .and_then(GomValue::as_i64)
                        .is_some_and(|id| roots.contains(&id));
                    for k in m.keys() {
                        *counts.entry(format!("nodes with {k}")).or_default() += 1;
                        if is_root {
                            *counts.entry(format!("root nodes with {k}")).or_default() += 1;
                        }
                    }
                    let pc = matches!(node.embedded_field(IS_PC), Some(GomValue::Bool(true)));
                    let spoken = node
                        .embedded_field(TEXT_MAP)
                        .and_then(GomValue::as_map)
                        .is_some_and(|m| !m.is_empty());
                    if m.contains_key("action") {
                        *counts
                            .entry(format!("action on pc={pc} spoken={spoken}"))
                            .or_default() += 1;
                    }
                    let kind = |h: &str| {
                        if h.contains('@') {
                            "other quest"
                        } else if h.starts_with("task") {
                            "task"
                        } else if h.starts_with('#') {
                            "unresolved"
                        } else {
                            "quest variable"
                        }
                    };
                    if let Some(h) = m.get("action").and_then(|a| a["hook"].as_str()) {
                        *counts
                            .entry(format!("action hook -> {}", kind(h)))
                            .or_default() += 1;
                        if let Some((task, _)) = h.split_once('@') {
                            *counts
                                .entry(format!("action hook not in its quest: {task}"))
                                .or_default() += 1;
                        }
                    }
                    for pair in m
                        .get("hooks")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        for h in pair[1].as_array().into_iter().flatten() {
                            let h = h.as_str().unwrap_or("");
                            *counts
                                .entry(format!("node hook -> {}", kind(h)))
                                .or_default() += 1;
                        }
                    }
                    if only.is_none() {
                        for call in m
                            .get("script_calls")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            println!(
                                "CALL {fqn} {} {} {} {}",
                                node.embedded_field(NODE_ID)
                                    .and_then(GomValue::as_i64)
                                    .unwrap_or(-1),
                                call[0].as_str().unwrap_or(""),
                                call[1].as_str().unwrap_or(""),
                                call[2]
                            );
                        }
                    }
                }
                let mut found = Vec::new();
                refs(node, 0, &objects, &mut found);
                for (f, id) in found {
                    let kind = objects.get(&id).map(|(k, _)| k.clone()).unwrap_or_default();
                    *ref_census.entry((format!("{f:08x}"), kind)).or_default() += 1;
                }
                let mut acts = Vec::new();
                hydra_actions(node, &mut acts);
                for (a, _) in &acts {
                    *actions.entry(a.clone()).or_default() += 1;
                }
                if only.is_some() && did.as_object().is_some_and(|m| !m.is_empty()) {
                    let line = node
                        .embedded_field(TEXT_MAP)
                        .and_then(GomValue::as_map)
                        .into_iter()
                        .flatten()
                        .find_map(|(_, s)| s.embedded_field(TEXT_REF)?.as_i64())
                        .map(|r| r as u32);
                    let interesting: Vec<&(String, String)> = acts
                        .iter()
                        .filter(|(a, _)| {
                            !a.contains("Camera") && !a.contains("Animation") && a != "="
                        })
                        .collect();
                    println!(
                        "{}",
                        json!({
                            "node": node.embedded_field(NODE_ID).and_then(GomValue::as_i64),
                            "line_id": line,
                            "pc": matches!(node.embedded_field(IS_PC), Some(GomValue::Bool(true))),
                            "speaker": node.embedded_field(SPEAKER).and_then(guid_of).map(|s| name(&objects, s)),
                            "children": node.embedded_field(CHILDREN).and_then(GomValue::as_list).into_iter().flatten().filter_map(GomValue::as_i64).collect::<Vec<i64>>(),
                            "link": node.embedded_field(LINK_TARGET).and_then(GomValue::as_i64),
                            "does": did,
                            "hydra": interesting,
                        })
                    );
                }
            }
        }
    }
    println!("unread conversations: {unread}");
    for (k, n) in &counts {
        println!("{k}: {n}");
    }
    if args.get(2).is_some_and(|a| a == "--hydra") {
        hydra_census(&conn, &objects)?;
        return Ok(());
    }
    if only.is_none() {
        println!("-- object refs under nodes, by field and kind");
        for ((f, k), n) in &ref_census {
            println!("{f} {k} {n}");
        }
        println!("-- hydra action names under nodes");
        let mut acts: Vec<_> = actions.into_iter().collect();
        acts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (a, n) in acts {
            println!("{n} {a}");
        }
    }
    Ok(())
}
