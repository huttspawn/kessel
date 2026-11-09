//! What a conversation node sets, advances, gives and takes (gaps A2, A13, C2): every node that
//! carries an action expression (cnvActionExpressionCompiled), a quest action, an alignment or
//! affection reward, a debit or credit transaction, or companion reactions, as one JSON line with
//! its fields rendered by low-32 id. Values that name an object render as `kind:fqn`.
//!
//! Also prints, to stderr, a census of every node field id: how many nodes carry it and the
//! value shapes seen.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_actions -- <spice.sqlite> > nodes.jsonl
//!   cargo run --release -p kessel-discovery --example cnv_actions -- <spice.sqlite> --all <fqn>
//!   cargo run --release -p kessel-discovery --example cnv_actions -- <spice.sqlite> --obj <fqn>

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_object_fields, GomValue, Reader};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const EMBEDDED: u8 = 0x09;
const PREAMBLE: usize = 33;
const NODES: u32 = 0x6683_c66d;
const LINK_NODES: u32 = 0x6683_c66e;
const NODE_ID: u32 = 0x24c9_78e5;

/// Node fields whose presence makes a node worth printing.
const WANTED: &[u32] = &[
    0xbc12_6752, // cnvActionExpressionCompiled
    0xb21f_4106, // cnvActionQuest
    0xa744_d62a, // cnvRewardForceType
    0x8c27_9053, // cnvRewardForceAmount
    0x8c27_9055, // cnvRewardAffectionRewards
    0x8c27_9058, // cnvRewardCredits
    0x8c67_53c0, // cnvRewardCreditType
    0x946e_6bf1, // cnvDebitTransaction
    0x946e_6bf2, // cnvCreditTransaction
    0x6c54_c424, // cnvNpcReactions
    0x70f2_8f35, // cnvNpcReactionMap
    0xee79_fe3f, // cnvActionList
];

/// Fields dropped from printed nodes: dialogue text, camera and animation.
const NOISE: &[u32] = &[0xa787_ee87];

type Objects = HashMap<u64, (String, String)>;

fn render(v: &GomValue, objects: &Objects) -> Value {
    match v {
        GomValue::Null => Value::Null,
        GomValue::U64(_) | GomValue::I64(_) => {
            let id = match v {
                GomValue::U64(n) => *n,
                GomValue::I64(n) => *n as u64,
                _ => 0,
            };
            match objects.get(&id) {
                Some((k, f)) => json!(format!("{k}:{f}")),
                None => match v {
                    GomValue::U64(n) => json!(n),
                    GomValue::I64(n) => json!(n),
                    _ => Value::Null,
                },
            }
        }
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
                .filter(|(id, _)| !NOISE.contains(&(*id as u32)))
                .map(|(id, val)| (format!("{:08x}", *id as u32), render(val, objects)))
                .collect(),
        ),
        GomValue::ClassRef(c) => json!({ "class": format!("{c:016x}") }),
        GomValue::Vector3(v) => json!(v),
    }
}

fn shape(v: &GomValue) -> String {
    match v {
        GomValue::Null => "null".into(),
        GomValue::U64(_) => "u64".into(),
        GomValue::I64(_) => "i64".into(),
        GomValue::Bool(_) => "bool".into(),
        GomValue::F32(_) => "f32".into(),
        GomValue::Enum(_) => "enum".into(),
        GomValue::Str(_) => "str".into(),
        GomValue::List(l) => format!("list<{}>", l.first().map(shape).unwrap_or_default()),
        GomValue::Map(m) => format!(
            "map<{},{}>",
            m.first().map(|p| shape(&p.0)).unwrap_or_default(),
            m.first().map(|p| shape(&p.1)).unwrap_or_default()
        ),
        GomValue::Embedded(_) => "obj".into(),
        GomValue::ClassRef(_) => "classref".into(),
        GomValue::Vector3(_) => "vec3".into(),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args
        .get(1)
        .context("usage: <spice.sqlite> [--all <fqn> | --obj <fqn>]")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut objects: Objects = HashMap::new();
    {
        let mut stmt = conn.prepare("select guid, kind, fqn from objects where is_canonical=1")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (g, k, f) = row?;
            if let Ok(id) = u64::from_str_radix(&g, 16) {
                objects.insert(id, (k, f));
            }
        }
    }
    let mode = args.get(2).map(String::as_str);
    if mode == Some("--obj") {
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
    let only = if mode == Some("--all") {
        Some(args.get(3).context("--all <fqn>")?.clone())
    } else {
        None
    };
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1 order by fqn",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut census: BTreeMap<u32, (u64, BTreeSet<String>)> = BTreeMap::new();
    let mut root_census: BTreeMap<u32, (u64, BTreeSet<String>)> = BTreeMap::new();
    let mut unread = 0u64;
    for row in rows {
        let (fqn, b64) = row?;
        if only.as_ref().is_some_and(|o| *o != fqn) {
            continue;
        }
        let payload = engine.decode(b64)?;
        let mut reader = Reader::new(&payload, PREAMBLE);
        let Ok(root) = reader.read_value(EMBEDDED) else {
            unread += 1;
            continue;
        };
        if let GomValue::Embedded(fields) = &root {
            for (id, v) in fields {
                let e = root_census.entry(*id as u32).or_default();
                e.0 += 1;
                if e.1.len() < 4 {
                    e.1.insert(shape(v));
                }
            }
            if only.is_some() {
                let top: serde_json::Map<String, Value> = fields
                    .iter()
                    .filter(|(id, _)| ![NODES, LINK_NODES].contains(&(*id as u32)))
                    .map(|(id, v)| (format!("{:08x}", *id as u32), render(v, &objects)))
                    .collect();
                println!("{}", json!({"fqn": fqn, "root": top}));
            }
        }
        for field in [NODES, LINK_NODES] {
            for (_, node) in root
                .embedded_field(field)
                .and_then(GomValue::as_map)
                .into_iter()
                .flatten()
            {
                let GomValue::Embedded(fields) = node else {
                    continue;
                };
                for (id, v) in fields {
                    let e = census.entry(*id as u32).or_default();
                    e.0 += 1;
                    if e.1.len() < 4 {
                        e.1.insert(shape(v));
                    }
                }
                let wanted = fields.iter().any(|(id, _)| WANTED.contains(&(*id as u32)));
                if only.is_some() || wanted {
                    println!(
                        "{}",
                        json!({
                            "fqn": fqn,
                            "link": field == LINK_NODES,
                            "node": node.embedded_field(NODE_ID).and_then(GomValue::as_i64),
                            "f": render(node, &objects),
                        })
                    );
                }
            }
        }
    }
    eprintln!("unread conversations: {unread}");
    eprintln!("-- root fields");
    for (id, (n, shapes)) in &root_census {
        eprintln!("{id:08x} {n} {shapes:?}");
    }
    eprintln!("-- node fields");
    for (id, (n, shapes)) in &census {
        eprintln!("{id:08x} {n} {shapes:?}");
    }
    Ok(())
}
