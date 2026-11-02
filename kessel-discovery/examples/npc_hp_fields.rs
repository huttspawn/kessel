//! For each canonical Npc: its name string id and every scalar field, as JSON lines.
//!
//! Joined with max HP from combat logs (`npc_hp_fit.py`) to look for the fields that drive NPC
//! health.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example npc_hp_fields -- <spice.sqlite> > fields.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use rusqlite::Connection;
use serde_json::{json, Map, Value};

fn scalar(v: &GomValue) -> Option<Value> {
    match v {
        GomValue::U64(n) => Some(json!(n)),
        GomValue::I64(n) => Some(json!(n)),
        GomValue::F32(f) => Some(json!(f)),
        GomValue::Enum(i) => Some(json!(i)),
        _ => None,
    }
}

/// Every numeric value below a field, keyed by its path of field ids, so nested multipliers show.
fn flatten(prefix: &str, v: &GomValue, out: &mut Map<String, Value>) {
    match v {
        GomValue::Embedded(fields) => {
            for (id, val) in fields {
                let path = format!("{prefix}.{:08x}", *id as u32);
                if let Some(s) = scalar(val) {
                    out.insert(path, s);
                } else {
                    flatten(&path, val, out);
                }
            }
        }
        GomValue::List(items) => {
            for item in items.iter().take(4) {
                flatten(prefix, item, out);
            }
        }
        GomValue::Map(pairs) => {
            for (k, val) in pairs.iter().take(8) {
                let key = scalar(k).map_or_else(|| "?".to_string(), |k| k.to_string());
                let path = format!("{prefix}[{key}]");
                if let Some(s) = scalar(val) {
                    out.insert(path, s);
                } else {
                    flatten(&path, val, out);
                }
            }
        }
        _ => {}
    }
}

fn main() -> Result<()> {
    let db = std::env::args().nth(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "select fqn, string_id, json_extract(json,'$.payload_b64'), \
         json_extract(json,'$.header_hex') from objects where kind='Npc' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let (mut read, mut failed) = (0u64, 0u64);
    for row in rows {
        let (fqn, sid, b64, header_hex) = row?;
        let (Some(b64), Some(header_hex)) = (b64, header_hex) else {
            continue;
        };
        let header: Vec<u8> = (0..header_hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(header_hex.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect();
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let Ok((GomValue::Embedded(fields), _)) = read_framed_object(&header, &payload) else {
            failed += 1;
            continue;
        };
        read += 1;
        let mut out = Map::new();
        for (id, val) in &fields {
            let path = format!("{:08x}", *id as u32);
            if let Some(s) = scalar(val) {
                out.insert(path, s);
            } else {
                flatten(&path, val, &mut out);
            }
        }
        // The display name: the a787ee87 text slot whose bucket is str.npc holds the full log id.
        let name = fields
            .iter()
            .find(|(id, _)| *id as u32 == 0xa787_ee87)
            .and_then(|(_, v)| v.as_map())
            .into_iter()
            .flatten()
            .filter(|(_, slot)| {
                matches!(slot.embedded_field(0x5ce8_7489), Some(GomValue::Str(b)) if b == "str.npc")
            })
            .find_map(|(_, slot)| slot.embedded_field(0x5ce8_7488).and_then(GomValue::as_i64));
        println!(
            "{}",
            json!({"fqn": fqn, "sid": sid, "name": name, "f": out})
        );
    }
    eprintln!("read {read}, failed {failed}");
    Ok(())
}
