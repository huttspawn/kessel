//! The field relation sweep: every link one game object makes to another through a field, with the
//! target resolved to the class of the object it names in the same extract.
//!
//! Walks every object and singleton (conversation graphs too) with kessel's own reader, the way
//! `field_sweep` does, and treats a value as a reference candidate when it is:
//!   - a u64 or i64 whose top 16 bits are 0xE000 (the GOM GUID space), resolved by GUID. A GUID is
//!     the fold of its FQN (FNV-1a 64 over the uppercased name, to 48 bits; RESEARCH 01a1c0ce-447e),
//!     so an unresolved one is checked against the folds of every string the data holds;
//!   - a class reference (tag 0x0E style `ClassRef`), resolved by GUID;
//!   - a string shaped like an FQN whose first segment is a prefix some loaded FQN uses,
//!     resolved by FQN.
//!
//! Each link is tallied under (source class, holding field id, target class, value form, position),
//! where position says whether the value sat in the field itself, a list element, a map key or a
//! map value. Unresolved candidates are tallied per (source class, field, form).
//!
//!   cargo run --release -p kessel-discovery --example relations_fields -- <unfiltered.sqlite> <out.json>

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue, Reader};
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

const EMBEDDED: u8 = 0x09;

/// One loaded object: its class and FQN.
struct Obj {
    class: String,
    fqn: String,
}

#[derive(Default)]
struct Rel {
    links: u64,
    self_links: u64,
    forms: BTreeMap<&'static str, u64>,
    positions: BTreeMap<&'static str, u64>,
    sources: HashSet<u32>,
    targets: HashMap<u32, HashSet<u32>>,
    per_source: HashMap<u32, u32>,
    paths: HashMap<String, u64>,
    sample: Option<(u32, u32)>,
}

#[derive(Default)]
struct Cand {
    total: u64,
    resolved: u64,
    sources: HashSet<u32>,
    unresolved_samples: Vec<String>,
    unresolved_prefixes: HashMap<String, u64>,
    unresolved_guids: HashMap<u64, u64>,
}

struct Sweep<'a> {
    objs: &'a [Obj],
    by_guid: &'a HashMap<u64, u32>,
    by_hash: &'a HashMap<u64, u32>,
    by_fqn: &'a HashMap<String, u32>,
    prefixes: &'a HashSet<String>,
    rels: HashMap<(String, u64, String), Rel>,
    cands: HashMap<(String, u64, &'static str), Cand>,
    path: Vec<u64>,
    /// Every string value seen: an unresolved GUID may be the fold of a name nothing loads.
    strings: HashSet<String>,
}

fn class_of(kind: &str) -> String {
    if kind.ends_with("_scs") {
        "scs".to_string()
    } else {
        kind.to_string()
    }
}

/// The achievement event-code fold of an FQN.
fn event_code(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.to_uppercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    0xE000_0000_0000_0000 | (h & 0x0000_FFFF_FFFF_0000) | ((h ^ (h >> 48)) & 0xFFFF)
}

fn fqn_shaped(s: &str) -> Option<&str> {
    let (head, rest) = s.split_once('.')?;
    if head.is_empty()
        || rest.is_empty()
        || !head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | '\''))
    {
        return None;
    }
    Some(head)
}

