//! gr2_census -- decode push 3 (#429), media group: read every SWTOR model
//! (.gr2, magic 'GAWB') in the archives, found by content with no dictionary
//! (FR-KESSEL-024), and prove the layout by consistency checks over all of
//! them. Prints counts; every check that fails is counted, never skipped.
//!
//!   cargo run --release -p kessel-discovery --example gr2_census -- <assets dir> [dict]
//!
//! Layout read (little endian, version 5 = 64-bit offsets):
//!   0x00 'GAWB'  0x04 u32 version  0x08 u32 minor  0x0C u32 tail offset
//!   0x10 u32 relocation count  0x14 u32 file kind (0 model, 1 , 2 skeleton)
//!   0x18 u16 meshes 0x1A u16 materials 0x1C u16 bones 0x1E u16 attachments
//!   0x20 f32[8] bounds (min xyzw, max xyzw)  0x40 16 zero bytes
//!   0x50 u64 relocation table  0x58 u64 meshes  0x60 u64 material names
//!   0x68 u64 bones  0x70 u64 attachments
//!   mesh header 64 B: u64 name, u32 flags1, u16 pieces, u16 used bones,
//!     u32 vertex flags, u32 vertex size, u32 vertices, u32 indices,
//!     u64 vertices, u64 pieces, u64 indices, u64 used-bone table
//!   piece 48 B: u32 first index/3, u32 triangles, i32 material, i32 piece, f32[8] bounds
//!   used bone 32 B: u64 name, f32[6] bounds
//!   bone 144 B: u64 name, i32 parent, u32 0, f32[16] local, f32[16] inverse world
//!   attachment 80 B: u64 name, u64 bone name, f32[16] transform
//!   relocation 8 B: u32 where, u32 value (the u64 at 'where' holds 'value')
//!   tail: section at the 0x0C offset, then 'EGCD' u32 version u32 tail offset

use std::collections::BTreeMap;
use std::path::PathBuf;

use kessel::hash::HashDictionary;
use kessel::myp::Archive;

#[derive(Default)]
struct Tally(BTreeMap<String, u64>);
impl Tally {
    fn add(&mut self, k: impl Into<String>, n: u64) {
        *self.0.entry(k.into()).or_insert(0) += n;
    }
    fn inc(&mut self, k: impl Into<String>) {
        self.add(k, 1)
    }
}

struct R<'a>(&'a [u8]);
impl R<'_> {
    fn u16(&self, o: usize) -> Option<u16> {
        self.0.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, o: usize) -> Option<u32> {
        self.0.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&self, o: usize) -> Option<i32> {
        self.u32(o).map(|v| v as i32)
    }
    fn u64(&self, o: usize) -> Option<u64> {
        self.0.get(o..o + 8).map(|b| {
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            u64::from_le_bytes(a)
        })
    }
    fn f32(&self, o: usize) -> Option<f32> {
        self.u32(o).map(f32::from_bits)
    }
    fn cstr(&self, o: usize) -> Option<String> {
        let s = self.0.get(o..)?;
        let end = s.iter().position(|b| *b == 0)?;
        Some(String::from_utf8_lossy(&s[..end]).into_owned())
    }
}

/// Bytes per vertex implied by the vertex flags.
fn vertex_size(flags: u32) -> u32 {
    let mut n = 0;
    if flags & 0x001 != 0 {
        n += 12; // position f32x3
    }
    if flags & 0x100 != 0 {
        n += 8; // bone weights u8x4 + bone indices u8x4
    }
    if flags & 0x002 != 0 {
        n += 8; // normal u8x4 + tangent u8x4
    }
    if flags & 0x010 != 0 {
        n += 4; // colour u8x4
    }
    for bit in [0x020, 0x040, 0x080] {
        if flags & bit != 0 {
            n += 4; // uv f16x2
        }
    }
    n
}

