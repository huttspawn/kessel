//! See README.md. Kessel's readers are taken by path so this toy links one SQLite, rannoch's.
#![allow(dead_code)]

#[path = "../../../kessel/src/gom_reader.rs"]
mod gom_reader;
#[path = "../../../kessel/src/myp.rs"]
mod myp;
#[path = "../../../kessel/src/pbuk.rs"]
mod pbuk;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use gom_reader::GomValue;
use rannoch_queue::Queue;
use rannoch_store::{Class, Cursor, Declaration, Direction, EventId, NodeRef, Relation, Value, View, open};
use rayon::prelude::*;
use serde_json::{Value as J, json};

const KEY: [u8; 32] = [11; 32];
const RAW: &str = "dev.huttspawn.kessel.raw.gom";
const FLUSH_EVENTS: usize = 2000;
const FLUSH_BYTES: usize = 64 << 20;

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn declared(name: &str) -> String {
    format!(r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.{name}"}}"#, id())
}

fn failures(fs: Vec<rannoch_queue::Failure>) -> Result<()> {
    if let Some(f) = fs.first() {
        bail!("{} queue failures, first: {f:?}", fs.len());
    }
    Ok(())
}

fn kind_of(fqn: &str) -> String {
    match fqn.split_once('.') {
        Some((k, _)) if k.chars().all(|c| c.is_ascii_alphanumeric()) => k.to_string(),
        _ => "singleton".to_string(),
    }
}

// ---- load ----

/// A standalone prototype entry (`PROT`): conversations and other nodes live here, not in buckets.
/// Its name starts at 0x14, its guid at 8. It carries no 42-byte header, so an all-zero one with the
/// guid stands in: the stream length reads 0, and the whole payload is the bytes after the fields.
fn prot(data: &[u8]) -> Option<pbuk::GomObject> {
    let name = data.get(0x14..)?;
    let end = name.iter().take(400).position(|b| *b == 0)?;
    let fqn = std::str::from_utf8(&name[..end]).ok()?.to_string();
    let mut header = vec![0u8; 42];
    header[..8].copy_from_slice(data.get(8..16)?);
    Some(pbuk::GomObject { fqn, header, payload: name.get(end + 1..)?.to_vec() })
}

/// Every entry in the install by what its bytes start with: which pieces exist, and how big.
fn census(assets: &Path) -> Result<()> {
    let archives: Vec<PathBuf> = fs::read_dir(assets)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "tor"))
        .collect();
    let tallies: Vec<BTreeMap<String, (u64, u64, u64)>> = archives
        .par_iter()
        .map(|path| -> Result<BTreeMap<String, (u64, u64, u64)>> {
            let mut archive = myp::Archive::open(path)?;
            let entries: Vec<myp::FileEntry> = archive.entries()?.cloned().collect();
            let mut t: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
            for e in &entries {
                if e.compressed_size == 0 {
                    continue;
                }
                let Ok(d) = archive.read_entry(e) else { continue };
                let magic: String = d.iter().take(4).map(|b| if b.is_ascii_graphic() { *b as char } else { '.' }).collect();
                let hexed: String = d.iter().take(4).map(|b| format!("{b:02x}")).collect();
                let slot = t.entry(format!("{magic} {hexed}")).or_default();
                slot.0 += 1;
                slot.1 += d.len() as u64;
                slot.2 += u64::from(e.compressed_size);
            }
            Ok(t)
        })
        .collect::<Result<_>>()?;
    let mut all: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for t in tallies {
        for (k, v) in t {
            let s = all.entry(k).or_default();
            s.0 += v.0;
            s.1 += v.1;
            s.2 += v.2;
        }
    }
    let mut rows: Vec<_> = all.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.1));
    println!("entries  MB-raw  MB-stored  first bytes");
    for (k, (n, raw, stored)) in rows.iter().take(45) {
        println!("{n:8} {:7} {:9}  {k}", raw >> 20, stored >> 20);
    }
    Ok(())
}

struct Found {
    fqn: String,
    header: Vec<u8>,
    payload: Vec<u8>,
}

