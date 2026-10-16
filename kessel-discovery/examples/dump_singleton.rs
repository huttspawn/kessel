//! Decode one prototype singleton from a spice.sqlite into JSON.
//!
//! Reads `singletons.payload_b64`, walks it with `gom_reader`, and renders the
//! value tree. Any u64 that matches an `objects.guid` is replaced by its FQN so
//! table rows read as references, not numbers.
//!
//! Usage:
//!   cargo run -p kessel-discovery --example dump_singleton -- \
//!     <spice.sqlite> <singleton fqn> [--first-field]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_first_field, read_object_fields, GomValue};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;

fn render(v: &GomValue, guids: &HashMap<u64, String>) -> Value {
    match v {
        GomValue::Null => Value::Null,
        GomValue::U64(n) => guids
            .get(n)
            .map(|f| json!(format!("@{f}")))
            .unwrap_or_else(|| json!(n)),
        GomValue::I64(n) => json!(n),
        GomValue::Bool(b) => json!(b),
        GomValue::F32(f) => json!(f),
        GomValue::Enum(i) => json!({ "enum": i }),
        GomValue::Str(s) => json!(s),
        GomValue::List(l) => Value::Array(l.iter().map(|x| render(x, guids)).collect()),
        GomValue::Map(m) => Value::Array(
            m.iter()
                .map(|(k, v)| json!({ "k": render(k, guids), "v": render(v, guids) }))
                .collect(),
        ),
        GomValue::Embedded(fields) => {
            let mut obj = serde_json::Map::new();
            for (id, val) in fields {
                obj.insert(format!("{:08x}", *id as u32), render(val, guids));
            }
            Value::Object(obj)
        }
        GomValue::ClassRef(c) => json!({ "class": format!("{c:016x}") }),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .get(1)
        .context("usage: <spice.sqlite> <singleton fqn>")?;
    let fqn = args
        .get(2)
        .context("usage: <spice.sqlite> <singleton fqn>")?;
    let first_field = args.iter().any(|a| a == "--first-field");

    let conn = Connection::open(db_path)?;
    let b64: String = conn
        .query_row(
            "SELECT payload_b64 FROM singletons WHERE fqn = ?1",
            [fqn],
            |r| r.get(0),
        )
        .with_context(|| format!("singleton {fqn} not found"))?;
    let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;

    let mut guids = HashMap::new();
    let mut stmt = conn.prepare("SELECT guid, fqn FROM objects")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (g, f) = row?;
        if let Ok(n) = u64::from_str_radix(&g, 16) {
            guids.insert(n, f);
        }
    }

    let value = if first_field {
        read_first_field(&payload)?
    } else {
        read_object_fields(&payload)?
    };
    println!("{}", serde_json::to_string_pretty(&render(&value, &guids))?);
    Ok(())
}
