//! area_graph -- room placements and trigger volumes as a graph of nodes and links.
//!
//! Reads every area under /resources/world/areas/ straight from the archives:
//! area.dat (room list and asset table) and each room .dat (instances and
//! their properties). Each gameplay instance (spawns, encounters, triggers,
//! map notes) becomes a node with its world position, in the area frame and
//! in the frame the combat log prints. Links join it to its area, its template,
//! and, through spice, to what the template spawns, plays and grants.
//!
//! Output: JSON lines, each either {"node": kind, ...} or {"link": kind, "from", "to", ...}.
//!
//! Usage:
//!   area_graph <assets dir> <hashes_filename.txt> <spice.sqlite> <out.jsonl> [--all] [area id ...]
//!
//! <spice.sqlite> should be an --unfiltered extract so abl.placeables.* ids resolve.
//! --all emits every instance (art, lights, cover points too), not only gameplay ones.
//!
//! Layouts (bytes are the record; decoded 2026-11-09 on live 7.x):
//!   area.dat: u32-len "AREA_DAT_BINARY_FORMAT_" signature, u32 version, then at 0x20
//!     eight u32 section offsets; at 0x56 u32 room count and that many u32-len names;
//!     then u32 asset count and (u64 id, u32 len counting the NUL, path) each. The
//!     asset table ends exactly at the second section offset in all 267 areas.
//!   room .dat: u32-len "ROOM_DAT_BINARY_FORMAT_" signature, u32 offset of the
//!     instance table; there u32 count, then per instance: u32 0xABCD1234, u8,
//!     u64 instance id, u64 asset id, u8, i32 property count, u32 property bytes, u8,
//!     then properties: u8 type, u32 key hash, value (0 u8, 1-4 u32/f32, 5 u64,
//!     6 vec3, 7 vec4, 8 u32 byte length + UTF-16, 9 u32 length + blob).

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use kessel::hash::{self, HashDictionary};
use kessel::myp::{Archive, FileEntry};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;

// Room instance property keys.
const POS: u32 = 0x4f77e269; // vec3, local position (x, height, z)
const ROT: u32 = 0x8c64af1e; // vec3, Euler degrees (rx, ry=yaw, rz)
const SCALE: u32 = 0xb181622a; // vec3
const PARENT: u32 = 0x40865e7f; // u64, parent instance id (0 = none)
const SCRIPT: u32 = 0xd84fb395; // string, hydra script FQN on a trigger
const MAP_PAGE: u32 = 0x39801eba; // string, map page tag a trigger switches to
const VOL_SHAPE: u32 = 0x80af3c1a; // u32, trigger shape
const VOL_Y: u32 = 0x16b4f247; // f32, trigger size along height
const VOL_X: u32 = 0xd9ddf326; // f32, trigger size along local x
const VOL_Z: u32 = 0x1b205d23; // f32, trigger size along local z

// GOM field ids (low 32 bits).
const SPN_ENTRIES: u32 = 0xaa610ec0; // Spawn: list of what it spawns
const SPN_ENTRY_TEMPLATE: u32 = 0xb78d6eec; // entry: template FQN (npc./plc.)
const PLC_ON_USE: u32 = 0x1d6b4924; // plcAbilitySpecOnUse
const PLC_CODEX: u32 = 0x2d7b8797; // plcCodexSpec
const PLC_CONVO: u32 = 0xf0827b5d; // plcConvo
const HYD_ACTION: u32 = 0xe5346fa0; // hydAction
const HYD_VALUE: u32 = 0xe5346faf; // hydValue
const MAP_PAGES: u32 = 0xb5f9c772; // mapdata: list of map pages
const PAGE_NAME: u32 = 0x6225006b;
const PAGE_NAME_REF: u32 = 0x858fc65e; // (string table << 32) | entry; the log's area id
const PAGE_TAGS: u32 = 0xb970d7b7;
const PAGE_MIN: u32 = 0x6225006f; // world volume min (x, height, z)
const PAGE_MAX: u32 = 0x62250070;
const PAGE_IMG_MIN: u32 = 0x0ccc4257; // rectangle the page image covers (x, _, z)
const PAGE_IMG_MAX: u32 = 0x0ccc4256;

const GAMEPLAY: &[&str] = &[
    "spn_p", "spn_c", "spn_crf", "spn_lst", "spn_grp", "spn_veh", "enc", "trg", "mpn",
];