fn load(assets: &Path, dir: &Path, source: &str) -> Result<()> {
    let start = Instant::now();
    fs::create_dir_all(dir)?;
    if dir.join("inbox.rannoch").exists() {
        bail!("{} already loaded; loading does not resume in this toy", dir.display());
    }
    let queue = Queue::open(open(dir.join("inbox.rannoch"), &KEY)?, Queue::INTERVAL)?;
    let mut pieces = BufWriter::new(File::create(dir.join("pieces.bin"))?);
    let mut index = BufWriter::new(File::create(dir.join("index.tsv"))?);

    let mut archives: Vec<PathBuf> = fs::read_dir(assets)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "tor"))
        .collect();
    archives.sort();

    let (tx, rx) = sync_channel::<Vec<Found>>(64);
    let walker = std::thread::spawn(move || -> Result<(u64, u64)> {
        let walked: Vec<Result<(u64, u64)>> = archives
            .par_iter()
            .map(|path| {
                let mut archive = myp::Archive::open(path)?;
                let entries: Vec<myp::FileEntry> = archive.entries()?.cloned().collect();
                let (mut seen, mut bad) = (0u64, 0u64);
                for entry in &entries {
                    if entry.compressed_size == 0 {
                        continue;
                    }
                    let Ok(data) = archive.read_entry(entry) else {
                        bad += 1;
                        continue;
                    };
                    seen += 1;
                    let objects = if pbuk::is_pbuk(&data) {
                        pbuk::parse(&data)
                    } else if pbuk::is_dblb(&data) {
                        pbuk::parse_dblb_direct(&data)
                    } else if data.get(..4) == Some(b"PROT") {
                        match prot(&data) {
                            Some(o) => Ok(vec![o]),
                            None => {
                                bad += 1;
                                continue;
                            }
                        }
                    } else {
                        continue;
                    };
                    match objects {
                        Ok(objects) => {
                            let found = objects
                                .into_iter()
                                .map(|o| Found { fqn: o.fqn, header: o.header, payload: o.payload })
                                .collect();
                            tx.send(found).map_err(|e| anyhow!("{e}"))?;
                        }
                        Err(_) => bad += 1,
                    }
                }
                Ok((seen, bad))
            })
            .collect();
        let mut total = (0, 0);
        for w in walked {
            let (s, b) = w?;
            total = (total.0 + s, total.1 + b);
        }
        Ok(total)
    });

    let (mut offset, mut events, mut raw_bytes, mut body_bytes, mut held) = (0u64, 0u64, 0u64, 0u64, 0usize);
    for batch in rx {
        for o in batch {
            let mut whole = Vec::with_capacity(o.header.len() + o.payload.len());
            whole.extend_from_slice(&o.header);
            whole.extend_from_slice(&o.payload);
            let hash = blake3::hash(&whole).to_hex().to_string();
            let packed = zstd::encode_all(&whole[..], 3)?;
            pieces.write_all(&packed)?;
            let guid = o.header.get(..8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])));
            let body = json!({
                "specversion": "1.0", "id": id(), "source": source, "type": RAW, "subject": o.fqn,
                "data": {"kind": kind_of(&o.fqn), "guid": format!("{guid:016x}"), "hash": hash,
                         "at": offset, "len": packed.len(), "header": o.header.len(), "raw": whole.len()},
            })
            .to_string();
            writeln!(index, "{guid:016x}\t{}\t{}\t{}", o.payload.len(), hash, o.fqn)?;
            body_bytes += body.len() as u64;
            queue.accept_from(body, "loader", offset.to_string())?;
            offset += packed.len() as u64;
            raw_bytes += whole.len() as u64;
            events += 1;
            held += 1;
            if held >= FLUSH_EVENTS {
                failures(queue.flush().1)?;
                held = 0;
            }
        }
    }
    let (entries, bad) = walker.join().map_err(|_| anyhow!("walker panicked"))??;
    pieces.flush()?;
    index.flush()?;
    failures(queue.close())?;
    println!(
        "load {source}: {events} objects from {entries} entries ({bad} unreadable) in {:.0}s; raw {} MB, pieces {} MB, event bodies {} MB, inbox {} MB",
        start.elapsed().as_secs_f64(), raw_bytes >> 20, offset >> 20, body_bytes >> 20, size(&dir.join("inbox.rannoch")) >> 20
    );
    Ok(())
}

/// A store's main file and its write-ahead log.
fn size(path: &Path) -> u64 {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    fs::metadata(path).map_or(0, |m| m.len()) + fs::metadata(wal).map_or(0, |m| m.len())
}

// ---- rendering ----

struct Refs<'a> {
    fqns: &'a HashMap<u64, String>,
    found: BTreeSet<String>,
}

