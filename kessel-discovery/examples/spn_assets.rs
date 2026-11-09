//! What `/resources/server/spn/*.spn_c` names, and where each spawner stands.
//!
//! Research probe for #365 and #366. Three modes:
//!
//! `census`: checks whether any `/resources/server/spn/` file ships in the
//! client archives (by the filename hash of every dictionary path), parses the
//! asset table of every `area.dat`, and cross-tabulates each spawn asset's
//! extension against the spawner type (`4000000031d37823`) of the `spn.*`
//! object it names.
//!
//! `area <dir id>`: reads every room `.dat` of one area, keeps the instances
//! whose asset is a spawner (`\spn\...` or an encounter slot `\a:enc.<fqn>.spn_c`)
//! plus the encounter instances they hang from, composes each one's world
//! position through its parent chain, and prints one JSON line per spawner
//! instance: area, room, instance, asset, spawn FQN, spawner type, entities it
//! spawns, timer preset names, local and world position.
//!
//! `questvol <spice.sqlite>`: for every quest, prints the start-volume script
//! its field `40000045816cd2a0` names (a `hyd.*` trig script), resolved by GUID.
//!
//! Room `.dat` parsing also lives in `area_graph.rs`, which reads the instance
//! table structurally and is canonical for rooms and trigger volumes. This
//! probe is canonical for spawner world positions: its parent-chain composer is
//! the one checked against combat-log NPC positions (Hutta 378 of 378).
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example spn_assets -- \
//!     census <assets_dir> <hashes_filename.txt> <spice.sqlite>
//!   cargo run --release -p kessel-discovery --example spn_assets -- \
//!     area <assets_dir> <hashes_filename.txt> <spice.sqlite> <area dir id> [--yaw-neg]
//!   cargo run --release -p kessel-discovery --example spn_assets -- \
//!     questvol <spice.sqlite>

use anyhow::{bail, Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use kessel::hash::{swtor_filename_hash, HashDictionary};
use kessel::myp::Archive;
use rusqlite::Connection;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

const INSTANCE_MAGIC: [u8; 4] = [0x34, 0x12, 0xcd, 0xab];
const KEY_PARENT: u32 = 0x40865e7f;
const KEY_POSITION: u32 = 0x4f77e269;
const KEY_ROTATION: u32 = 0x8c64af1e;

/// FNV-1a 64 of the uppercased string, as the signed i64 the GOM stores.
fn fnv_upper(s: &str) -> i64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.to_uppercase().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h as i64
}

fn timer_names() -> HashMap<i64, String> {
    let mut names = Vec::new();
    for n in 0..=1000 {
        names.push(n.to_string());
    }
    for p in ["SPN", "CRF", "CRE", "PLC", "ENC", "SEC"] {
        names.push(format!("{p}_DEFAULT"));
        for n in 0..=100 {
            names.push(format!("{p}_{n}"));
        }
    }
    names.push("INSTANT".into());
    names.push("5 SECONDS".into());
    names.into_iter().map(|n| (fnv_upper(&n), n)).collect()
}

fn field(v: &GomValue, low: u32) -> Option<&GomValue> {
    match v {
        GomValue::Embedded(fs) => fs.iter().find(|(id, _)| *id as u32 == low).map(|(_, x)| x),
        _ => None,
    }
}