#[derive(Clone)]
enum Prop {
    Num(u64),
    F(f32),
    V3([f32; 3]),
    V4,
    S(String),
    Blob,
}

struct Inst {
    id: u64,
    room: String,
    asset: u64,
    props: HashMap<u32, Prop>,
}

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n).context("truncated")?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn lstr(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        Ok(String::from_utf8_lossy(b)
            .trim_end_matches('\0')
            .to_string())
    }
}

struct AreaDat {
    rooms: Vec<String>,
    assets: HashMap<u64, String>,
}

fn read_area(d: &[u8]) -> Result<AreaDat> {
    let mut r = Rd { d, p: 0 };
    let sig = r.lstr()?;
    anyhow::ensure!(sig.starts_with("AREA_DAT_BINARY_FORMAT"), "not an area.dat");
    r.p = 0x56;
    let n = r.u32()?;
    let rooms = (0..n).map(|_| r.lstr()).collect::<Result<Vec<_>>>()?;
    let n = r.u32()?;
    let mut assets = HashMap::new();
    for _ in 0..n {
        let id = r.u64()?;
        assets.insert(id, r.lstr()?);
    }
    Ok(AreaDat { rooms, assets })
}

fn read_room(d: &[u8], room: &str, out: &mut Vec<Inst>) -> Result<()> {
    let mut r = Rd { d, p: 0 };
    let sig = r.lstr()?;
    anyhow::ensure!(sig.starts_with("ROOM_DAT_BINARY_FORMAT"), "not a room .dat");
    r.p = r.u32()? as usize;
    let n = r.u32()?;
    for _ in 0..n {
        anyhow::ensure!(r.u32()? == 0xABCD1234, "instance magic");
        r.u8()?;
        let id = r.u64()?;
        let asset = r.u64()?;
        r.u8()?;
        let count = r.u32()?;
        r.u32()?;
        r.u8()?;
        let mut props = HashMap::new();
        for _ in 0..count {
            let t = r.u8()?;
            let key = r.u32()?;
            let v = match t {
                0 => Prop::Num(r.u8()? as u64),
                1..=3 => Prop::Num(r.u32()? as u64),
                4 => Prop::F(r.f32()?),
                5 => Prop::Num(r.u64()?),
                6 => Prop::V3([r.f32()?, r.f32()?, r.f32()?]),
                7 => {
                    r.take(16)?;
                    Prop::V4
                }
                8 => {
                    let n = r.u32()? as usize;
                    let b = r.take(n)?;
                    let u: Vec<u16> = b
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    Prop::S(
                        String::from_utf16_lossy(&u)
                            .trim_end_matches('\0')
                            .to_string(),
                    )
                }
                9 => {
                    let n = r.u32()? as usize;
                    r.take(n)?;
                    Prop::Blob
                }
                _ => anyhow::bail!("property type {t}"),
            };
            props.insert(key, v);
        }
        out.push(Inst {
            id,
            room: room.to_string(),
            asset,
            props,
        });
    }
    Ok(())
}

type M3 = [[f64; 3]; 3];