impl Refs<'_> {
    fn id(&mut self, id: u64) -> J {
        match self.fqns.get(&id) {
            Some(fqn) => {
                self.found.insert(fqn.clone());
                json!({"ref": fqn})
            }
            None => json!(id.to_string()),
        }
    }

    fn render(&mut self, v: &GomValue) -> J {
        match v {
            GomValue::Null => J::Null,
            GomValue::U64(n) | GomValue::ClassRef(n) => self.id(*n),
            GomValue::I64(n) => u64::try_from(*n).map_or_else(|_| json!(n.to_string()), |u| self.id(u)),
            GomValue::Bool(b) => json!(b),
            GomValue::F32(f) => float(*f),
            GomValue::Enum(n) => json!({"enum": n}),
            GomValue::Str(s) => json!(s),
            GomValue::List(l) => J::Array(l.iter().map(|x| self.render(x)).collect()),
            GomValue::Map(m) => {
                let mut pairs: Vec<(String, J)> = m
                    .iter()
                    .map(|(k, x)| (self.render(k).to_string(), self.render(x)))
                    .collect();
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                J::Array(pairs.into_iter().map(|(k, x)| json!([k, x])).collect())
            }
            GomValue::Embedded(fields) => {
                let o: serde_json::Map<String, J> =
                    fields.iter().map(|(i, x)| (format!("{i:016x}"), self.render(x))).collect();
                J::Object(o)
            }
            GomValue::Vector3(a) => J::Array(a.iter().map(|f| float(*f)).collect()),
        }
    }
}

fn float(f: f32) -> J {
    if f.is_finite() { json!(f) } else { json!(format!("bits:{:08x}", f.to_bits())) }
}

/// Names of objects a rendered value refers to.
fn refs_in(v: &J, out: &mut BTreeSet<String>) {
    match v {
        J::Array(a) => a.iter().for_each(|x| {
            if let Some(s) = x.as_str() {
                if s.starts_with('{') {
                    if let Ok(k) = serde_json::from_str::<J>(s) {
                        refs_in(&k, out);
                    }
                }
            }
            refs_in(x, out);
        }),
        J::Object(o) => {
            if o.len() == 1 {
                if let Some(J::String(s)) = o.get("ref") {
                    out.insert(s.clone());
                    return;
                }
            }
            o.values().for_each(|x| refs_in(x, out));
        }
        _ => {}
    }
}

enum Reading {
    Read { fields: String, tail: String, refs: BTreeSet<String> },
    Unread(String),
}

fn read(reader: &str, whole: &[u8], header_len: usize, fqns: &HashMap<u64, String>) -> Reading {
    let (header, payload) = whole.split_at(header_len);
    let decoded = match reader {
        "old" => gom_reader::read_object_fields(payload).map(|v| (v, payload.len())),
        _ => gom_reader::read_framed_object(header, payload),
    };
    match decoded {
        Err(e) => Reading::Unread(format!("{e:#}")),
        Ok((value, end)) => {
            let mut r = Refs { fqns, found: BTreeSet::new() };
            let fields = r.render(&value).to_string();
            let tail = base64::engine::general_purpose::STANDARD.encode(payload.get(end..).unwrap_or(&[]));
            Reading::Read { fields, tail, refs: r.found }
        }
    }
}

// ---- work ----

struct Index {
    fqns: HashMap<u64, String>,
    /// The object each name means: the largest payload among those carrying the name.
    best: HashMap<String, (usize, String)>,
    dups: u64,
}

fn index(dir: &Path) -> Result<Index> {
    let mut idx = Index { fqns: HashMap::new(), best: HashMap::new(), dups: 0 };
    for line in BufReader::new(File::open(dir.join("index.tsv"))?).lines() {
        let line = line?;
        let mut p = line.splitn(4, '\t');
        let (Some(guid), Some(len), Some(hash), Some(fqn)) = (p.next(), p.next(), p.next(), p.next()) else {
            bail!("bad index line {line}");
        };
        idx.fqns.insert(u64::from_str_radix(guid, 16)?, fqn.to_string());
        let len: usize = len.parse()?;
        let slot = idx.best.entry(fqn.to_string()).or_insert_with(|| (0, String::new()));
        if slot.0 != 0 {
            idx.dups += 1;
        }
        if len > slot.0 || slot.1.is_empty() {
            *slot = (len, hash.to_string());
        }
    }
    Ok(idx)
}

fn node(class: &str, key: &str, source: &str, build: i64, props: J) -> String {
    json!({"specversion": "1.0", "id": id(), "source": source, "type": format!("rannoch.node.{class}"),
           "subject": key, "data": {"properties": props, "valid": "unknown", "axis": {"build": build}}})
    .to_string()
}

fn link(kind: &str, rel: &str, from: (&str, &str), to: (&str, &str), source: &str, build: i64) -> String {
    json!({"specversion": "1.0", "id": id(), "source": source, "type": format!("rannoch.{kind}.{rel}"), "subject": from.1,
           "data": {"from": {"class": from.0, "key": from.1}, "to": {"class": to.0, "key": to.1},
                    "valid": "unknown", "axis": {"build": build}}})
    .to_string()
}

