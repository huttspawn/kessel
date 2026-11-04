//! The field sweep: one line per GOM field id across every object and singleton in a database,
//! including conversation graphs. Each line gives where the field is used, its value shapes, and
//! what its references resolve to. Names and types join on afterwards, from the client.
//!
//! Run an unfiltered extract first, so developer content and every variant is present:
//!   cargo run --release -p kessel-discovery --example field_sweep -- <spice.sqlite> > fields.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue, Reader};
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

const EMBEDDED: u8 = 0x09;
const SAMPLES: usize = 6;

#[derive(Default)]
struct Field {
    uses: u64,
    kinds: BTreeMap<String, u64>,
    /// The field holding this one, where it is nested.
    parents: BTreeMap<u64, u64>,
    shapes: BTreeMap<&'static str, u64>,
    enums: BTreeMap<i64, u64>,
    ref_kinds: BTreeMap<String, u64>,
    prefixes: BTreeMap<String, u64>,
    samples: Vec<String>,
    numbers: u64,
    min: f64,
    max: f64,
}

struct Sweep<'a> {
    fields: HashMap<u64, Field>,
    kind_of: &'a HashMap<u64, String>,
}

fn prefix(text: &str) -> String {
    match text.split_once('.') {
        Some((head, _)) if head.len() <= 12 && head.chars().all(|c| c.is_ascii_alphanumeric()) => {
            format!("{head}.")
        }
        _ => "(text)".to_string(),
    }
}

impl Sweep<'_> {
    fn note(&mut self, id: u64, kind: &str, value: &GomValue) {
        let field = self.fields.entry(id).or_insert_with(|| Field {
            min: f64::MAX,
            max: f64::MIN,
            ..Field::default()
        });
        field.uses += 1;
        *field.kinds.entry(kind.to_string()).or_default() += 1;
        let (shape, number) = match value {
            GomValue::Null => ("null", None),
            GomValue::U64(n) => ("u64", Some(*n as f64)),
            GomValue::I64(n) => ("i64", Some(*n as f64)),
            GomValue::Bool(_) => ("bool", None),
            GomValue::F32(f) => ("f32", Some(f64::from(*f))),
            GomValue::Enum(_) => ("enum", None),
            GomValue::Str(_) => ("str", None),
            GomValue::List(_) => ("list", None),
            GomValue::Map(_) => ("map", None),
            GomValue::Embedded(_) => ("object", None),
            GomValue::ClassRef(_) => ("class", None),
            GomValue::Vector3(_) => ("vec3", None),
        };
        *field.shapes.entry(shape).or_default() += 1;
        if let Some(n) = number {
            field.numbers += 1;
            field.min = field.min.min(n);
            field.max = field.max.max(n);
        }
        match value {
            GomValue::Enum(i) => *field.enums.entry(*i).or_default() += 1,
            GomValue::Str(s) => *field.prefixes.entry(prefix(s)).or_default() += 1,
            GomValue::U64(n) => {
                if let Some(target) = self.kind_of.get(n) {
                    *field.ref_kinds.entry(target.clone()).or_default() += 1;
                }
            }
            _ => {}
        }
        if field.samples.len() < SAMPLES {
            let sample = match value {
                GomValue::U64(n) => n.to_string(),
                GomValue::I64(n) => n.to_string(),
                GomValue::Bool(b) => b.to_string(),
                GomValue::F32(f) => f.to_string(),
                GomValue::Enum(i) => format!("enum {i}"),
                GomValue::Str(s) => s.chars().take(60).collect(),
                _ => String::new(),
            };
            if !sample.is_empty() && !field.samples.contains(&sample) {
                field.samples.push(sample);
            }
        }
    }

    /// Every field below `value`; list and map entries count as uses of the field holding them.
    fn walk(&mut self, kind: &str, holder: Option<u64>, value: &GomValue) {
        match value {
            GomValue::Embedded(fields) => {
                for (id, field) in fields {
                    let id = *id;
                    self.note(id, kind, field);
                    if let (Some(parent), Some(entry)) = (holder, self.fields.get_mut(&id)) {
                        *entry.parents.entry(parent).or_default() += 1;
                    }
                    self.walk(kind, Some(id), field);
                }
            }
            GomValue::List(items) => {
                for item in items {
                    self.element(kind, holder, item);
                }
            }
            GomValue::Map(pairs) => {
                for (key, item) in pairs {
                    self.element(kind, holder, key);
                    self.element(kind, holder, item);
                }
            }
            _ => {}
        }
    }

    fn element(&mut self, kind: &str, holder: Option<u64>, value: &GomValue) {
        if let Some(id) = holder {
            if !matches!(
                value,
                GomValue::Embedded(_) | GomValue::List(_) | GomValue::Map(_)
            ) {
                self.note(id, kind, value);
            }
        }
        self.walk(kind, holder, value);
    }
}

/// A conversation's graph object, after its 33-byte preamble.
fn graph_root(payload: &[u8]) -> Option<GomValue> {
    let mut reader = Reader::new(payload, 33);
    let value = reader.read_value(EMBEDDED).ok()?;
    (reader.pos() == payload.len()).then_some(value)
}

fn main() -> Result<()> {
    let db = std::env::args().nth(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut kind_of = HashMap::new();
    {
        let mut stmt = conn.prepare("select guid, kind from objects")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (guid, kind) = row?;
            if let Ok(id) = u64::from_str_radix(&guid, 16) {
                kind_of.insert(id, kind);
            }
        }
    }
    let mut sweep = Sweep {
        fields: HashMap::new(),
        kind_of: &kind_of,
    };
    let (mut objects, mut failed) = (0u64, 0u64);
    let mut stmt = conn.prepare(
        "select kind, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') \
         from objects union all select 'singleton:' || fqn, header_hex, payload_b64 from singletons",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (kind, header, b64) = row?;
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
        match value {
            Some(value) => {
                objects += 1;
                sweep.walk(&kind, None, &value);
            }
            None => failed += 1,
        }
    }
    let mut ids: Vec<_> = sweep.fields.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let f = &sweep.fields[&id];
        println!(
            "{}",
            json!({
                "id": format!("{id:016x}"), "uses": f.uses, "kinds": f.kinds, "shapes": f.shapes,
                "parents": f.parents.iter().map(|(p, n)| (format!("{p:016x}"), *n)).collect::<BTreeMap<_, _>>(),
                "enums": f.enums, "ref_kinds": f.ref_kinds, "prefixes": f.prefixes,
                "samples": f.samples,
                "range": (f.numbers > 0).then_some([f.min, f.max]),
            })
        );
    }
    eprintln!(
        "objects read {objects}, failed {failed}, fields {}",
        sweep.fields.len()
    );
    Ok(())
}