fn as_str(v: Option<&GomValue>) -> Option<String> {
    match v {
        Some(GomValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

struct Spawn {
    kind: String,
    mode: String,
    entities: Vec<String>,
    timers: Vec<String>,
    delay: String,
    /// spnAutoStart (40000000293dbe27): absent on spawners a script starts.
    autostart: bool,
}

fn name_timer(v: &GomValue, names: &HashMap<i64, String>) -> String {
    match v.as_i64() {
        Some(h) => names.get(&h).cloned().unwrap_or_else(|| format!("#{h}")),
        None => String::new(),
    }
}

/// Encounter FQN -> (slot letter -> spawn FQN, encounter timer name).
type Encounters = HashMap<String, (BTreeMap<String, String>, String)>;

fn load_objects(
    db: &str,
    names: &HashMap<i64, String>,
) -> Result<(HashMap<String, Spawn>, Encounters)> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "SELECT fqn, kind, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') \
         FROM objects WHERE kind IN ('Spawn','Encounter') AND is_canonical = 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut spawns = HashMap::new();
    let mut encs = HashMap::new();
    for row in rows {
        let (fqn, kind, hh, b64) = row?;
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let Ok((v, _)) = read_framed_object(&hex::decode(hh)?, &payload) else {
            continue;
        };
        if kind == "Spawn" {
            let entities = match field(&v, 0xaa610ec0) {
                Some(GomValue::List(l)) => l
                    .iter()
                    .filter_map(|e| as_str(field(e, 0xb78d6eec)))
                    .collect(),
                _ => Vec::new(),
            };
            let timers = match field(&v, 0x15aa9dbb) {
                Some(GomValue::List(l)) => l.iter().map(|t| name_timer(t, names)).collect(),
                _ => Vec::new(),
            };
            let delay = field(&v, 0x15aa9dbc)
                .map(|t| name_timer(t, names))
                .unwrap_or_default();
            spawns.insert(
                fqn,
                Spawn {
                    kind: as_str(field(&v, 0x31d37823)).unwrap_or_default(),
                    mode: as_str(field(&v, 0x9eebcf39)).unwrap_or_default(),
                    entities,
                    timers,
                    delay,
                    autostart: matches!(field(&v, 0x293dbe27), Some(GomValue::Bool(true))),
                },
            );
        } else {
            let mut slots = BTreeMap::new();
            if let Some(GomValue::Map(m)) = field(&v, 0xd9a52ab9) {
                for (k, s) in m {
                    if let (GomValue::Str(k), GomValue::Str(s)) = (k, s) {
                        slots.insert(k.clone(), s.clone());
                    }
                }
            }
            let timer = field(&v, 0x15ac249a)
                .map(|t| name_timer(t, names))
                .unwrap_or_default();
            encs.insert(fqn, (slots, timer));
        }
    }
    Ok((spawns, encs))
}

/// Asset table of an area.dat: u64 id, u32 length counting the NUL, path.
fn asset_table(b: &[u8]) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 13 < b.len() {
        if b[i + 12] == b'\\' && b[i + 7] == 0x40 {
            let len = u32::from_le_bytes([b[i + 8], b[i + 9], b[i + 10], b[i + 11]]) as usize;
            let end = i + 12 + len;
            if (2..1024).contains(&len) && end <= b.len() && b[end - 1] == 0 {
                let s = &b[i + 12..end - 1];
                if s.iter().all(|c| (0x20..0x7f).contains(c)) {
                    let id = u64::from_le_bytes(b[i..i + 8].try_into().unwrap_or_default());
                    out.push((id, String::from_utf8_lossy(s).into_owned()));
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// What a spawn asset path names: (spawn fqn, extension, encounter fqn, slot).
fn spawn_of_asset(
    path: &str,
    encs: &HashMap<String, (BTreeMap<String, String>, String)>,
) -> Option<(String, String, Option<String>, Option<String>)> {
    let (stem, ext) = path.rsplit_once('.')?;
    if !ext.starts_with("spn_") {
        return None;
    }
    if let Some(rest) = stem.strip_prefix("\\spn\\") {
        return Some((
            format!("spn.{}", rest.replace('\\', ".")),
            ext.into(),
            None,
            None,
        ));
    }
    let rest = stem.strip_prefix('\\')?;
    let (slot, enc) = rest.split_once(":enc.")?;
    let enc = format!("enc.{enc}");
    let slot = slot.to_uppercase();
    let spn = encs
        .get(&enc)
        .and_then(|(s, _)| s.get(&slot))
        .cloned()
        .unwrap_or_default();
    Some((spn, ext.into(), Some(enc), Some(slot)))
}

struct Inst {
    room: String,
    asset: u64,
    parent: u64,
    pos: [f32; 3],
    rot: [f32; 3],
}

fn read_vec3(b: &[u8], o: usize) -> Option<[f32; 3]> {
    let f = |k: usize| -> Option<f32> {
        Some(f32::from_le_bytes(
            b.get(o + k..o + k + 4)?.try_into().ok()?,
        ))
    };
    Some([f(0)?, f(4)?, f(8)?])
}

/// Room instances: magic, 00, u64 instance, u64 asset, 00, u32 prop count,
/// u32 prop bytes, 00, then props of type u8 + key u32 + value.
fn room_instances(room: &str, b: &[u8], out: &mut HashMap<u64, Inst>) {
    let mut i = 0;
    while let Some(p) = b[i..].windows(4).position(|w| w == INSTANCE_MAGIC) {
        let s = i + p;
        i = s + 4;
        if b.get(s + 4) != Some(&0) || s + 31 > b.len() {
            continue;
        }
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap_or_default());
        let iid = u64_at(s + 5);
        let asset = u64_at(s + 13);
        let count = u32::from_le_bytes(b[s + 22..s + 26].try_into().unwrap_or_default());
        let mut o = s + 31;
        let mut inst = Inst {
            room: room.into(),
            asset,
            parent: 0,
            pos: [0.0; 3],
            rot: [0.0; 3],
        };
        let mut ok = true;
        for _ in 0..count {
            let (Some(&t), Some(kb)) = (b.get(o), b.get(o + 1..o + 5)) else {
                ok = false;
                break;
            };
            let key = u32::from_le_bytes(kb.try_into().unwrap_or_default());
            o += 5;
            let len = |o: usize| -> usize {
                b.get(o..o + 4)
                    .map(|x| u32::from_le_bytes(x.try_into().unwrap_or_default()) as usize)
                    .unwrap_or(usize::MAX / 2)
            };
            match t {
                0 => o += 1,
                1 | 3 | 4 => o += 4,
                5 => {
                    if key == KEY_PARENT && o + 8 <= b.len() {
                        inst.parent = u64_at(o);
                    }
                    o += 8
                }
                6 => {
                    if let Some(v) = read_vec3(b, o) {
                        if key == KEY_POSITION {
                            inst.pos = v;
                        } else if key == KEY_ROTATION {
                            inst.rot = v;
                        }
                    }
                    o += 12
                }
                8 | 9 => o += 4 + len(o),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            out.insert(iid, inst);
        }
    }
}

/// World position through the parent chain. Rotation is applied as yaw
/// (degrees about the height axis) only; the slots it is used for sit within
/// a few units of their encounter, so pitch and roll are ignored. On Hutta the
/// positive sign puts 344 encounter-slot NPCs at a median 0.04 log units from
/// their logged position, against 0.07 and a far worse tail for `--yaw-neg`.
/// Combat-log position (a, b, c) = 10 x world (x, z, y).
fn world(iid: u64, insts: &HashMap<u64, Inst>, yaw_sign: f32) -> Option<([f32; 3], usize)> {
    let mut pos = insts.get(&iid)?.pos;
    let mut cur = insts.get(&iid)?.parent;
    let mut depth = 0;
    while cur != 0 {
        let p = insts.get(&cur)?;
        let a = (yaw_sign * p.rot[1]).to_radians();
        let (s, c) = a.sin_cos();
        let x = pos[0] * c + pos[2] * s;
        let z = -pos[0] * s + pos[2] * c;
        pos = [p.pos[0] + x, p.pos[1] + pos[1], p.pos[2] + z];
        cur = p.parent;
        depth += 1;
        if depth > 16 {
            return None;
        }
    }
    Some((pos, depth))
}

/// Read every archive entry whose filename hash is wanted.
fn read_files(assets: &str, wanted: &HashMap<u64, String>) -> Result<HashMap<String, Vec<u8>>> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(assets)? {
        let tor = entry?.path();
        if tor.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for e in &entries {
            if let Some(path) = wanted.get(&e.filename_hash) {
                out.insert(path.clone(), archive.read_entry(e)?);
            }
        }
    }
    Ok(out)
}

fn archive_hashes(assets: &str) -> Result<HashSet<u64>> {
    let mut out = HashSet::new();
    for entry in std::fs::read_dir(assets)? {
        let tor = entry?.path();
        if tor.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let Ok(archive) = Archive::open(&tor) else {
            continue;
        };
        out.extend(archive.entries()?.map(|e| e.filename_hash));
    }
    Ok(out)
}

fn census(assets: &str, hashes: &HashDictionary, db: &str) -> Result<()> {
    let present = archive_hashes(assets)?;
    let mut by_ext: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (h, p) in hashes.paths_matching("/resources/server/") {
        let ext = p.rsplit('.').next().unwrap_or("").to_string();
        let shipped = present.contains(&swtor_filename_hash(p)) || present.contains(&h);
        let e = by_ext
            .entry(format!("{}:{ext}", p.split('/').nth(3).unwrap_or("")))
            .or_default();
        e.0 += 1;
        e.1 += usize::from(shipped);
    }
    println!("server paths in dictionary -> shipped in archives:");
    for (k, (n, s)) in &by_ext {
        println!("  {k}: {n} named, {s} shipped");
    }

    let names = timer_names();
    let (spawns, encs) = load_objects(db, &names)?;
    let wanted: HashMap<u64, String> = hashes
        .paths_matching("/area.dat")
        .into_iter()
        .map(|(h, p)| (h, p.clone()))
        .collect();
    let files = read_files(assets, &wanted)?;
    let mut auto: BTreeMap<(bool, bool), usize> = BTreeMap::new();
    for (fqn, s) in &spawns {
        *auto
            .entry((fqn.starts_with("spn.dynamic_events."), s.autostart))
            .or_default() += 1;
    }
    println!("spawns by (dynamic_events FQN, spnAutoStart true): {auto:?}");
    let mut cross: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut slots = 0;
    for b in files.values() {
        for (_, path) in asset_table(b) {
            let Some((spn, ext, enc, _)) = spawn_of_asset(&path, &encs) else {
                continue;
            };
            let form = if enc.is_some() {
                "enc-slot"
            } else {
                "spn-path"
            };
            slots += usize::from(enc.is_some());
            let kind = spawns
                .get(&spn)
                .map(|s| s.kind.clone())
                .unwrap_or_else(|| "(no spn object)".into());
            *cross.entry((form.into(), ext, kind)).or_default() += 1;
        }
    }
    println!(
        "area.dat files read: {}; encounter-slot assets: {slots}",
        files.len()
    );
    println!("asset form, extension, spawner type of the named spn object: count");
    for ((f, e, k), n) in &cross {
        println!("  {f} .{e} {k}: {n}");
    }
    Ok(())
}

/// Quests that name a hydra script in 40000045816cd2a0: what kind of script
/// it is, split by Dynamic Encounter quests and the rest.
fn questvol(db: &str) -> Result<()> {
    let conn = Connection::open(db)?;
    let mut hyd: HashMap<u64, String> = HashMap::new();
    let mut stmt = conn.prepare("SELECT guid, fqn FROM objects WHERE fqn LIKE 'hyd.%'")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (g, f) = row?;
        hyd.insert(u64::from_str_radix(&g, 16)?, f);
    }
    let mut stmt = conn.prepare(
        "SELECT fqn, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') \
         FROM objects WHERE kind = 'Quest' AND is_canonical = 1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut tally: BTreeMap<(bool, String), usize> = BTreeMap::new();
    for row in rows {
        let (fqn, hh, b64) = row?;
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let Ok((v, _)) = read_framed_object(&hex::decode(hh)?, &payload) else {
            continue;
        };
        let de = fqn.starts_with("qst.dynamic_events.");
        let what = match field(&v, 0x816cd2a0).and_then(|x| x.as_i64()) {
            None => "(none)".to_string(),
            Some(g) => match hyd.get(&(g as u64)) {
                None => "(unresolved)".to_string(),
                Some(h) => h.rsplit('.').next().unwrap_or("").to_string(),
            },
        };
        if fqn.starts_with("qst.dynamic_events.ilum.") {
            println!("{fqn} -> {what}");
        }
        *tally.entry((de, what)).or_default() += 1;
    }
    for ((de, what), n) in &tally {
        if *n > 2 || *de {
            println!("dynamic_events={de} {what}: {n}");
        }
    }
    Ok(())
}

fn area(assets: &str, hashes: &HashDictionary, db: &str, area_id: &str, yaw: f32) -> Result<()> {
    let names = timer_names();
    let (spawns, encs) = load_objects(db, &names)?;
    let prefix = format!("/resources/world/areas/{area_id}/");
    let wanted: HashMap<u64, String> = hashes
        .paths_matching(&prefix)
        .into_iter()
        .filter(|(_, p)| p.ends_with(".dat"))
        .map(|(h, p)| (h, p.clone()))
        .collect();
    let files = read_files(assets, &wanted)?;
    let area_dat = files
        .get(&format!("{prefix}area.dat"))
        .context("area.dat not found")?;
    let table: HashMap<u64, String> = asset_table(area_dat).into_iter().collect();
    let mut insts = HashMap::new();
    for (path, b) in &files {
        if path.ends_with("/area.dat") {
            continue;
        }
        let room = path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".dat");
        room_instances(room, b, &mut insts);
    }
    let mut n = 0;
    for (iid, inst) in &insts {
        let Some(path) = table.get(&inst.asset) else {
            continue;
        };
        let Some((spn, ext, enc, slot)) = spawn_of_asset(path, &encs) else {
            continue;
        };
        let s = spawns.get(&spn);
        let w = world(*iid, &insts, yaw);
        println!(
            "{}",
            json!({
                "area": area_id,
                "room": inst.room,
                "instance": iid.to_string(),
                "asset": path,
                "ext": ext,
                "encounter": enc,
                "encounter_timer": enc.as_ref().and_then(|e| encs.get(e)).map(|(_, t)| t),
                "slot": slot,
                "spawn": spn,
                "spawner_type": s.map(|s| &s.kind),
                "mode": s.map(|s| &s.mode),
                "timers": s.map(|s| &s.timers),
                "delay": s.map(|s| &s.delay),
                "autostart": s.map(|s| s.autostart),
                "entities": s.map(|s| &s.entities),
                "local": inst.pos,
                "rotation": inst.rot,
                "parent": inst.parent.to_string(),
                "world": w.map(|(p, _)| p),
                "depth": w.map(|(_, d)| d),
            })
        );
        n += 1;
    }
    eprintln!(
        "{} room files, {} instances, {n} spawner instances",
        files.len(),
        insts.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut hashes = HashDictionary::new();
    match args.get(1).map(String::as_str) {
        Some("census") if args.len() == 5 => {
            hashes.load(&args[3])?;
            census(&args[2], &hashes, &args[4])
        }
        Some("questvol") if args.len() == 3 => questvol(&args[2]),
        Some("area") if args.len() >= 6 => {
            hashes.load(&args[3])?;
            let yaw = if args.get(6).map(String::as_str) == Some("--yaw-neg") { -1.0 } else { 1.0 };
            area(&args[2], &hashes, &args[4], &args[5], yaw)
        }
        _ => bail!(
            "usage: spn_assets census <assets> <hashes> <spice> | area <assets> <hashes> <spice> <area dir id> [--yaw-neg] | questvol <spice>"
        ),
    }
}
