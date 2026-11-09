//! Decode every canonical object of one kind into JSON lines, for analysis.
//!
//! Each line is `{"fqn": ..., "v": <value tree>}`. Field ids are the low 32
//! bits in hex; any u64 that matches an `objects.guid` is replaced by
//! `@<fqn>`. Objects that fail to decode are reported on stderr and skipped.
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example dump_kind_jsonl -- \
//!     <spice.sqlite> <kind> > out.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;

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
        GomValue::Vector3(v) => json!(v),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args.get(1).context("usage: <spice.sqlite> <kind>")?;
    let kind = args.get(2).context("usage: <spice.sqlite> <kind>")?;
    let conn = Connection::open(db_path)?;

    let mut guids = HashMap::new();
    let mut stmt = conn.prepare("SELECT guid, fqn FROM objects")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (g, f) = row?;
        if let Ok(n) = u64::from_str_radix(&g, 16) {
            guids.insert(n, f);
        }
    }

    let mut stmt = conn.prepare(
        "SELECT fqn, json_extract(json, '$.payload_b64'), json_extract(json, '$.header_hex') \
         FROM objects WHERE kind = ?1 AND is_canonical = 1",
    )?;
    let rows = stmt.query_map([kind], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    let (mut ok, mut bad) = (0usize, 0usize);
    for row in rows {
        let (fqn, b64, hdr) = row?;
        let (Some(b64), Some(hdr)) = (b64, hdr) else {
            bad += 1;
            continue;
        };
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(anyhow::Error::from)
            .and_then(|p| Ok((hex::decode(hdr)?, p)))
            .and_then(|(h, p)| read_framed_object(&h, &p));
        match decoded {
            Ok((v, _)) => {
                ok += 1;
                writeln!(out, "{}", json!({ "fqn": fqn, "v": render(&v, &guids) }))?;
            }
            Err(e) => {
                bad += 1;
                eprintln!("{fqn}: {e}");
            }
        }
    }
    eprintln!("{kind}: {ok} decoded, {bad} skipped");
    Ok(())
}
