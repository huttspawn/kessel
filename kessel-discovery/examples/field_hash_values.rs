//! Every distinct large integer value of chosen fields, with counts and example holders.
//!
//! Walks the same objects and singletons as `field_sweep` (conversation graphs included) and, for
//! each field id listed in the ids file (one 16-hex id per line), prints one JSON line per distinct
//! u64 or i64 value whose magnitude is at least 2^32: `{"field", "value" (16 hex), "count",
//! "holders" (up to 3 FQNs)}`. Built for the push-2 field registry pass, to feed hash and GUID-fold
//! candidate matching over unresolved values.
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example field_hash_values -- \
//!     <spice.sqlite> <field ids file> > values.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue, Reader};
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

const EMBEDDED: u8 = 0x09;
const HOLDERS: usize = 3;

#[derive(Default)]
struct Seen {
    count: u64,
    holders: Vec<String>,
}

struct Collect {
    wanted: HashSet<u64>,
    values: BTreeMap<(u64, u64), Seen>,
}

impl Collect {
    fn note(&mut self, id: u64, fqn: &str, value: &GomValue) {
        if !self.wanted.contains(&id) {
            return;
        }
        let n = match value {
            GomValue::U64(n) => *n,
            GomValue::I64(n) => *n as u64,
            _ => return,
        };
        let magnitude = if (n as i64) < 0 {
            (n as i64).unsigned_abs()
        } else {
            n
        };
        if magnitude < 1 << 32 {
            return;
        }
        let seen = self.values.entry((id, n)).or_default();
        seen.count += 1;
        if seen.holders.len() < HOLDERS && !seen.holders.iter().any(|h| h == fqn) {
            seen.holders.push(fqn.to_string());
        }
    }

    fn walk(&mut self, fqn: &str, holder: Option<u64>, value: &GomValue) {
        match value {
            GomValue::Embedded(fields) => {
                for (id, field) in fields {
                    self.note(*id, fqn, field);
                    self.walk(fqn, Some(*id), field);
                }
            }
            GomValue::List(items) => {
                for item in items {
                    self.element(fqn, holder, item);
                }
            }
            GomValue::Map(pairs) => {
                for (key, item) in pairs {
                    self.element(fqn, holder, key);
                    self.element(fqn, holder, item);
                }
            }
            _ => {}
        }
    }

    fn element(&mut self, fqn: &str, holder: Option<u64>, value: &GomValue) {
        if let Some(id) = holder {
            if !matches!(
                value,
                GomValue::Embedded(_) | GomValue::List(_) | GomValue::Map(_)
            ) {
                self.note(id, fqn, value);
            }
        }
        self.walk(fqn, holder, value);
    }
}

fn graph_root(payload: &[u8]) -> Option<GomValue> {
    let mut reader = Reader::new(payload, 33);
    let value = reader.read_value(EMBEDDED).ok()?;
    (reader.pos() == payload.len()).then_some(value)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let db = args
        .next()
        .context("usage: <spice.sqlite> <field ids file>")?;
    let ids = args
        .next()
        .context("usage: <spice.sqlite> <field ids file>")?;
    let wanted = std::fs::read_to_string(ids)?
        .lines()
        .filter_map(|l| u64::from_str_radix(l.trim(), 16).ok())
        .collect();
    let mut collect = Collect {
        wanted,
        values: BTreeMap::new(),
    };
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select kind, fqn, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') \
         from objects union all select 'singleton', fqn, header_hex, payload_b64 from singletons",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (kind, fqn, header, b64) = row?;
        let (Some(header), Some(b64)) = (header, b64) else {
            continue;
        };
        let payload = engine.decode(b64)?;
        let value = if kind == "Conversation" {
            graph_root(&payload)
        } else {
            read_framed_object(&hex::decode(header)?, &payload)
                .ok()
                .map(|(v, _)| v)
        };
        if let Some(value) = value {
            collect.walk(&fqn, None, &value);
        }
    }
    let mut per_field: HashMap<u64, u64> = HashMap::new();
    for ((field, value), seen) in &collect.values {
        *per_field.entry(*field).or_default() += 1;
        println!(
            "{}",
            json!({
                "field": format!("{field:016x}"),
                "value": format!("{value:016x}"),
                "count": seen.count,
                "holders": seen.holders,
            })
        );
    }
    eprintln!("{} fields with large values", per_field.len());
    Ok(())
}