impl Sweep<'_> {
    #[allow(clippy::too_many_arguments)]
    fn link(
        &mut self,
        src: u32,
        field: u64,
        pos: &'static str,
        form: &'static str,
        target: Option<u32>,
        shown: String,
        prefix: Option<String>,
    ) {
        let class = self.objs[src as usize].class.clone();
        let cand = self.cands.entry((class.clone(), field, form)).or_default();
        cand.total += 1;
        cand.sources.insert(src);
        let Some(t) = target else {
            if cand.unresolved_samples.len() < 6 && !cand.unresolved_samples.contains(&shown) {
                cand.unresolved_samples.push(shown);
            }
            if let Some(p) = prefix {
                *cand.unresolved_prefixes.entry(p).or_default() += 1;
            }
            return;
        };
        cand.resolved += 1;
        let tclass = self.objs[t as usize].class.clone();
        let ancestors = match self.path.last() {
            Some(last) if *last == field => &self.path[..self.path.len() - 1],
            _ => &self.path[..],
        };
        let path = ancestors
            .iter()
            .map(|p| format!("{p:016x}"))
            .collect::<Vec<_>>()
            .join("/");
        let rel = self.rels.entry((class, field, tclass)).or_default();
        *rel.forms.entry(form).or_default() += 1;
        *rel.positions.entry(pos).or_default() += 1;
        rel.links += 1;
        if src == t {
            rel.self_links += 1;
        }
        rel.sources.insert(src);
        rel.targets.entry(t).or_default().insert(src);
        *rel.per_source.entry(src).or_default() += 1;
        *rel.paths.entry(path).or_default() += 1;
        rel.sample.get_or_insert((src, t));
    }

    fn scalar(&mut self, src: u32, field: u64, pos: &'static str, value: &GomValue) {
        let n = match value {
            GomValue::U64(n) => Some(*n),
            GomValue::I64(n) => Some(*n as u64),
            _ => None,
        };
        if let Some(n) = n {
            if n >> 48 == 0xE000 {
                if let Some(t) = self.by_guid.get(&n) {
                    self.link(src, field, pos, "guid", Some(*t), String::new(), None);
                } else if let Some(t) = self.by_hash.get(&n) {
                    self.link(src, field, pos, "fqn-hash", Some(*t), String::new(), None);
                } else {
                    self.link(src, field, pos, "guid", None, format!("{n:016X}"), None);
                    let class = self.objs[src as usize].class.clone();
                    if let Some(c) = self.cands.get_mut(&(class, field, "guid")) {
                        *c.unresolved_guids.entry(n).or_default() += 1;
                    }
                }
            }
            return;
        }
        match value {
            GomValue::ClassRef(n) => {
                let t = self.by_guid.get(n).copied();
                self.link(src, field, pos, "class-ref", t, format!("{n:016X}"), None);
            }
            GomValue::Str(s) => {
                if s.len() < 200 && !self.strings.contains(s) {
                    self.strings.insert(s.clone());
                }
                let Some(head) = fqn_shaped(s) else { return };
                if !self.prefixes.contains(head) {
                    return;
                }
                if let Some(t) = self.by_fqn.get(s.as_str()).copied() {
                    self.link(src, field, pos, "fqn", Some(t), String::new(), None);
                    return;
                }
                // A member of a loaded object: its FQN plus trailing segments (a quest variable,
                // a conversation's node, and so on). Strip segments until a loaded FQN matches.
                let mut cut = s.as_str();
                while let Some((head_part, _)) = cut.rsplit_once('.') {
                    cut = head_part;
                    if let Some(t) = self.by_fqn.get(cut).copied() {
                        self.link(src, field, pos, "fqn-member", Some(t), String::new(), None);
                        return;
                    }
                }
                self.link(
                    src,
                    field,
                    pos,
                    "fqn",
                    None,
                    s.chars().take(80).collect(),
                    Some(head.to_string()),
                );
            }
            _ => {}
        }
    }

    fn walk(&mut self, src: u32, holder: Option<u64>, pos: &'static str, value: &GomValue) {
        match value {
            GomValue::Embedded(fields) => {
                for (id, field) in fields {
                    self.scalar(src, *id, "field", field);
                    self.path.push(*id);
                    self.walk(src, Some(*id), "field", field);
                    self.path.pop();
                }
            }
            GomValue::List(items) => {
                for item in items {
                    self.element(src, holder, "list", item);
                }
            }
            GomValue::Map(pairs) => {
                for (k, v) in pairs {
                    self.element(src, holder, "map-key", k);
                    self.element(src, holder, "map-value", v);
                }
            }
            _ => {
                let _ = pos;
            }
        }
    }

    fn element(&mut self, src: u32, holder: Option<u64>, pos: &'static str, value: &GomValue) {
        if let Some(id) = holder {
            self.scalar(src, id, pos, value);
        }
        self.walk(src, holder, pos, value);
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
        .context("usage: <unfiltered.sqlite> <out.json>")?;
    let out = args
        .next()
        .context("usage: <unfiltered.sqlite> <out.json>")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;

    let mut objs: Vec<Obj> = Vec::new();
    let mut payloads: Vec<(String, String)> = Vec::new();
    let mut by_guid = HashMap::new();
    let mut by_fqn: HashMap<String, u32> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "select guid, kind, fqn, is_canonical, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') from objects \
             union all select '', 'singleton:' || fqn, fqn, 1, header_hex, payload_b64 from singletons",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            ))
        })?;
        for row in rows {
            let (guid, kind, fqn, canonical, header, b64) = row?;
            let idx = u32::try_from(objs.len())?;
            let guid = if guid.is_empty() {
                let h = hex::decode(&header).unwrap_or_default();
                h.get(..8)
                    .and_then(|b| b.try_into().ok())
                    .map_or(0, u64::from_le_bytes)
            } else {
                u64::from_str_radix(&guid, 16)?
            };
            by_guid.insert(guid, idx);
            if canonical == 1 || !by_fqn.contains_key(&fqn) {
                by_fqn.insert(fqn.clone(), idx);
            }
            objs.push(Obj {
                class: class_of(&kind),
                fqn,
            });
            payloads.push((header, b64));
        }
    }
    let by_hash: HashMap<u64, u32> = objs
        .iter()
        .enumerate()
        .map(|(i, o)| (event_code(&o.fqn), i as u32))
        .collect();
    let prefixes: HashSet<String> = objs
        .iter()
        .filter_map(|o| o.fqn.split_once('.').map(|(h, _)| h.to_string()))
        .collect();
    eprintln!("loaded {} objects, {} prefixes", objs.len(), prefixes.len());

    let mut sweep = Sweep {
        objs: &objs,
        by_guid: &by_guid,
        by_hash: &by_hash,
        by_fqn: &by_fqn,
        prefixes: &prefixes,
        rels: HashMap::new(),
        cands: HashMap::new(),
        path: Vec::new(),
        strings: HashSet::new(),
    };
    let (mut read, mut failed) = (0u64, 0u64);
    let mut failed_by: BTreeMap<String, u64> = BTreeMap::new();
    for (i, (header, b64)) in payloads.iter().enumerate() {
        let src = i as u32;
        let Ok(payload) = engine.decode(b64) else {
            failed += 1;
            continue;
        };
        let value = if objs[i].class == "Conversation" {
            graph_root(&payload)
        } else {
            hex::decode(header)
                .ok()
                .and_then(|h| read_framed_object(&h, &payload).ok().map(|(v, _)| v))
        };
        match value {
            Some(v) => {
                read += 1;
                sweep.walk(src, None, "field", &v);
            }
            None => {
                failed += 1;
                *failed_by.entry(objs[i].class.clone()).or_default() += 1;
            }
        }
    }
    eprintln!("read {read}, failed {failed}: {failed_by:?}");

    let mut rels = Vec::new();
    for ((sc, field, tc), r) in &sweep.rels {
        let max_per_source = r.per_source.values().copied().max().unwrap_or(0);
        let max_sources_per_target = r.targets.values().map(HashSet::len).max().unwrap_or(0);
        let mut paths: Vec<_> = r.paths.iter().collect();
        paths.sort_by(|a, b| b.1.cmp(a.1));
        let (s, t) = r.sample.unwrap_or((0, 0));
        rels.push(json!({
            "source_class": sc, "field_id": format!("{field:016x}"), "target_class": tc,
            "forms": r.forms, "positions": r.positions, "links": r.links, "self_links": r.self_links, "sources": r.sources.len(),
            "targets": r.targets.len(), "max_per_source": max_per_source,
            "max_sources_per_target": max_sources_per_target,
            "pairs": r.targets.values().map(HashSet::len).sum::<usize>(),
            "paths": paths.iter().take(3).map(|(p, n)| json!([p, n])).collect::<Vec<_>>(),
            "path_count": r.paths.len(),
            "sample": [objs[s as usize].fqn, objs[t as usize].fqn],
        }));
    }
    let named: HashMap<u64, &String> = sweep.strings.iter().map(|s| (event_code(s), s)).collect();
    eprintln!("strings seen {}", named.len());
    let mut cands = Vec::new();
    for ((sc, field, form), c) in &sweep.cands {
        let (mut by_name, mut name_prefixes, mut name_samples) =
            (0u64, BTreeMap::<String, u64>::new(), Vec::new());
        for (g, n) in &c.unresolved_guids {
            if let Some(name) = named.get(g) {
                by_name += n;
                let head = name
                    .split_once('.')
                    .map_or("(text)", |(h, _)| h)
                    .to_string();
                *name_prefixes.entry(head).or_default() += n;
                if name_samples.len() < 4 {
                    name_samples.push((*name).clone());
                }
            }
        }
        let mut pre: Vec<_> = c.unresolved_prefixes.iter().collect();
        pre.sort_by(|a, b| b.1.cmp(a.1));
        cands.push(json!({
            "source_class": sc, "field_id": format!("{field:016x}"), "form": form,
            "total": c.total, "resolved": c.resolved, "sources": c.sources.len(),
            "unresolved_samples": c.unresolved_samples,
            "unresolved_prefixes": pre.iter().take(5).map(|(p, n)| json!([p, n])).collect::<Vec<_>>(),
            "unresolved_named_by_string": by_name, "name_prefixes": name_prefixes, "name_samples": name_samples,
        }));
    }
    let doc = json!({"objects": objs.len(), "read": read, "failed": failed, "failed_by": failed_by,
                     "relations": rels, "candidates": cands});
    std::fs::write(&out, serde_json::to_vec(&doc)?)?;
    eprintln!("relations {}, candidate groups {}", rels.len(), cands.len());
    Ok(())
}