fn class(name: &str) -> Class {
    let mut c = Class::new(name, ["fields", "tail", "hash", "removed", "reason"]);
    c.axis = vec!["build".to_string()];
    c
}

fn relation_name(from: &str, to: &str) -> String {
    format!("ref_{from}_{to}")
}

fn text(p: &BTreeMap<String, Value>, k: &str) -> Option<String> {
    match p.get(k) {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

#[derive(Default)]
struct Tally {
    taken: u64,
    skipped: u64,
    duplicate: u64,
    same: u64,
    added: u64,
    changed: u64,
    unread: u64,
    removed: u64,
    links: u64,
    ended: u64,
    json_bytes: u64,
    by_kind: BTreeMap<String, (u64, u64)>,
    changed_fields: BTreeMap<String, BTreeMap<String, u64>>,
}

/// Which top-level field ids differ between two renderings.
fn differing(old: &str, new: &str) -> Vec<String> {
    let (Ok(J::Object(a)), Ok(J::Object(b))) = (serde_json::from_str::<J>(old), serde_json::from_str::<J>(new)) else {
        return vec!["(not an object)".into()];
    };
    let mut out = Vec::new();
    for (k, v) in &a {
        if b.get(k) != Some(v) {
            out.push(k.clone());
        }
    }
    for k in b.keys() {
        if !a.contains_key(k) {
            out.push(k.clone());
        }
    }
    out
}

fn work(dir: &Path, graph: &Path, build: i64, reader: &str) -> Result<()> {
    let start = Instant::now();
    let idx = index(dir)?;
    let before = size(graph);
    let source = format!("kessel/reader/{reader}");

    // Declare every class this build could need. Relations come after the nodes, when known.
    {
        let mut g = open(graph, &KEY)?;
        let mut kinds: BTreeSet<String> = idx.best.keys().map(|f| kind_of(f)).collect();
        kinds.insert("unread".into());
        for k in &kinds {
            g.declare(&Declaration::Class(class(k)), &declared(k))?;
        }
    }
    let inbox = Queue::open(open(dir.join("inbox.rannoch"), &KEY)?, Queue::INTERVAL)?;
    let out = Queue::open(open(graph, &KEY)?, Queue::INTERVAL)?;
    let reads = open(graph, &KEY)?;
    let pieces = File::open(dir.join("pieces.bin"))?;

    let suffix = std::env::var("RUN").unwrap_or_default();
    let worker = format!("shape-{build}-{reader}{suffix}");
    let producer = format!("{worker}");
    let resume: i64 = out.position(&producer)?.map_or(Ok(-1), |p| p.parse())?;

    let view = View { axis: Some(vec![build]), known: None };
    let mut tally = Tally::default();
    let (mut held, mut held_bytes) = (0usize, 0usize);
    // Closed only once the graph has committed what they caused: the two files commit apart, so a
    // close saved first would lose held writes in a crash. The graph's position is the truth.
    let mut unclosed: Vec<Cursor> = Vec::new();

    while let Some(event) = inbox.next(&worker)? {
        let cursor = event.cursor;
        let at: i64 = serde_json::to_value(cursor)?.as_i64().ok_or_else(|| anyhow!("cursor"))?;
        let body: J = serde_json::from_str(&event.body)?;
        if body["type"] != RAW {
            inbox.done(&worker, cursor)?;
            continue;
        }
        let fqn = body["subject"].as_str().ok_or_else(|| anyhow!("no subject"))?.to_string();
        let data = &body["data"];
        tally.taken += 1;
        if at <= resume {
            tally.skipped += 1;
            inbox.done(&worker, cursor)?;
            continue;
        }
        let hash = data["hash"].as_str().unwrap_or_default();
        if idx.best.get(&fqn).is_none_or(|(_, h)| h != hash) {
            tally.duplicate += 1;
            inbox.done(&worker, cursor)?;
            continue;
        }
        let (off, len, hlen) = (
            data["at"].as_u64().unwrap_or(0),
            data["len"].as_u64().unwrap_or(0) as usize,
            data["header"].as_u64().unwrap_or(0) as usize,
        );
        let mut packed = vec![0u8; len];
        pieces.read_exact_at(&mut packed, off).context("reading a piece")?;
        let whole = zstd::decode_all(&packed[..])?;

        let kind = kind_of(&fqn);
        let reference = NodeRef::new(&kind, &fqn);
        let current = reads.state_at(&reference, &view)?;
        let reading = read(reader, &whole, hlen, &idx.fqns);
        let mut writes: Vec<String> = Vec::new();
        match reading {
            Reading::Unread(reason) => {
                tally.unread += 1;
                let unread = NodeRef::new("unread", &fqn);
                let already = reads.state_at(&unread, &view)?;
                if already.is_none_or(|v| text(&v.properties, "reason").as_deref() != Some(&reason) || text(&v.properties, "hash").as_deref() != Some(hash)) {
                    writes.push(node("unread", &fqn, &source, build, json!({"reason": reason, "hash": hash})));
                }
                *tally.changed_fields.entry(kind.clone()).or_default().entry("(unread)".into()).or_insert(0) += 1;
            }
            Reading::Read { fields, tail, .. } => {
                let unread = NodeRef::new("unread", &fqn);
                if reads.state_at(&unread, &view)?.is_some_and(|v| text(&v.properties, "removed").is_none()) {
                    writes.push(node("unread", &fqn, &source, build, json!({"removed": build.to_string(), "hash": hash})));
                }
                let knowledge = blake3::hash(format!("{fields}\u{0}{tail}").as_bytes()).to_hex().to_string();
                let prior = current.as_ref().and_then(|v| {
                    if text(&v.properties, "removed").is_some() { None } else { Some(v) }
                });
                let same = prior.is_some_and(|v| text(&v.properties, "hash").as_deref() == Some(&knowledge));
                if same {
                    tally.same += 1;
                } else {
                    let slot = tally.by_kind.entry(kind.clone()).or_default();
                    if let Some(v) = prior {
                        tally.changed += 1;
                        slot.1 += 1;
                        let old = text(&v.properties, "fields").unwrap_or_default();
                        let fields_map = tally.changed_fields.entry(kind.clone()).or_default();
                        for f in differing(&old, &fields) {
                            *fields_map.entry(f).or_insert(0) += 1;
                        }
                        if text(&v.properties, "tail") != Some(tail.clone()) {
                            *fields_map.entry("(tail)".into()).or_insert(0) += 1;
                        }
                    } else {
                        tally.added += 1;
                        slot.0 += 1;
                    }
                    tally.json_bytes += (fields.len() + tail.len()) as u64;
                    writes.push(node(&kind, &fqn, &source, build, json!({"fields": fields, "tail": tail, "hash": knowledge})));
                }
            }
        }
        for w in writes {
            held += 1;
            held_bytes += w.len();
            out.accept_from(w, &producer, at.to_string())?;
        }
        unclosed.push(cursor);
        if held >= FLUSH_EVENTS || held_bytes >= FLUSH_BYTES || unclosed.len() >= FLUSH_EVENTS * 4 {
            failures(out.flush().1)?;
            (held, held_bytes) = (0, 0);
            for c in unclosed.drain(..) {
                inbox.done(&worker, c)?;
            }
        }
    }
    failures(out.flush().1)?;
    for c in unclosed.drain(..) {
        inbox.done(&worker, c)?;
    }
    let nodes_secs = start.elapsed().as_secs_f64();

    // What this build no longer holds. Graph-held names of each class, as this view reads them.
    let mut removed_writes = Vec::new();
    {
        let kinds: Vec<String> = idx.best.keys().map(|f| kind_of(f)).collect::<BTreeSet<_>>().into_iter().collect();
        for k in &kinds {
            for (key, v) in reads.nodes(k, &BTreeMap::new(), &view)? {
                if !idx.best.contains_key(&key) && text(&v.properties, "removed").is_none() {
                    removed_writes.push(node(k, &key, &source, build, json!({"removed": build.to_string()})));
                }
            }
        }
    }
    tally.removed = removed_writes.len() as u64;
    for w in removed_writes {
        out.accept(w)?;
    }
    failures(out.close())?;
    drop(reads);
    failures(inbox.close())?;

    println!(
        "work {} build {build} reader {reader}: taken {} (duplicate names {}, resumed past {}), same {}, added {}, changed {}, unread {}, removed {}; {:.0}s; wrote {} MB of JSON; graph {} MB -> {} MB",
        dir.display(), tally.taken, tally.duplicate, tally.skipped, tally.same, tally.added, tally.changed, tally.unread, tally.removed,
        nodes_secs, tally.json_bytes >> 20, before >> 20, size(graph) >> 20
    );
    for (k, (added, changed)) in &tally.by_kind {
        let top: Vec<String> = tally.changed_fields.get(k).map_or_else(Vec::new, |m| {
            let mut v: Vec<(&String, &u64)> = m.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            v.iter().take(4).map(|(f, n)| format!("{f}:{n}")).collect()
        });
        println!("  {k}: added {added}, changed {changed}; {}", top.join(" "));
    }
    Ok(())
}

// ---- links: a second worker, on the graph's own log ----

/// The millisecond in a `CloudEvents` id whose uuid is a `UUIDv7`.
fn known_ms(id: &str) -> Option<u64> {
    let hex: String = id.chars().filter(|c| *c != '-').take(12).collect();
    u64::from_str_radix(&hex, 16).ok()
}

fn open_declaring(graph: &Path, pairs: &mut BTreeSet<(String, String)>, new: Option<(String, String)>) -> Result<Queue> {
    let mut g = open(graph, &KEY)?;
    if let Some((from, to)) = new {
        let name = relation_name(&from, &to);
        let mut r = Relation::new(&name, &from, &to, false);
        r.axis = vec!["build".to_string()];
        g.declare(&Declaration::Relation(r), &declared(&name))?;
        pairs.insert((from, to));
    }
    Ok(Queue::open(g, Queue::INTERVAL)?)
}

/// Turns each node event into the links it adds and the links it ends. A link's id is derived from
/// the node event's, so doing a node twice records nothing twice. The position moves only when a
/// node's links are all held: the heartbeat can commit part of a node, and a position saying the
/// node is finished would then lose the rest after a crash.
fn links(graph: &Path) -> Result<()> {
    let start = Instant::now();
    let reads = open(graph, &KEY)?;
    let mut pairs: BTreeSet<(String, String)> = {
        let o = reads.ontology().clone();
        o.relations.values().map(|r| (r.from.clone(), r.to.clone())).collect()
    };
    let mut out = open_declaring(graph, &mut pairs, None)?;
    let (worker, producer) = ("links", "links");
    let _ = FLUSH_BYTES;
    let resume: i64 = out.position(producer)?.map_or(Ok(-1), |p| p.parse())?;
    let (mut nodes, mut added, mut ended, mut reopened, mut held) = (0u64, 0u64, 0u64, 0u64, 0usize);
    let mut last_done = resume;
    let mut unclosed: Vec<Cursor> = Vec::new();
    while let Some(event) = out.next(worker)? {
        let cursor = event.cursor;
        let at: i64 = serde_json::to_value(cursor)?.as_i64().ok_or_else(|| anyhow!("cursor"))?;
        if at <= resume || !event.body.contains(r#""type":"rannoch.node."#) {
            out.done(worker, cursor)?;
            continue;
        }
        let body: J = serde_json::from_str(&event.body)?;
        let Some(class_name) = body["type"].as_str().and_then(|t| t.strip_prefix("rannoch.node.")).map(str::to_string) else {
            out.done(worker, cursor)?;
            continue;
        };
        if class_name == "unread" {
            out.done(worker, cursor)?;
            continue;
        }
        nodes += 1;
        let key = body["subject"].as_str().unwrap_or_default().to_string();
        let build = body["data"]["axis"]["build"].as_i64().unwrap_or(0);
        let source = body["source"].as_str().unwrap_or_default().to_string();
        let event_id = body["id"].as_str().unwrap_or_default().to_string();
        let uuid = event_id.split(':').next().unwrap_or_default().to_string();
        let mut now = BTreeSet::new();
        if let Some(fields) = body["data"]["properties"]["fields"].as_str() {
            refs_in(&serde_json::from_str::<J>(fields)?, &mut now);
        }
        // What the node said just before this event, on the axis and in time.
        let before = known_ms(&uuid).map(|m| SystemTime::UNIX_EPOCH + Duration::from_millis(m.saturating_sub(1)));
        let prior = reads.state_at(&NodeRef::new(&class_name, &key), &View { axis: Some(vec![build]), known: before })?;
        let mut then = BTreeSet::new();
        if let Some(fields) = prior.as_ref().and_then(|v| text(&v.properties, "fields")) {
            refs_in(&serde_json::from_str::<J>(&fields)?, &mut then);
        }
        // A relation can only be declared with the queue closed, and closing leaves this event
        // in progress: the reopened queue hands it out again, so nothing is accepted before this.
        let added_now: Vec<String> = now.difference(&then).cloned().collect();
        let ended_now: Vec<String> = then.difference(&now).cloned().collect();
        let missing = added_now.iter().chain(&ended_now).map(|t| kind_of(t)).find(|k| !pairs.contains(&(class_name.clone(), k.clone())));
        if let Some(to_kind) = missing {
            // Everything closed so far must be committed first; the close below commits it.
            failures(out.flush().1)?;
            for c in unclosed.drain(..) {
                out.done(worker, c)?;
            }
            failures(out.close())?;
            out = open_declaring(graph, &mut pairs, Some((class_name.clone(), to_kind)))?;
            nodes -= 1;
            reopened += 1;
            continue;
        }
        let mut fact = 0u32;
        for (kind, targets) in [("link", added_now), ("ended", ended_now)] {
            for to in targets {
                let to_kind = kind_of(&to);
                fact += 1;
                let mut b: J = serde_json::from_str(&link(kind, &relation_name(&class_name, &to_kind), (&class_name, &key), (&to_kind, &to), &source, build))?;
                b["id"] = json!(format!("{uuid}:{fact}"));
                out.accept_from(b.to_string(), producer, last_done.to_string())?;
                if kind == "link" { added += 1 } else { ended += 1 }
                held += 1;
            }
        }
        // Closed only after a flush: a place saved by the heartbeat could otherwise pass a node whose
        // links are still held, and a crash would lose them. No call commits them together.
        unclosed.push(cursor);
        last_done = at;
        if held >= FLUSH_EVENTS || unclosed.len() >= FLUSH_EVENTS * 4 {
            failures(out.flush().1)?;
            held = 0;
            for c in unclosed.drain(..) {
                out.done(worker, c)?;
            }
        }
    }
    failures(out.flush().1)?;
    for c in unclosed.drain(..) {
        out.done(worker, c)?;
    }
    failures(out.close())?;
    println!("links: {nodes} node events, +{added}, ended {ended}, {} relations, {reopened} reopens, {:.0}s; graph {} MB", pairs.len(), start.elapsed().as_secs_f64(), size(graph) >> 20);
    Ok(())
}

/// One line per class with a hash of every node's key and knowledge at a build, and link totals:
/// two graphs that answer the same print the same.
fn digest(graph: &Path, build: i64) -> Result<()> {
    let g = open(graph, &KEY)?;
    let view = View { axis: Some(vec![build]), known: None };
    let classes: Vec<String> = g.ontology().classes.keys().cloned().collect();
    let mut whole = blake3::Hasher::new();
    let mut total = 0u64;
    let mut open_unread = 0u64;
    for c in classes {
        for (key, v) in g.nodes(&c, &BTreeMap::new(), &view)? {
            if c == "unread" {
                open_unread += u64::from(text(&v.properties, "removed").is_none());
                continue;
            }
            whole.update(c.as_bytes());
            whole.update(key.as_bytes());
            whole.update(text(&v.properties, "hash").unwrap_or_default().as_bytes());
            total += 1;
        }
    }
    let k = g.counts(&view)?;
    let links: u64 = k.links.values().sum();
    println!("digest build {build}: {total} nodes, {open_unread} unread, {links} links, {} relations, {}", k.links.len(), whole.finalize().to_hex());
    Ok(())
}

/// FR-KESSEL-013's walk over every relation: everything within 3 links, not following a node with
/// more than 200. No relay can name "any relation", so this walks through `describe` and `links`.
fn neighbourhoods(graph: &Path, build: i64, class_name: &str, take: usize) -> Result<()> {
    let g = open(graph, &KEY)?;
    let view = View { axis: Some(vec![build]), known: None };
    let all = g.nodes(class_name, &BTreeMap::new(), &view)?;
    let step = (all.len() / take.max(1)).max(1);
    let mut sizes = Vec::new();
    let mut times = Vec::new();
    for (key, _) in all.iter().step_by(step).take(take) {
        let t = Instant::now();
        let root = NodeRef::new(class_name, key);
        let mut seen = BTreeSet::from([(class_name.to_string(), key.clone())]);
        let mut frontier = vec![root];
        for _ in 0..3 {
            let mut next = Vec::new();
            for n in &frontier {
                let Some(d) = g.describe(n, &view)? else { continue };
                let degree: u64 = d.links.values().sum();
                if degree > 200 {
                    continue;
                }
                for (relation, direction) in d.links.keys().map(|(r, x)| (r, *x)) {
                    let rel = g.ontology().relations.get(relation).ok_or_else(|| anyhow!("relation"))?;
                    let other = if matches!(direction, Direction::Out) { rel.to.clone() } else { rel.from.clone() };
                    for (to, _) in g.links(n, relation, direction, &view)? {
                        if seen.insert((other.clone(), to.clone())) {
                            next.push(NodeRef::new(&other, &to));
                        }
                    }
                }
            }
            frontier = next;
        }
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        sizes.push(seen.len());
    }
    sizes.sort_unstable();
    times.sort_by(f64::total_cmp);
    let n = sizes.len();
    println!("{class_name}: {n} walks; objects p50 {} p90 {} max {}; ms p50 {:.1} p90 {:.1} max {:.1}", sizes[n / 2], sizes[n * 9 / 10], sizes[n - 1], times[n / 2], times[n * 9 / 10], times[n - 1]);
    Ok(())
}

// ---- check ----

fn counts(graph: &Path, build: i64) -> Result<(u64, u64, usize, usize)> {
    let g = open(graph, &KEY)?;
    let c = g.counts(&View { axis: Some(vec![build]), known: None })?;
    Ok((c.nodes.values().sum(), c.links.values().sum(), c.nodes.len(), c.links.len()))
}

fn check(graph: &Path) -> Result<()> {
    let g = open(graph, &KEY)?;
    for build in [1, 2] {
        let (n, l, classes, rels) = counts(graph, build)?;
        println!("build {build}: {n} nodes in {classes} classes, {l} links in {rels} relations");
    }
    let committed = serde_json::to_value(g.events().committed()?)?;
    println!("log events: {committed}");
    Ok(())
}

/// One object's answers: at each build, and as known at `known_ms` when given.
fn show(graph: &Path, class_name: &str, key: &str, known_ms: Option<u64>) -> Result<()> {
    let g = open(graph, &KEY)?;
    let r = NodeRef::new(class_name, key);
    let known = known_ms.map(|m| SystemTime::UNIX_EPOCH + Duration::from_millis(m));
    for build in [1, 2] {
        for (label, k) in [("now", None), ("then", known)] {
            if label == "then" && known.is_none() {
                continue;
            }
            let v = g.state_at(&r, &View { axis: Some(vec![build]), known: k })?;
            let summary = v.map(|v| {
                (v.axis.clone(), v.source.clone(), text(&v.properties, "hash").unwrap_or_default().chars().take(12).collect::<String>(), text(&v.properties, "fields").map_or(0, |f| f.len()))
            });
            println!("{key} build {build} {label}: {summary:?}");
        }
    }
    println!("hearings: {}", g.hearings(&r)?.len());
    let links = g.describe(&r, &View::default())?;
    println!("describe: {links:?}", links = links.map(|d| format!("{:?}", d.links)));
    let _ = Direction::Out;
    Ok(())
}

/// Reader against reader over one build's pieces: how often do they disagree?
fn compare(dir: &Path) -> Result<()> {
    let idx = index(dir)?;
    let g = open(dir.join("inbox.rannoch"), &KEY)?;
    let pieces = File::open(dir.join("pieces.bin"))?;
    let mut totals: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut samples = Vec::new();
    for event in g.events().since(serde_json::from_value::<Cursor>(json!(0))?)? {
        let body: J = serde_json::from_str(&event.body)?;
        if body["type"] != RAW {
            continue;
        }
        let d = &body["data"];
        let mut packed = vec![0u8; d["len"].as_u64().unwrap_or(0) as usize];
        pieces.read_exact_at(&mut packed, d["at"].as_u64().unwrap_or(0))?;
        let whole = zstd::decode_all(&packed[..])?;
        let hlen = d["header"].as_u64().unwrap_or(0) as usize;
        let fqn = body["subject"].as_str().unwrap_or_default().to_string();
        let a = read("old", &whole, hlen, &idx.fqns);
        let b = read("framed", &whole, hlen, &idx.fqns);
        *totals.entry("objects").or_default() += 1;
        let key = match (&a, &b) {
            (Reading::Unread(_), Reading::Unread(_)) => "both unread",
            (Reading::Unread(_), Reading::Read { .. }) => "old unread, framed read",
            (Reading::Read { .. }, Reading::Unread(_)) => "old read, framed unread",
            (Reading::Read { fields: x, .. }, Reading::Read { fields: y, .. }) => {
                if x == y { "agree" } else { "disagree" }
            }
        };
        *totals.entry(key).or_default() += 1;
        if key != "agree" {
            *by_kind.entry(format!("{key}: {}", kind_of(&fqn))).or_default() += 1;
            if samples.len() < 8 {
                samples.push(format!("{key}: {fqn}"));
            }
        }
    }
    println!("{totals:#?}\n{by_kind:#?}\n{samples:#?}");
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["load", assets, dir, source] => load(Path::new(assets), Path::new(dir), source),
        ["work", dir, graph, build, reader] => work(Path::new(dir), Path::new(graph), build.parse()?, reader),
        ["check", graph] => check(Path::new(graph)),
        ["show", graph, class_name, key] => show(Path::new(graph), class_name, key, None),
        ["show", graph, class_name, key, known] => show(Path::new(graph), class_name, key, Some(known.parse()?)),
        ["compare", dir] => compare(Path::new(dir)),
        ["census", assets] => census(Path::new(assets)),
        ["links", graph] => links(Path::new(graph)),
        ["digest", graph, build] => digest(Path::new(graph), build.parse()?),
        ["walk", graph, build, class_name, take] => neighbourhoods(Path::new(graph), build.parse()?, class_name, take.parse()?),
        _ => bail!("usage: load <assets> <dir> <source> | work <dir> <graph> <build> <reader> | check <graph> | show <graph> <class> <key> [known ms] | compare <dir>"),
    }
}
