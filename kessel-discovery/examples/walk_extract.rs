//! walk_extract -- where the client holds walkability (decode push 3, #429).
//!
//! Two modes, both reading the .tor archives directly:
//!
//!   walk_extract census <assets dir> <hashes_filename.txt> <out.json>
//!     Sweeps every archive entry. (1) For every entry the dictionary does not
//!     name, records its leading bytes, so the unnamed set can be checked for a
//!     navmesh or pathing format. (2) For every GR2 model (magic GAWB, found by
//!     content), reads the mesh table and counts collision meshes: meshes whose
//!     name holds "collision" and meshes with a 12-byte (position-only) vertex.
//!     The dictionary is only used to say which entries are unnamed.
//!
//!   walk_extract area <assets dir> <area dir id> <out dir>
//!     Reads /resources/world/areas/<id>/area.dat and its rooms (no dictionary:
//!     room paths come from area.dat) and writes <out>/instances.jsonl, one line
//!     per room instance with every property (vec3, numbers, strings; blobs go
//!     to <out>/blob/<instance>_<key>.bin), plus every .gr2 the instances
//!     reference into <out>/gr2/<asset id>.gr2. walk_mesh.py turns that into
//!     the walkable-surface drawing.
//!
//! GR2 (SWTOR "GAWB", read after the SWTOR Slicers Blender add-on's importer,
//! io_scene_gr2/ops/import_gr2.py): u32 magic 0x42574147, u32 version (4, or 5 =
//! 64-bit offsets), u32 type at 0x14, u16 mesh count at 0x18, u16 material count
//! 0x1A, u16 bone count 0x1C, eight f32 bounds at 0x30, offsets at 0x50
//! (cached offsets, mesh headers, material names, bones). Mesh header (40 bytes,
//! 64 in v5): name offset, u32 flag1, u16 pieces, u16 bones, u16 flag2, u16
//! vertex size, u32 vertex count, u32 face count, then offsets of vertices,
//! pieces, indices (u16 triples) and bones.

use anyhow::{Context, Result};
use kessel::hash::{self, HashDictionary};
use kessel::myp::{Archive, FileEntry};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

const GAWB: u32 = 0x4257_4147;

fn path_hash(p: &str) -> u64 {
    let (ph, sh) = hash::hashlittle2(p.as_bytes(), 0, 0);
    hash::combine_hash(ph, sh)
}

struct Archives {
    archives: Vec<Archive>,
    index: HashMap<u64, (usize, FileEntry)>,
}

impl Archives {
    fn open(dir: &str) -> Result<Self> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "tor"))
            .collect();
        paths.sort();
        let mut archives = Vec::new();
        let mut index = HashMap::new();
        for tor in paths {
            let Ok(a) = Archive::open(&tor) else { continue };
            let entries: Vec<FileEntry> = a.entries()?.cloned().collect();
            let i = archives.len();
            for fe in entries {
                index.entry(fe.filename_hash).or_insert((i, fe));
            }
            archives.push(a);
        }
        Ok(Archives { archives, index })
    }
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let (i, fe) = self.index.get(&path_hash(path))?.clone();
        self.archives[i].read_entry(&fe).ok()
    }
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}
fn cstr_at(d: &[u8], o: usize) -> String {
    let s = d.get(o..).unwrap_or(&[]);
    let n = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..n]).into_owned()
}

/// One GR2 mesh header: name, flag2, vertex size, vertex and face counts.
struct MeshHead {
    name: String,
    flag2: u32,
    vsize: u32,
    verts: u32,
    faces: u32,
}

fn gr2_meshes(d: &[u8]) -> Option<Vec<MeshHead>> {
    if u32_at(d, 0)? != GAWB {
        return None;
    }
    let v5 = u32_at(d, 4)? == 5;
    let n = u16_at(d, 0x18)? as usize;
    let off = if v5 {
        u64_at(d, 0x58)? as usize
    } else {
        u32_at(d, 0x54)? as usize
    };
    let size = if v5 { 64 } else { 40 };
    let mut out = Vec::new();
    for i in 0..n {
        let p = off + i * size;
        let (name_off, rest) = if v5 {
            (u64_at(d, p)? as usize, p + 8)
        } else {
            (u32_at(d, p)? as usize, p + 4)
        };
        let q = rest + 4 + 4; // flag1, pieces + bones
        let (flag2, vsize, q) = if v5 {
            (u32_at(d, q)?, u32_at(d, q + 4)?, q + 8)
        } else {
            (u16_at(d, q)? as u32, u16_at(d, q + 2)? as u32, q + 4)
        };
        out.push(MeshHead {
            name: cstr_at(d, name_off),
            flag2,
            vsize,
            verts: u32_at(d, q)?,
            faces: u32_at(d, q + 4)?,
        });
    }
    Some(out)
}