fn check_model(d: &[u8], t: &mut Tally, names: &mut Tally) -> Result<(), String> {
    let r = R(d);
    let ver = r.u32(4).ok_or("short")?;
    t.inc(format!("version {ver}"));
    t.inc(format!("minor {}", r.u32(8).ok_or("short")?));
    if ver != 5 {
        return Err(format!("version {ver}"));
    }
    let tail = r.u32(0x0C).ok_or("short")? as usize;
    let nreloc = r.u32(0x10).ok_or("short")? as usize;
    let kind = r.u32(0x14).ok_or("short")?;
    t.inc(format!("kind {kind}"));
    let nmesh = r.u16(0x18).ok_or("short")? as usize;
    let nmat = r.u16(0x1A).ok_or("short")? as usize;
    let nbone = r.u16(0x1C).ok_or("short")? as usize;
    let natt = r.u16(0x1E).ok_or("short")? as usize;
    let off = |o| r.u64(o).map(|v| v as usize).ok_or("short");
    let (o_rel, o_mesh, o_mat, o_bone, o_att) = (off(0x50)?, off(0x58)?, off(0x60)?, off(0x68)?, off(0x70)?);
    if d[0x40..0x50].iter().any(|b| *b != 0) {
        t.inc("x 0x40..0x50 nonzero");
    }
    // count fields agree with the presence of their tables
    for (n, o, what) in [(nmesh, o_mesh, "mesh"), (nmat, o_mat, "material"), (nbone, o_bone, "bone"), (natt, o_att, "attachment")] {
        if (n == 0) != (o == 0) {
            t.inc(format!("x {what} count/offset disagree"));
        }
    }
    // footer
    let n = d.len();
    if n >= 12 && &d[n - 12..n - 8] == b"EGCD" {
        t.inc("footer EGCD");
        if r.u32(n - 8) != Some(ver) || r.u32(n - 4).map(|v| v as usize) != Some(tail) {
            t.inc("x footer fields disagree with header");
        }
        t.add("tail section bytes", (n - 12).saturating_sub(tail) as u64);
        t.inc(format!("tail section size {}", (n - 12).saturating_sub(tail)));
    } else {
        t.inc("x no EGCD footer");
    }
    // relocations: every pointer in the file is listed and holds its value
    let mut relocs = 0u64;
    for i in 0..nreloc {
        let at = r.u32(o_rel + i * 8).ok_or("reloc table short")? as usize;
        let val = r.u32(o_rel + i * 8 + 4).ok_or("reloc table short")? as u64;
        if r.u64(at) != Some(val) {
            t.inc("x relocation value mismatch");
        }
        relocs += 1;
    }
    t.add("relocations checked", relocs);
    if o_rel + nreloc * 8 > tail {
        t.inc("x relocation table overruns tail");
    }

    // header bounds
    let bmin = [r.f32(0x20).unwrap_or(0.0), r.f32(0x24).unwrap_or(0.0), r.f32(0x28).unwrap_or(0.0)];
    let bmax = [r.f32(0x30).unwrap_or(0.0), r.f32(0x34).unwrap_or(0.0), r.f32(0x38).unwrap_or(0.0)];

    for m in 0..nmesh {
        let h = o_mesh + m * 64;
        let name = r.cstr(off(h)?).ok_or("mesh name")?;
        let flags1 = r.u32(h + 8).ok_or("mesh")?;
        let npieces = r.u16(h + 12).ok_or("mesh")? as usize;
        let nused = r.u16(h + 14).ok_or("mesh")? as usize;
        let vflags = r.u32(h + 16).ok_or("mesh")?;
        let vsize = r.u32(h + 20).ok_or("mesh")?;
        let nvert = r.u32(h + 24).ok_or("mesh")? as usize;
        let nidx = r.u32(h + 28).ok_or("mesh")? as usize;
        let (o_v, o_p, o_i, o_u) = (off(h + 32)?, off(h + 40)?, off(h + 48)?, off(h + 56)?);
        t.inc("meshes");
        t.add("vertices", nvert as u64);
        t.add("triangles", (nidx / 3) as u64);
        t.inc(format!("mesh flags1 {flags1:08x}"));
        t.inc(format!("vertex flags {vflags:04x} size {vsize}"));
        if vertex_size(vflags) != vsize {
            t.inc(format!("x vertex size {vsize} != flags-implied {} (flags {vflags:x})", vertex_size(vflags)));
        }
        if name.to_ascii_lowercase().contains("collision") || flags1 & 0x2000_0000 != 0 {
            t.inc(format!("mesh name has 'collision': {} / flags1 bit29: {}", name.to_ascii_lowercase().contains("collision"), flags1 & 0x2000_0000 != 0));
        }
        if nidx % 3 != 0 {
            t.inc("x index count not a multiple of 3");
        }
        if o_v + nvert * vsize as usize > d.len() || o_i + nidx * 2 > d.len() {
            t.inc("x mesh buffer out of file");
            continue;
        }
        // indices < vertex count
        let mut bad_idx = 0;
        for k in 0..nidx {
            if r.u16(o_i + 2 * k).unwrap_or(u16::MAX) as usize >= nvert {
                bad_idx += 1;
            }
        }
        if bad_idx > 0 {
            t.inc("x index >= vertex count");
        }
        // pieces tile the index buffer
        let mut covered = 0usize;
        for p in 0..npieces {
            let ph = o_p + p * 48;
            let first = r.u32(ph).ok_or("piece")? as usize;
            let tris = r.u32(ph + 4).ok_or("piece")? as usize;
            let mat = r.i32(ph + 8).ok_or("piece")?;
            if first != covered {
                t.inc("x piece does not start where the previous ended");
            }
            covered = first + tris;
            if mat >= 0 && mat as usize >= nmat {
                t.inc("x piece material index >= material count");
            }
            t.inc(if mat < 0 { "piece material -1" } else { "piece material indexed" });
            t.inc("pieces");
        }
        if covered * 3 != nidx {
            t.inc("x pieces do not cover all triangles");
        }
        // vertices: positions inside the header bounds, weights sum to 255,
        // bone indices inside the used-bone table
        let mut pos_out = 0;
        let mut wsum_bad = 0;
        let mut bidx_bad = 0;
        for v in 0..nvert {
            let b = o_v + v * vsize as usize;
            let mut c = b;
            if vflags & 1 != 0 {
                let p = [r.f32(c).unwrap_or(f32::NAN), r.f32(c + 4).unwrap_or(f32::NAN), r.f32(c + 8).unwrap_or(f32::NAN)];
                if (0..3).any(|k| !(p[k] >= bmin[k] - 1e-3 && p[k] <= bmax[k] + 1e-3)) {
                    pos_out += 1;
                }
                c += 12;
            }
            if vflags & 0x100 != 0 {
                let w: u32 = d[c..c + 4].iter().map(|x| *x as u32).sum();
                if w != 255 {
                    wsum_bad += 1;
                }
                if d[c + 4..c + 8].iter().zip(&d[c..c + 4]).any(|(i, w)| *w > 0 && *i as usize >= nused) {
                    bidx_bad += 1;
                }
            }
        }
        if pos_out > 0 {
            t.inc("x mesh with vertex outside header bounds");
            t.add("x vertices outside header bounds", pos_out);
        }
        if wsum_bad > 0 {
            t.inc("x mesh with bone weights not summing to 255");
            t.add("x vertices weights != 255", wsum_bad);
        }
        if bidx_bad > 0 {
            t.inc("x mesh with bone index >= used bones");
        }
        if vflags & 0x100 != 0 && nused == 0 {
            t.inc("x skinned mesh with no used bones");
        }
        for u in 0..nused {
            if r.cstr(off(o_u + u * 32)?).is_none() {
                t.inc("x used bone name unreadable");
            }
        }
    }
    for m in 0..nmat {
        match off(o_mat + m * 8).ok().and_then(|o| r.cstr(o)) {
            Some(s) => {
                t.inc("materials");
                names.inc(if s.contains(['/', '\\']) { "material name is a path" } else { "material name is a bare name" });
            }
            None => t.inc("x material name unreadable"),
        }
    }
    for b in 0..nbone {
        let h = o_bone + b * 144;
        let parent = r.i32(h + 8).ok_or("bone")?;
        if r.cstr(off(h)?).is_none() {
            t.inc("x bone name unreadable");
        }
        if parent >= b as i32 {
            t.inc("x bone parent not before child");
        }
        if r.u32(h + 12) != Some(0) {
            t.inc("x bone pad nonzero");
        }
        t.inc("bones");
    }
    for a in 0..natt {
        let h = o_att + a * 80;
        if r.cstr(off(h)?).is_none() || r.cstr(off(h + 8)?).is_none() {
            t.inc("x attachment name unreadable");
        }
        t.inc("attachments");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let assets = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("/home/sean/swtor/Assets"));
    let mut dict = HashDictionary::new();
    let have_dict = args.get(2).map(|p| dict.load(p).is_ok()).unwrap_or(false);
    let mut tors: Vec<PathBuf> = std::fs::read_dir(&assets)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "tor"))
        .collect();
    tors.sort();
    let mut t = Tally::default();
    let mut names = Tally::default();
    for tor in &tors {
        let mut ar = Archive::open(tor)?;
        let entries: Vec<_> = ar.entries()?.cloned().collect();
        for e in &entries {
            // the content test needs only the first bytes, but entries are compressed whole
            let d = match ar.read_entry(e) {
                Ok(d) => d,
                Err(_) => {
                    t.inc("read errors");
                    continue;
                }
            };
            if d.len() < 4 || &d[..4] != b"GAWB" {
                continue;
            }
            t.inc("files");
            if have_dict {
                let ext = match dict.get(e.filename_hash) {
                    Some(p) if p.ends_with(".lod.gr2") => "dict .lod.gr2",
                    Some(p) if p.ends_with(".gr2") => "dict .gr2",
                    Some(_) => "dict other extension",
                    None => "dict unnamed",
                };
                t.inc(ext);
            }
            if let Err(err) = check_model(&d, &mut t, &mut names) {
                t.inc(format!("x unreadable: {err}"));
            }
        }
        eprintln!("{}", tor.display());
    }
    for (k, v) in t.0.iter().chain(names.0.iter()) {
        if v > &0 && !(k.starts_with("tail section size") && *v < 50) && !(k.starts_with("vertex flags") && *v < 20) && !(k.starts_with("mesh flags1") && *v < 20) {
            println!("{k:<70} {v:>12}");
        }
    }
    Ok(())
}