fn mul(a: &M3, b: &M3) -> M3 {
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// Euler degrees (rx, ry, rz) composed Z, then X, then Y (y up): R = Ry Rx Rz.
fn rot_zxy(d: [f32; 3]) -> M3 {
    let [rx, ry, rz] = d.map(|a| (a as f64).to_radians());
    let (cx, sx, cy, sy, cz, sz) = (rx.cos(), rx.sin(), ry.cos(), ry.sin(), rz.cos(), rz.sin());
    let x = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let y = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let z = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(&y, &mul(&x, &z))
}

fn v3(p: &HashMap<u32, Prop>, k: u32, def: [f32; 3]) -> [f32; 3] {
    match p.get(&k) {
        Some(Prop::V3(v)) => *v,
        _ => def,
    }
}
fn num(p: &HashMap<u32, Prop>, k: u32) -> Option<u64> {
    match p.get(&k) {
        Some(Prop::Num(n)) => Some(*n),
        _ => None,
    }
}
fn flt(p: &HashMap<u32, Prop>, k: u32) -> Option<f32> {
    match p.get(&k) {
        Some(Prop::F(f)) => Some(*f),
        _ => None,
    }
}
fn txt(p: &HashMap<u32, Prop>, k: u32) -> Option<&str> {
    match p.get(&k) {
        Some(Prop::S(s)) if !s.is_empty() => Some(s.as_str()),
        _ => None,
    }
}

#[derive(Clone)]
struct World {
    pos: [f64; 3],
    m: M3,
    sc: [f64; 3],
}

fn resolve(
    id: u64,
    by_id: &HashMap<u64, usize>,
    insts: &[Inst],
    memo: &mut HashMap<u64, World>,
    depth: u32,
) -> World {
    if let Some(w) = memo.get(&id) {
        return w.clone();
    }
    let p = &insts[by_id[&id]].props;
    let pos = v3(p, POS, [0.0; 3]).map(|v| v as f64);
    let m = rot_zxy(v3(p, ROT, [0.0; 3]));
    let sc = v3(p, SCALE, [1.0; 3]).map(|v| v as f64);
    let parent = num(p, PARENT).unwrap_or(0);
    let w = if parent != 0 && by_id.contains_key(&parent) && depth < 32 {
        let pw = resolve(parent, by_id, insts, memo, depth + 1);
        let local = [pos[0] * pw.sc[0], pos[1] * pw.sc[1], pos[2] * pw.sc[2]];
        let mut wp = pw.pos;
        for (i, w) in wp.iter_mut().enumerate() {
            *w += (0..3).map(|k| pw.m[i][k] * local[k]).sum::<f64>();
        }
        World {
            pos: wp,
            m: mul(&pw.m, &m),
            sc: [sc[0] * pw.sc[0], sc[1] * pw.sc[1], sc[2] * pw.sc[2]],
        }
    } else {
        World { pos, m, sc }
    };
    memo.insert(id, w.clone());
    w
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// World (x, height, z) to the combat log's (a, b, c): a = 10x, b = 10z, c = 10 height.
fn to_log(p: [f64; 3]) -> [f64; 3] {
    [r2(p[0] * 10.0), r2(p[2] * 10.0), r2(p[1] * 10.0)]
}

/// \spn\codex\lore\hutta\varl.spn_p -> spn.codex.lore.hutta.varl
fn asset_fqn(path: &str) -> Option<String> {
    let p = path.strip_prefix('\\')?;
    let base = p.rsplit_once('.').map(|(b, _)| b).unwrap_or(p);
    Some(base.replace('\\', "."))
}

fn ext(path: &str) -> &str {
    path.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

struct Spice {
    conn: Connection,
    guid_fqn: HashMap<u64, String>,
    cache: HashMap<String, Option<GomValue>>,
}

impl Spice {
    fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let mut guid_fqn = HashMap::new();
        {
            let mut st = conn.prepare("SELECT guid, fqn FROM objects WHERE is_canonical = 1")?;
            let rows =
                st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (g, f) = row?;
                if let Ok(n) = u64::from_str_radix(&g, 16) {
                    guid_fqn.insert(n, f);
                }
            }
        }
        Ok(Spice {
            conn,
            guid_fqn,
            cache: HashMap::new(),
        })
    }

    fn object(&mut self, fqn: &str) -> Option<GomValue> {
        if let Some(v) = self.cache.get(fqn) {
            return v.clone();
        }
        let row: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT json_extract(json, '$.payload_b64'), json_extract(json, '$.header_hex') \
                 FROM objects WHERE fqn = ?1 AND is_canonical = 1 LIMIT 1",
                [fqn],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let v = row.and_then(|(b64, hh)| {
            let payload = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            let header = hex::decode(hh).ok()?;
            read_framed_object(&header, &payload).ok().map(|(v, _)| v)
        });
        self.cache.insert(fqn.to_string(), v.clone());
        v
    }

    fn name(&self, v: &GomValue) -> Option<String> {
        match v {
            GomValue::U64(n) => self.guid_fqn.get(n).cloned(),
            GomValue::I64(n) => self.guid_fqn.get(&(*n as u64)).cloned(),
            _ => None,
        }
    }
}

fn field<'a>(v: &'a GomValue, id: u32) -> Option<&'a GomValue> {
    match v {
        GomValue::Embedded(f) => f.iter().find(|(k, _)| *k as u32 == id).map(|(_, v)| v),
        _ => None,
    }
}

