//! Decode one object's stored payload from spice.sqlite (objects.json payload_b64)
//! as a flat run of top-level `<field_id><tag><value>` fields, starting at the
//! first `CF 40 00 00` field marker (the prototype / conversation node layout).
//!
//! Usage: dump_spice_object <spice.sqlite> <fqn> <out.json>

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

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
                .map(|(k, v)| json!({ "key": render(k), "value": render(v) }))
                .collect(),
        ),
        GomValue::Embedded(fields) => Value::Object(
            fields
                .iter()
                .map(|(id, v)| (format!("{id:#018x}"), render(v)))
                .collect(),
        ),
        GomValue::ClassRef(id) => json!({ "class": format!("{id:#018x}") }),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, db, fqn, out] = args.as_slice() else {
        anyhow::bail!("usage: dump_spice_object <spice.sqlite> <fqn> <out.json>");
    };
    let conn = Connection::open(db)?;
    let raw: String = conn.query_row(
        "SELECT json FROM objects WHERE fqn = ?1 ORDER BY is_canonical DESC LIMIT 1",
        [fqn],
        |r| r.get(0),
    )?;
    let obj: Value = serde_json::from_str(&raw)?;
    let b64 = obj["payload_b64"].as_str().context("no payload_b64")?;
    let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
    let marker = payload
        .windows(4)
        .position(|w| w == [0xCF, 0x40, 0x00, 0x00])
        .context("no field marker")?;
    let mut reader = Reader::new(&payload, marker);
    let mut fields = Map::new();
    let mut error = Value::Null;
    while reader.pos() < payload.len() {
        let field = match reader.read_number() {
            Ok(f) => f,
            Err(e) => {
                error = json!(format!("{e:#}"));
                break;
            }
        };
        let tag = match reader.read_tag() {
            Ok(t) => t,
            Err(e) => {
                error = json!(format!("{e:#}"));
                break;
            }
        };
        match reader.read_value(tag) {
            Ok(v) => {
                fields.insert(format!("{field:#018x}"), render(&v));
            }
            Err(e) => {
                error = json!(format!("field {field:#018x}: {e:#}"));
                break;
            }
        }
    }
    let result = json!({ "fqn": fqn, "fields": fields, "stopped": error, "bytes": payload.len() });
    std::fs::write(out, serde_json::to_string_pretty(&result)?)?;
    eprintln!("{} top-level fields; stopped: {}", result["fields"].as_object().map_or(0, |m| m.len()), result["stopped"]);
    Ok(())
}
