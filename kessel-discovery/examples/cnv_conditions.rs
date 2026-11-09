//! Every conversation node's gate (A5/A4 decode): the condition kind (cnvConditionType
//! 999711e2), the Condition it names (cnvConditional 6584d6d0), its compiled quest expression
//! (cnvConditionCompiled bc126754, the same token stream as qstReqExpression), the invert and
//! hide flags, and the level bounds. One JSON line per node that carries any of them.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_conditions -- <spice.sqlite> > out.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const EMBEDDED: u8 = 0x09;
const PREAMBLE: usize = 33;
const NODES: u32 = 0x6683_c66d;
const LINK_NODES: u32 = 0x6683_c66e;
const NODE_ID: u32 = 0x24c9_78e5;
const CHILDREN: u32 = 0x24c9_78b9;
const ROOT: u32 = 0x5028_1a20;

/// (field, label) for every node field that gates the node.
const GATE_FIELDS: &[(u32, &str)] = &[
    (0x9997_11e2, "cnvConditionType"),
    (0x6584_d6d0, "cnvConditional"),
    (0xbc12_6754, "cnvConditionCompiled"),
    (0xbc12_6753, "cnvConditionPreviewCompiled"),
    (0x9997_11e0, "cnvInvertConditional"),
    (0x63e5_0980, "cnvHideIfConditionalFalse"),
    (0x4a55_533e, "cnvLevelConditionMin"),
    (0x4a55_533b, "cnvLevelConditionMax"),
    (0x462d_e298, "cnvRandomCondition"),
    (0x48e5_173b, "cnvBarkEventCondition"),
    (0x447e_20c0, "cnvIgnoreConditional"),
];

fn render(v: &GomValue, objects: &HashMap<u64, String>) -> Value {
    match v {
        GomValue::U64(n) => objects
            .get(n)
            .map(|f| json!(format!("@{f}")))
            .unwrap_or_else(|| json!(n)),
        GomValue::I64(n) => objects
            .get(&(*n as u64))
            .map(|f| json!(format!("@{f}")))
            .unwrap_or_else(|| json!(n)),
        GomValue::Bool(b) => json!(b),
        GomValue::Enum(i) => json!({ "enum": i }),
        GomValue::Str(s) => json!(s),
        GomValue::F32(f) => json!(f),
        GomValue::List(l) => Value::Array(l.iter().map(|x| render(x, objects)).collect()),
        other => json!(format!("{other:?}")),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let conn = Connection::open(args.get(1).context("usage: <spice.sqlite>")?)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut objects = HashMap::new();
    {
        let mut stmt = conn.prepare("select guid, fqn from objects where is_canonical=1")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (g, f) = row?;
            if let Ok(id) = u64::from_str_radix(&g, 16) {
                objects.insert(id, f);
            }
        }
    }
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1 order by fqn",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let (mut read, mut unread) = (0u64, 0u64);
    for row in rows {
        let (fqn, b64) = row?;
        let payload = engine.decode(b64)?;
        let mut reader = Reader::new(&payload, PREAMBLE);
        let Some(root) = reader
            .read_value(EMBEDDED)
            .ok()
            .filter(|_| reader.pos() == payload.len())
        else {
            unread += 1;
            continue;
        };
        read += 1;
        let roots: Vec<i64> = root
            .embedded_field(ROOT)
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
                let mut gate = Map::new();
                for (id, label) in GATE_FIELDS {
                    if let Some(v) = node.embedded_field(*id) {
                        gate.insert((*label).into(), render(v, &objects));
                    }
                }
                if gate.is_empty() {
                    continue;
                }
                let id = node.embedded_field(NODE_ID).and_then(GomValue::as_i64);
                let is_root = id.is_some_and(|i| roots.contains(&i));
                println!(
                    "{}",
                    json!({"fqn": fqn, "node": id, "root": is_root, "link": field == LINK_NODES, "gate": gate})
                );
            }
        }
    }
    eprintln!("conversations read {read}, unread {unread}");
    Ok(())
}