/// Every (hook, action, value) in a hydra script, the hook being the map key above it.
fn hydra_actions(v: &GomValue, hook: &str, out: &mut Vec<(String, String, String)>) {
    match v {
        GomValue::Embedded(f) => {
            if let (Some(GomValue::Str(a)), Some(val)) = (field(v, HYD_ACTION), field(v, HYD_VALUE))
            {
                let val = match val {
                    GomValue::Str(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                out.push((hook.to_string(), a.clone(), val));
            }
            for (_, x) in f {
                hydra_actions(x, hook, out);
            }
        }
        GomValue::List(l) => l.iter().for_each(|x| hydra_actions(x, hook, out)),
        GomValue::Map(m) => {
            for (k, x) in m {
                let h = match k {
                    GomValue::Str(s) => s.as_str(),
                    _ => hook,
                };
                hydra_actions(x, h, out);
            }
        }
        _ => {}
    }
}

fn is_grant(action: &str, value: &str) -> bool {
    let a = action.to_ascii_lowercase();
    a.contains("codex")
        || a.contains("achievement")
        || a.contains("grant")
        || a.contains("reward")
        || a.contains("give")
        || a.contains("quest") && !a.contains("set quest")
        || value.starts_with("cdx.")
        || value.starts_with("ach.")
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 5,
        "usage: <assets> <hashes> <spice.sqlite> <out.jsonl> [--all] [area id ...]"
    );
    let (assets, hashes_path, spice_path, out_path) = (&args[1], &args[2], &args[3], &args[4]);
    let all = args.iter().any(|a| a == "--all");
    let only: HashSet<&str> = args[5..]
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    let mut dict = HashDictionary::new();
    dict.load(hashes_path)?;
    let mut area_ids: Vec<String> = dict
        .paths_matching("/resources/world/areas/")
        .into_iter()
        .filter_map(|(_, p)| {
            p.strip_prefix("/resources/world/areas/")?
                .strip_suffix("/area.dat")
                .map(str::to_string)
        })
        .filter(|a| only.is_empty() || only.contains(a.as_str()))
        .collect();
    area_ids.sort();
    area_ids.dedup();

    let mut archives = Vec::new();
    let mut index: HashMap<u64, (usize, FileEntry)> = HashMap::new();
    for e in std::fs::read_dir(assets)? {
        let tor = e?.path();
        if tor.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let Ok(a) = Archive::open(&tor) else { continue };
        let entries: Vec<FileEntry> = a.entries()?.cloned().collect();
        let i = archives.len();
        for fe in entries {
            index.entry(fe.filename_hash).or_insert((i, fe));
        }
        archives.push(a);
    }
    let mut read = |path: &str| -> Option<Vec<u8>> {
        let (ph, sh) = hash::hashlittle2(path.as_bytes(), 0, 0);
        let (i, fe) = index.get(&hash::combine_hash(ph, sh))?;
        archives[*i].read_entry(fe).ok()
    };

    let mut spice = Spice::open(spice_path)?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path)?);
    let mut emit = |v: Value| -> Result<()> {
        serde_json::to_writer(&mut out, &v)?;
        out.write_all(b"\n")?;
        Ok(())
    };
    let mut templates_done: HashSet<String> = HashSet::new();
    let mut tally: HashMap<&'static str, u64> = HashMap::new();

    // Map pages: world.areas.<area>.mapdata. A page's name ref is the id the combat
    // log prints for the area in AreaEntered (area id = a page's name ref).
    let mapdata: Vec<String> = {
        let mut st = spice.conn.prepare(
            "SELECT fqn FROM objects WHERE fqn LIKE 'world.areas.%.mapdata' AND is_canonical = 1",
        )?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let mut log_ids: HashMap<String, Vec<u64>> = HashMap::new();
    for fqn in &mapdata {
        let Some(area) = fqn
            .strip_prefix("world.areas.")
            .and_then(|s| s.strip_suffix(".mapdata"))
        else {
            continue;
        };
        if !only.is_empty() && !only.contains(area) {
            continue;
        }
        let Some(obj) = spice.object(fqn) else {
            continue;
        };
        let Some(GomValue::List(pages)) = field(&obj, MAP_PAGES) else {
            continue;
        };
        for pg in pages {
            let name = match field(pg, PAGE_NAME) {
                Some(GomValue::Str(s)) => s.clone(),
                _ => continue,
            };
            let name_ref = field(pg, PAGE_NAME_REF).and_then(GomValue::as_i64);
            let tags: Vec<String> = match field(pg, PAGE_TAGS) {
                Some(GomValue::List(l)) => l
                    .iter()
                    .filter_map(|t| {
                        if let GomValue::Str(s) = t {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => vec![],
            };
            let vec = |id| match field(pg, id) {
                Some(GomValue::Vector3(v)) => json!(v),
                _ => Value::Null,
            };
            let page_id = format!("{area}/{name}");
            emit(
                json!({"node": "map_page", "id": page_id, "area": area, "name": name, "log_area_id": name_ref,
                "tags": tags, "world_min": vec(PAGE_MIN), "world_max": vec(PAGE_MAX),
                "image_min": vec(PAGE_IMG_MIN), "image_max": vec(PAGE_IMG_MAX)}),
            )?;
            emit(json!({"link": "has_page", "from": area, "to": page_id}))?;
            if let Some(r) = name_ref {
                log_ids.entry(area.to_string()).or_default().push(r as u64);
            }
            *tally.entry("map_pages").or_default() += 1;
        }
    }

    for area in &area_ids {
        let Some(ad) = read(&format!("/resources/world/areas/{area}/area.dat")) else {
            *tally.entry("area_missing").or_default() += 1;
            continue;
        };
        let ad = match read_area(&ad) {
            Ok(a) => a,
            Err(_) => {
                *tally.entry("area_unreadable").or_default() += 1;
                continue;
            }
        };
        let mut insts = Vec::new();
        let mut rooms_read = 0;
        for room in &ad.rooms {
            if let Some(d) = read(&format!("/resources/world/areas/{area}/{room}.dat")) {
                if read_room(&d, room, &mut insts).is_ok() {
                    rooms_read += 1;
                } else {
                    *tally.entry("room_unreadable").or_default() += 1;
                }
            }
        }
        let by_id: HashMap<u64, usize> = insts.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
        let resolved = insts
            .iter()
            .filter(|x| ad.assets.contains_key(&x.asset))
            .count();
        emit(
            json!({"node": "area", "id": area, "rooms": ad.rooms.len(), "rooms_read": rooms_read,
            "instances": insts.len(), "assets_resolved": resolved,
            "log_area_ids": log_ids.get(area.as_str()).cloned().unwrap_or_default()}),
        )?;
        *tally.entry("instances").or_default() += insts.len() as u64;
        *tally.entry("resolved").or_default() += resolved as u64;

        let mut memo = HashMap::new();
        for x in &insts {
            let path = ad.assets.get(&x.asset).map(String::as_str).unwrap_or("");
            let kind = ext(path);
            let script = txt(&x.props, SCRIPT);
            if !all && !GAMEPLAY.contains(&kind) && script.is_none() {
                continue;
            }
            let w = resolve(x.id, &by_id, &insts, &mut memo, 0);
            let lp = v3(&x.props, POS, [0.0; 3]);
            let parent = num(&x.props, PARENT).filter(|p| *p != 0);
            let template = if path.starts_with('\\') {
                asset_fqn(path)
            } else {
                path.strip_prefix("a:")
                    .map(|s| s.rsplit_once('.').map(|(b, _)| b).unwrap_or(s).to_string())
            };
            let mut node = json!({
                "node": "instance", "id": x.id.to_string(), "area": area, "room": x.room,
                "kind": kind, "asset_id": x.asset.to_string(), "asset": path, "template": template,
                "parent": parent.map(|p| p.to_string()),
                "local": {"pos": lp, "rot": v3(&x.props, ROT, [0.0; 3]), "scale": v3(&x.props, SCALE, [1.0; 3])},
                "world": w.pos.map(r2), "log": to_log(w.pos),
            });
            if kind == "trg" {
                let size = [
                    flt(&x.props, VOL_X),
                    flt(&x.props, VOL_Y),
                    flt(&x.props, VOL_Z),
                ];
                // Box centred on the instance, full sizes along its local axes; corners into the log frame.
                let mut lo = [f64::MAX; 3];
                let mut hi = [f64::MIN; 3];
                if let [Some(sx), Some(sy), Some(sz)] = size {
                    for c in 0..8 {
                        let l = [
                            if c & 1 == 0 { -0.5 } else { 0.5 } * sx as f64 * w.sc[0],
                            if c & 2 == 0 { -0.5 } else { 0.5 } * sy as f64 * w.sc[1],
                            if c & 4 == 0 { -0.5 } else { 0.5 } * sz as f64 * w.sc[2],
                        ];
                        let mut p = w.pos;
                        for (i, v) in p.iter_mut().enumerate() {
                            *v += (0..3).map(|k| w.m[i][k] * l[k]).sum::<f64>();
                        }
                        let lg = to_log(p);
                        for i in 0..3 {
                            lo[i] = lo[i].min(lg[i]);
                            hi[i] = hi[i].max(lg[i]);
                        }
                    }
                }
                let yaw = w.m[0][2].atan2(w.m[0][0]).to_degrees();
                node["volume"] = json!({
                    "shape": num(&x.props, VOL_SHAPE), "size_xyz": size, "yaw_deg": r2(yaw),
                    "log_aabb": if lo[0] <= hi[0] { json!({"min": lo, "max": hi}) } else { Value::Null },
                    "map_page": txt(&x.props, MAP_PAGE),
                });
            }
            if let Some(s) = script {
                node["script"] = json!(s);
            }
            emit(node)?;
            *tally.entry("emitted").or_default() += 1;
            emit(json!({"link": "contains", "from": area, "to": x.id.to_string()}))?;
            if let Some(t) = &template {
                emit(json!({"link": "is", "from": x.id.to_string(), "to": t}))?;
            }
            if let Some(s) = script {
                emit(json!({"link": "runs", "from": x.id.to_string(), "to": s}))?;
            }

            // Template chain, once per template.
            let mut queue: Vec<String> = template
                .iter()
                .cloned()
                .chain(script.map(str::to_string))
                .collect();
            while let Some(t) = queue.pop() {
                if !templates_done.insert(t.clone()) {
                    continue;
                }
                let Some(obj) = spice.object(&t) else {
                    continue;
                };
                if t.starts_with("spn.") {
                    if let Some(GomValue::List(es)) = field(&obj, SPN_ENTRIES) {
                        for e in es {
                            if let Some(GomValue::Str(f)) = field(e, SPN_ENTRY_TEMPLATE) {
                                emit(json!({"link": "spawns", "from": t, "to": f}))?;
                                queue.push(f.clone());
                            }
                        }
                    }
                } else if t.starts_with("plc.") {
                    if let Some(v) = field(&obj, PLC_ON_USE) {
                        let to = spice.name(v).unwrap_or_else(|| format!("{v:?}"));
                        // The log prints an ability as (string id << 32): the cast's id >> 32 is the ability's string_id.
                        let sid: Option<i64> = spice
                            .conn
                            .query_row(
                                "SELECT string_id FROM objects WHERE fqn = ?1 AND is_canonical = 1",
                                [&to],
                                |r| r.get(0),
                            )
                            .ok()
                            .flatten();
                        emit(
                            json!({"link": "plays", "from": t, "to": to, "field": "plcAbilitySpecOnUse",
                            "log_ability_id": sid.map(|s| (s as u64) << 32)}),
                        )?;
                    }
                    if let Some(GomValue::Str(c)) = field(&obj, PLC_CONVO) {
                        emit(json!({"link": "starts", "from": t, "to": c, "field": "plcConvo"}))?;
                    }
                    if let Some(v) = field(&obj, PLC_CODEX) {
                        if let Some(c) = spice.name(v) {
                            emit(
                                json!({"link": "grants", "from": t, "to": c, "field": "plcCodexSpec"}),
                            )?;
                        }
                    }
                    // Any other field naming a script, quest, achievement or item.
                    if let GomValue::Embedded(fs) = &obj {
                        for (fid, v) in fs {
                            let fid = *fid as u32;
                            if fid == PLC_ON_USE || fid == PLC_CODEX {
                                continue;
                            }
                            if let Some(n) = spice.name(v) {
                                if ["hyd.", "qst.", "ach.", "itm.", "cdx.", "cnv."]
                                    .iter()
                                    .any(|p| n.starts_with(p))
                                {
                                    emit(
                                        json!({"link": "refers", "from": t, "to": n, "field": format!("{fid:08x}")}),
                                    )?;
                                    if n.starts_with("hyd.") {
                                        queue.push(n);
                                    }
                                }
                            }
                        }
                    }
                } else if t.starts_with("hyd.") {
                    let mut acts = Vec::new();
                    hydra_actions(&obj, "", &mut acts);
                    for (hook, action, value) in acts {
                        if action.to_ascii_lowercase().contains("conversation") {
                            emit(
                                json!({"link": "starts", "from": t, "to": value, "hook": hook, "action": action}),
                            )?;
                        } else if is_grant(&action, &value) {
                            emit(
                                json!({"link": "grants", "from": t, "to": value, "hook": hook, "action": action}),
                            )?;
                        }
                    }
                }
            }
        }
    }
    drop(emit);
    eprintln!("areas {} {:?}", area_ids.len(), tally);
    Ok(())
}
