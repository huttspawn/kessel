//! Conversations: from which offset does one GOM object read to the end of the payload?
//!
//! For every conversation, tries each offset in the first bytes, keeps the first that reads an
//! object to exactly the end, and counts the offsets and the bytes before them. With an fqn,
//! dumps that conversation's object as JSON instead.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_graph_census -- <spice.sqlite> [fqn]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;

const EMBEDDED: u8 = 0x09;

fn root(payload: &[u8]) -> Option<(usize, GomValue)> {
    (0..payload.len().min(96)).find_map(|start| {
        let mut reader = Reader::new(payload, start);
        let value = reader.read_value(EMBEDDED).ok()?;
        (reader.pos() == payload.len()).then_some((start, value))
    })
}

fn render(v: &GomValue) -> Value {
    match v {
        GomValue::Null => Value::Null,
        GomValue::U64(n) => json!(n),
        GomValue::I64(n) => json!(n),
        GomValue::Bool(b) => json!(b),
        GomValue::F32(f) => json!(f),
        GomValue::Enum(i) => json!({ "enum": i }),
        GomValue::Str(s) => json!(s),
        GomValue::List(l) => Value::Array(l.iter().map(render).collect()),
        GomValue::Map(m) => Value::Array(
            m.iter()
                .map(|(k, v)| json!({ "k": render(k), "v": render(v) }))
                .collect(),
        ),
        GomValue::Embedded(fields) => Value::Object(
            fields
                .iter()
                .map(|(id, val)| (format!("{:08x}", *id as u32), render(val)))
                .collect(),
        ),
        GomValue::ClassRef(c) => json!({ "class": format!("{c:016x}") }),
        GomValue::Vector3(v) => json!(v),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).context("usage: <spice.sqlite> [fqn]")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    if let Some(fqn) = args.get(2) {
        let b64: String = conn.query_row(
            "select json_extract(json,'$.payload_b64') from objects where fqn=?1 and is_canonical=1",
            [fqn],
            |r| r.get(0),
        )?;
        let payload = engine.decode(b64)?;
        let (start, value) = root(&payload).context("no offset reads to the end")?;
        println!(
            "{}",
            json!({"start": start, "head": hex::encode(&payload[..start]), "root": render(&value)})
        );
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut values: HashMap<(u32, String), u64> = HashMap::new();
    let (mut no_nodes, mut choice_nodes) = (Vec::new(), 0u64);
    let (mut starts, mut misses): (HashMap<usize, u64>, Vec<String>) = Default::default();
    for row in rows {
        let (fqn, b64) = row?;
        match root(&engine.decode(b64)?) {
            Some((start, root)) => {
                *starts.entry(start).or_default() += 1;
                let nodes = root.embedded_field(0x6683_c66d).and_then(GomValue::as_map);
                if nodes.is_none() {
                    no_nodes.push(fqn.clone());
                }
                let nodes: Vec<&GomValue> = nodes.into_iter().flatten().map(|(_, n)| n).collect();
                let by_id: HashMap<i64, &GomValue> = nodes
                    .iter()
                    .filter_map(|n| Some((n.embedded_field(0x24c9_78e5)?.as_i64()?, *n)))
                    .collect();
                for node in &nodes {
                    let kids: Vec<&GomValue> = node
                        .embedded_field(0x24c9_78b9)
                        .and_then(GomValue::as_list)
                        .into_iter()
                        .flatten()
                        .filter_map(|id| by_id.get(&id.as_i64()?).copied())
                        .filter(|k| k.embedded_field(0x259e_02f8).is_some())
                        .collect();
                    if kids.len() < 2 {
                        continue;
                    }
                    for kid in kids {
                        choice_nodes += 1;
                        if let GomValue::Embedded(fields) = kid {
                            for (id, value) in fields {
                                let text = render(value).to_string();
                                let text = if text.len() > 60 {
                                    "<long>".to_string()
                                } else {
                                    text
                                };
                                *values.entry((*id as u32, text)).or_default() += 1;
                            }
                        }
                    }
                }
            }
            None => misses.push(fqn),
        }
    }
    let mut starts: Vec<_> = starts.into_iter().collect();
    starts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("read to end at offset (offset, count): {starts:?}");
    println!(
        "no node map: {} {:?}",
        no_nodes.len(),
        &no_nodes[..no_nodes.len().min(4)]
    );
    println!("choice nodes (with a sibling choice): {choice_nodes}");
    let mut by_field: HashMap<u32, Vec<(String, u64)>> = HashMap::new();
    for ((id, text), n) in values {
        by_field.entry(id).or_default().push((text, n));
    }
    let mut fields: Vec<_> = by_field.into_iter().collect();
    fields.sort_by_key(|(id, _)| *id);
    for (id, mut seen) in fields {
        seen.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let total: u64 = seen.iter().map(|(_, n)| n).sum();
        println!(
            "{id:08x} on {total}, {} values: {:?}",
            seen.len(),
            &seen[..seen.len().min(6)]
        );
    }
    println!(
        "no offset: {} {:?}",
        misses.len(),
        &misses[..misses.len().min(12)]
    );
    Ok(())
}