fn census(assets: &str, dict_path: &str, out: &str) -> Result<()> {
    let mut dict = HashDictionary::new();
    dict.load(dict_path)?;
    let mut arc = Archives::open(assets)?;
    let keys: Vec<u64> = arc.index.keys().copied().collect();
    eprintln!("{} unique entries", keys.len());

    let mut unnamed_magic: BTreeMap<String, u64> = BTreeMap::new();
    let mut unnamed_examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unnamed = 0u64;
    let mut gr2_total = 0u64;
    let mut gr2_with_coll_name = 0u64;
    let mut gr2_with_pos_only = 0u64;
    let mut coll_meshes = 0u64;
    let mut coll_mesh_vsize: BTreeMap<u32, u64> = BTreeMap::new();
    let mut coll_mesh_flag2: BTreeMap<u32, u64> = BTreeMap::new();
    let mut posonly_names: BTreeMap<String, u64> = BTreeMap::new();
    let mut gr2_versions: BTreeMap<u32, u64> = BTreeMap::new();
    let mut examples: Vec<Value> = Vec::new();
    let mut named_ext_coll: BTreeMap<String, u64> = BTreeMap::new();

    for (n, h) in keys.iter().enumerate() {
        if n % 100_000 == 0 {
            eprintln!("  {n}");
        }
        let (i, fe) = arc.index[h].clone();
        let Ok(d) = arc.archives[i].read_entry(&fe) else {
            continue;
        };
        let name = dict.get(*h).cloned();
        if name.is_none() {
            unnamed += 1;
            let head: String = d
                .iter()
                .take(4)
                .map(|b| {
                    if b.is_ascii_graphic() {
                        (*b as char).to_string()
                    } else {
                        format!("\\x{b:02x}")
                    }
                })
                .collect();
            *unnamed_magic.entry(head.clone()).or_default() += 1;
            let ex = unnamed_examples.entry(head).or_default();
            if ex.len() < 3 {
                ex.push(format!("{h:016x} len {}", d.len()));
            }
        }
        if let Some(ms) = gr2_meshes(&d) {
            gr2_total += 1;
            *gr2_versions.entry(u32_at(&d, 4).unwrap_or(0)).or_default() += 1;
            let mut has_name = false;
            let mut has_pos = false;
            for m in &ms {
                let is_coll = m.name.to_ascii_lowercase().contains("collision");
                if is_coll {
                    has_name = true;
                    coll_meshes += 1;
                    *coll_mesh_vsize.entry(m.vsize).or_default() += 1;
                    *coll_mesh_flag2.entry(m.flag2).or_default() += 1;
                }
                if m.vsize == 12 {
                    has_pos = true;
                    if !is_coll {
                        let key = m
                            .name
                            .rsplit('_')
                            .next()
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        *posonly_names.entry(key).or_default() += 1;
                    }
                }
            }
            if has_name {
                gr2_with_coll_name += 1;
                let kind = name
                    .as_deref()
                    .map(|p| p.split('/').take(4).collect::<Vec<_>>().join("/"))
                    .unwrap_or_else(|| "(unnamed)".into());
                *named_ext_coll.entry(kind).or_default() += 1;
                if examples.len() < 40 {
                    examples.push(json!({
                        "path": name,
                        "meshes": ms.iter().map(|m| json!({"name": m.name, "vsize": m.vsize,
                            "flag2": m.flag2, "verts": m.verts, "faces": m.faces})).collect::<Vec<_>>()
                    }));
                }
            }
            if has_pos {
                gr2_with_pos_only += 1;
            }
        }
    }
    let v = json!({
        "entries": keys.len(),
        "unnamed": unnamed,
        "unnamed_leading_bytes": unnamed_magic,
        "unnamed_examples": unnamed_examples,
        "gr2_total": gr2_total,
        "gr2_versions": gr2_versions,
        "gr2_with_collision_named_mesh": gr2_with_coll_name,
        "gr2_with_position_only_mesh": gr2_with_pos_only,
        "collision_meshes": coll_meshes,
        "collision_mesh_vertex_size": coll_mesh_vsize,
        "collision_mesh_flag2": coll_mesh_flag2,
        "position_only_noncollision_name_suffix": posonly_names,
        "collision_gr2_by_dir": named_ext_coll,
        "examples": examples,
    });
    std::fs::write(out, serde_json::to_string_pretty(&v)?)?;
    eprintln!("wrote {out}");
    Ok(())
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

fn area(assets: &str, area_id: &str, out_dir: &str) -> Result<()> {
    let mut arc = Archives::open(assets)?;
    let root = format!("/resources/world/areas/{area_id}");
    let ad = arc
        .read(&format!("{root}/area.dat"))
        .context("area.dat not found")?;
    let mut r = Rd { d: &ad, p: 0x56 };
    let rooms = (0..r.u32()?)
        .map(|_| r.lstr())
        .collect::<Result<Vec<_>>>()?;
    let mut assets_tab: BTreeMap<u64, String> = BTreeMap::new();
    for _ in 0..r.u32()? {
        let id = r.u64()?;
        assets_tab.insert(id, r.lstr()?);
    }
    std::fs::create_dir_all(format!("{out_dir}/blob"))?;
    std::fs::create_dir_all(format!("{out_dir}/gr2"))?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(format!(
        "{out_dir}/instances.jsonl"
    ))?);
    let mut used: BTreeMap<u64, u64> = BTreeMap::new();
    for room in &rooms {
        let path = format!("{root}/{}.dat", room.to_ascii_lowercase());
        let Some(d) = arc.read(&path) else {
            eprintln!("missing room {path}");
            continue;
        };
        let mut r = Rd { d: &d, p: 0 };
        let sig = r.lstr()?;
        anyhow::ensure!(sig.starts_with("ROOM_DAT_BINARY_FORMAT"), "not a room");
        r.p = r.u32()? as usize;
        for _ in 0..r.u32()? {
            anyhow::ensure!(r.u32()? == 0xABCD1234, "instance magic");
            r.u8()?;
            let id = r.u64()?;
            let asset = r.u64()?;
            r.u8()?;
            let count = r.u32()?;
            r.u32()?;
            r.u8()?;
            let mut props = serde_json::Map::new();
            for _ in 0..count {
                let t = r.u8()?;
                let key = r.u32()?;
                let k = format!("{key:08x}");
                let v = match t {
                    0 => json!(r.u8()?),
                    1..=3 => json!(r.u32()?),
                    4 => json!(r.f32()?),
                    5 => json!(r.u64()?.to_string()),
                    6 => json!([r.f32()?, r.f32()?, r.f32()?]),
                    7 => json!([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
                    8 => {
                        let n = r.u32()? as usize;
                        let u: Vec<u16> = r
                            .take(n)?
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        json!(String::from_utf16_lossy(&u).trim_end_matches('\0'))
                    }
                    9 => {
                        let n = r.u32()? as usize;
                        let b = r.take(n)?;
                        let f = format!("{out_dir}/blob/{id}_{k}.bin");
                        std::fs::write(&f, b)?;
                        json!({"blob": n})
                    }
                    _ => anyhow::bail!("property type {t}"),
                };
                props.insert(format!("{t}:{k}"), v);
            }
            *used.entry(asset).or_default() += 1;
            let ap = assets_tab.get(&asset).cloned();
            serde_json::to_writer(
                &mut out,
                &json!({"room": room, "id": id.to_string(), "asset": asset.to_string(),
                "path": ap, "props": props}),
            )?;
            out.write_all(b"\n")?;
        }
    }
    out.flush()?;
    let mut ext: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for (asset, n) in &used {
        let Some(p) = assets_tab.get(asset) else {
            continue;
        };
        let e = p.rsplit_once('.').map(|x| x.1).unwrap_or("").to_string();
        let ent = ext.entry(e.clone()).or_default();
        ent.0 += 1;
        ent.1 += n;
        if e == "gr2" {
            let ap = format!("/resources{}", p.replace('\\', "/")).to_ascii_lowercase();
            if let Some(d) = arc.read(&ap) {
                std::fs::write(format!("{out_dir}/gr2/{asset}.gr2"), d)?;
                ent.2 += 1;
            }
        }
    }
    std::fs::write(
        format!("{out_dir}/assets.json"),
        serde_json::to_string_pretty(&assets_tab)?,
    )?;
    for (e, (a, n, f)) in &ext {
        eprintln!("{e:>10}: {a} assets, {n} instances, {f} files read");
    }
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("census") if a.len() == 5 => census(&a[2], &a[3], &a[4]),
        Some("area") if a.len() == 5 => area(&a[2], &a[3], &a[4]),
        _ => anyhow::bail!(
            "usage: walk_extract census <assets> <hashes> <out.json> | area <assets> <area id> <out dir>"
        ),
    }
}
