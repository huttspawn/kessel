//! Decode many objects from a spice.sqlite into JSON lines.
//!
//! `field_values.rs` does the same walk but prints only chosen fields; use it
//! when you need a few fields across a prefix, this one for whole objects.
//!
//! Walks every canonical object whose FQN starts with one of the given
//! prefixes, decodes its payload with `gom_reader`, and prints one line per
//! object: `{"fqn", "guid", "string_id", "fields"}`. Field ids are the low 32
//! bits in hex, as in `dump_singleton`. With `--need <field>` only objects
//! that carry that top-level field are printed. Built for #369 (title grants)
//! and kept general: any per-kind field census can start from it.
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example dump_objects_jsonl -- \
//!     <spice.sqlite> <fqn prefix>... [--need <field hex>]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use rusqlite::Connection;
use serde_json::{json, Value};

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
        GomValue::Embedded(fields) => {
            let mut obj = serde_json::Map::new();
            for (id, val) in fields {
                obj.insert(format!("{:08x}", *id as u32), render(val));
            }
            Value::Object(obj)
        }
        GomValue::ClassRef(c) => json!({ "class": format!("{c:016x}") }),
        GomValue::Vector3(v) => json!(v),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: <spice.sqlite> <fqn prefix>... [--need <field hex>]";
    let db_path = args.get(1).context(usage)?;
    let mut prefixes = Vec::new();
    let mut need: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--need" {
            need = Some(args.get(i + 1).context(usage)?.to_lowercase());
            i += 2;
        } else {
            prefixes.push(args[i].clone());
            i += 1;
        }
    }
    anyhow::ensure!(!prefixes.is_empty(), usage);

    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT fqn, guid, string_id, json_extract(json, '$.payload_b64'), \
         json_extract(json, '$.header_hex') FROM objects \
         WHERE is_canonical = 1 AND fqn >= ?1 AND fqn < ?1 || char(1114111)",
    )?;
    let (mut ok, mut failed) = (0usize, 0usize);
    for prefix in &prefixes {
        let rows = stmt.query_map([prefix], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (fqn, guid, string_id, b64, header_hex) = row?;
            let (Some(b64), Some(header_hex)) = (b64, header_hex) else {
                failed += 1;
                continue;
            };
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(anyhow::Error::from)
                .and_then(|p| Ok((hex::decode(header_hex)?, p)))
                .and_then(|(h, p)| read_framed_object(&h, &p));
            let fields = match decoded {
                Ok((v, _)) => render(&v),
                Err(_) => {
                    failed += 1;
                    continue;
                }
            };
            ok += 1;
            if let Some(n) = &need {
                if fields.get(n).is_none() {
                    continue;
                }
            }
            println!(
                "{}",
                json!({ "fqn": fqn, "guid": guid, "string_id": string_id, "fields": fields })
            );
        }
    }
    eprintln!("decoded {ok}, failed {failed}");
    Ok(())
}
