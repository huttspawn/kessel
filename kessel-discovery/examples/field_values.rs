//! Print chosen top-level fields of every object whose FQN starts with a prefix.
//!
//! `dump_objects_jsonl.rs` prints whole objects over the same walk.
//!
//! One JSON line per object that carries at least one of the fields:
//! `{"fqn": ..., "guid": ..., "<field low32>": <value>}`. Values render as in
//! `dump_singleton`, without GUID-to-FQN replacement, so joins stay numeric.
//! With no field ids, every top-level field is printed.
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example field_values -- \
//!     <spice.sqlite> <fqn prefix> <field low32 hex>...

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
    let usage = "usage: <spice.sqlite> <fqn prefix> <field low32 hex>...";
    let db = args.get(1).context(usage)?;
    let prefix = args.get(2).context(usage)?;
    let wanted: Vec<u32> = args[3..]
        .iter()
        .map(|h| u32::from_str_radix(h, 16))
        .collect::<Result<_, _>>()?;

    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "SELECT fqn, guid, json_extract(json, '$.payload_b64'), json_extract(json, '$.header_hex') \
         FROM objects WHERE fqn >= ?1 AND fqn < ?1 || x'7f' AND is_canonical = 1",
    )?;
    let rows = stmt.query_map([prefix], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut failed = 0usize;
    for row in rows {
        let (fqn, guid, b64, header) = row?;
        let (Some(b64), Some(header)) = (b64, header) else {
            continue;
        };
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let Ok((GomValue::Embedded(fields), _)) =
            read_framed_object(&hex::decode(header)?, &payload)
        else {
            failed += 1;
            continue;
        };
        let mut out = serde_json::Map::new();
        for (id, val) in &fields {
            if wanted.is_empty() || wanted.contains(&(*id as u32)) {
                out.insert(format!("{:08x}", *id as u32), render(val));
            }
        }
        if !out.is_empty() {
            out.insert("fqn".into(), json!(fqn));
            out.insert("guid".into(), json!(guid));
            println!("{}", Value::Object(out));
        }
    }
    eprintln!("unreadable: {failed}");
    Ok(())
}
