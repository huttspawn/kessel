//! world2_area_scan -- find every area.dat in the archives by its content, with no
//! filename dictionary, then name the ones nothing in the client references.
//!
//! Decode push 2, world (gaps A8 follow-up, FR-KESSEL-024). Push 1 (RESEARCH
//! 01a1c274-7763) found 12 area directories that no object payload or world file
//! names, so a dictionary-free extract cannot reach them through any reference.
//!
//! Modes:
//!   scan  <assets dir> <out.tsv>
//!       Reads the first bytes of every entry of every .tor and keeps the ones
//!       that start with the u32-length signature "AREA_DAT_BINARY_FORMAT_";
//!       parses each fully (rooms at 0x56, then the asset table) and writes
//!       archive, entry hash, size, room count, asset count, first rooms.
//!   name  <scan.tsv> <known ids file> <out.tsv> <lo> <hi> [<lo> <hi> ...]
//!       Hashes "/resources/world/areas/<id>/area.dat" (the archive key) for
//!       every decimal id in the given bands and reports which scanned entries
//!       it names. The known ids file (dictionary-free sources: map pages and
//!       mapAreasDataProto) marks which entries are already reachable.
//!
//! Prediction written before the run: 267 area.dat entries; 249 named by the
//! known ids; the other 18 named by brute force inside the id bands the known
//! ids span, 12 of them the unreferenced ones.

use anyhow::{bail, Context, Result};
use kessel::myp::{Archive, FileEntry};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const SIG: &[u8] = b"AREA_DAT_BINARY_FORMAT_";

fn head(f: &mut File, e: &FileEntry, n: usize) -> Result<Vec<u8>> {
    f.seek(SeekFrom::Start(e.position + e.header_size as u64))?;
    let take = (e.compressed_size as usize).min(if e.compression == 0 { n } else { 1 << 18 });
    let mut buf = vec![0u8; take];
    f.read_exact(&mut buf)?;
    if e.compression == 0 {
        return Ok(buf);
    }
    let mut out = vec![0u8; n];
    if buf.len() >= 4 && buf[..4] == [0x28, 0xb5, 0x2f, 0xfd] {
        let mut d = zstd::stream::read::Decoder::new(&buf[..])?;
        let mut got = 0;
        while got < n {
            match d.read(&mut out[got..]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(_) => break,
            }
        }
        out.truncate(got);
    } else {
        let mut d = flate2::read::ZlibDecoder::new(&buf[..]);
        let mut got = 0;
        while got < n {
            match d.read(&mut out[got..]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(_) => break,
            }
        }
        out.truncate(got);
    }
    Ok(out)
}

fn lstr(d: &[u8], p: &mut usize) -> Option<String> {
    let n = u32::from_le_bytes(d.get(*p..*p + 4)?.try_into().ok()?) as usize;
    let s = d.get(*p + 4..*p + 4 + n)?;
    *p += 4 + n;
    Some(
        String::from_utf8_lossy(s)
            .trim_end_matches('\0')
            .to_string(),
    )
}

/// Rooms and asset count; None unless the asset table ends exactly at the
/// second section offset (0x24), the check push 1 found true in all 267 areas.
fn parse_area(d: &[u8]) -> Option<(Vec<String>, usize, bool)> {
    let mut p = 0;
    let sig = lstr(d, &mut p)?;
    if !sig.starts_with("AREA_DAT_BINARY_FORMAT") {
        return None;
    }
    let second = u32::from_le_bytes(d.get(0x24..0x28)?.try_into().ok()?) as usize;
    p = 0x56;
    let n = u32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?) as usize;
    p += 4;
    let mut rooms = Vec::with_capacity(n);
    for _ in 0..n {
        rooms.push(lstr(d, &mut p)?);
    }
    let na = u32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?) as usize;
    p += 4;
    for _ in 0..na {
        p += 8;
        lstr(d, &mut p)?;
    }
    Some((rooms, na, p == second))
}

fn scan(assets: &str, out: &str) -> Result<()> {
    let mut w = std::io::BufWriter::new(File::create(out)?);
    writeln!(
        w,
        "archive\thash\tsize\trooms\tassets\tends_at_section2\tfirst_rooms"
    )?;
    let (mut total, mut sig_hits, mut parsed) = (0u64, 0u64, 0u64);
    let mut tors: Vec<_> = std::fs::read_dir(assets)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "tor"))
        .collect();
    tors.sort();
    for tor in tors {
        let Ok(mut a) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<FileEntry> = a.entries()?.cloned().collect();
        let mut f = File::open(&tor)?;
        let name = tor
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for e in &entries {
            total += 1;
            let Ok(h) = head(&mut f, e, 4 + SIG.len()) else {
                continue;
            };
            if h.len() < 4 + SIG.len() || &h[4..4 + SIG.len()] != SIG {
                continue;
            }
            sig_hits += 1;
            let data = a.read_entry(e).context("read area.dat")?;
            let Some((rooms, na, ends)) = parse_area(&data) else {
                continue;
            };
            parsed += 1;
            let first: Vec<&str> = rooms.iter().take(6).map(|s| s.as_str()).collect();
            writeln!(
                w,
                "{name}\t{:016x}\t{}\t{}\t{na}\t{ends}\t{}",
                e.filename_hash,
                data.len(),
                rooms.len(),
                first.join(",")
            )?;
        }
    }
    println!("entries {total}  area.dat signature {sig_hits}  parsed {parsed}");
    Ok(())
}

fn key_hash(id: u64) -> u64 {
    let s = format!("/resources/world/areas/{id}/area.dat");
    let (ph, sh) = kessel::hash::hashlittle2(s.as_bytes(), 0, 0);
    kessel::hash::combine_hash(ph, sh)
}

fn mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32, u32) {
    a = a.wrapping_sub(c) ^ c.rotate_left(4);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a) ^ a.rotate_left(6);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b) ^ b.rotate_left(8);
    b = b.wrapping_add(a);
    a = a.wrapping_sub(c) ^ c.rotate_left(16);
    c = c.wrapping_add(b);
    b = b.wrapping_sub(a) ^ a.rotate_left(19);
    a = a.wrapping_add(c);
    c = c.wrapping_sub(b) ^ b.rotate_left(4);
    b = b.wrapping_add(a);
    (a, b, c)
}

fn final_mix(mut a: u32, mut b: u32, mut c: u32) -> (u32, u32) {
    c = (c ^ b).wrapping_sub(b.rotate_left(14));
    a = (a ^ c).wrapping_sub(c.rotate_left(11));
    b = (b ^ a).wrapping_sub(a.rotate_left(25));
    c = (c ^ b).wrapping_sub(b.rotate_left(16));
    a = (a ^ c).wrapping_sub(c.rotate_left(4));
    b = (b ^ a).wrapping_sub(a.rotate_left(14));
    c = (c ^ b).wrapping_sub(b.rotate_left(24));
    (b, c)
}

fn w(k: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([k[o], k[o + 1], k[o + 2], k[o + 3]])
}

/// lookup3 hashlittle2 state after the first two 12-byte blocks of the 51-byte
/// key "/resources/world/areas/<19 digits>/area.dat"; bytes 0..24 are the
/// prefix plus the id's first digit, constant over a band of 4xxx ids.
fn midstate(k: &[u8; 51]) -> (u32, u32, u32) {
    let init = 0xdeadbeef_u32.wrapping_add(51);
    let (mut a, mut b, mut c) = (init, init, init);
    for o in [0, 12] {
        (a, b, c) = mix(
            a.wrapping_add(w(k, o)),
            b.wrapping_add(w(k, o + 4)),
            c.wrapping_add(w(k, o + 8)),
        );
    }
    (a, b, c)
}

fn fast_key_hash(k: &[u8; 51], (mut a, mut b, mut c): (u32, u32, u32)) -> u64 {
    for o in [24, 36] {
        (a, b, c) = mix(
            a.wrapping_add(w(k, o)),
            b.wrapping_add(w(k, o + 4)),
            c.wrapping_add(w(k, o + 8)),
        );
    }
    a = a.wrapping_add(k[48] as u32 | (k[49] as u32) << 8 | (k[50] as u32) << 16);
    let (ph, sh) = final_mix(a, b, c);
    kessel::hash::combine_hash(ph, sh)
}

fn name(scan_tsv: &str, known: &str, out: &str, bands: &[(u64, u64)]) -> Result<()> {
    let mut targets: HashMap<u64, String> = HashMap::new();
    for line in BufReader::new(File::open(scan_tsv)?).lines().skip(1) {
        let line = line?;
        let c: Vec<&str> = line.split('\t').collect();
        targets.insert(u64::from_str_radix(c[1], 16)?, line.clone());
    }
    let known_ids: Vec<u64> = std::fs::read_to_string(known)?
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    let mut named: HashMap<u64, (u64, &'static str)> = HashMap::new();
    for id in &known_ids {
        let h = key_hash(*id);
        if targets.contains_key(&h) {
            named.insert(h, (*id, "known"));
        }
    }
    println!(
        "area.dat entries {}  named by known ids {}",
        targets.len(),
        named.len()
    );
    let open: Arc<Vec<u64>> = Arc::new(
        targets
            .keys()
            .filter(|h| !named.contains_key(h))
            .copied()
            .collect(),
    );
    // The fast path must agree with kessel's hash on every 19-digit id.
    let probe = *b"/resources/world/areas/4611686043810670011/area.dat";
    let mid = midstate(&probe);
    anyhow::ensure!(fast_key_hash(&probe, mid) == key_hash(4611686043810670011));
    anyhow::ensure!(
        bands
            .iter()
            .all(|&(lo, hi)| lo >= 4_000_000_000_000_000_000 && hi <= 4_999_999_999_999_999_999),
        "bands must be 19-digit ids starting with 4"
    );
    let found: Arc<Mutex<Vec<(u64, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4) as u64;
    for &(lo, hi) in bands {
        let next = Arc::new(AtomicU64::new(lo));
        const CHUNK: u64 = 1 << 24;
        let mut hs = Vec::new();
        for _ in 0..threads {
            let (open, found, next) = (open.clone(), found.clone(), next.clone());
            hs.push(std::thread::spawn(move || {
                let mut buf = *b"/resources/world/areas/0000000000000000000/area.dat";
                loop {
                    let start = next.fetch_add(CHUNK, Ordering::Relaxed);
                    if start >= hi {
                        break;
                    }
                    let end = (start + CHUNK).min(hi);
                    // 19-digit decimal ids only (every band here is 19 digits).
                    let mut digits = [0u8; 19];
                    let mut v = start;
                    for i in (0..19).rev() {
                        digits[i] = b'0' + (v % 10) as u8;
                        v /= 10;
                    }
                    for id in start..end {
                        buf[23..42].copy_from_slice(&digits);
                        let h = fast_key_hash(&buf, mid);
                        if open.contains(&h) {
                            found.lock().map(|mut f| f.push((h, id))).ok();
                        }
                        // increment decimal digits
                        let mut i = 18;
                        loop {
                            if digits[i] == b'9' {
                                digits[i] = b'0';
                                if i == 0 {
                                    break;
                                }
                                i -= 1;
                            } else {
                                digits[i] += 1;
                                break;
                            }
                        }
                    }
                }
            }));
        }
        for h in hs {
            h.join().ok();
        }
        println!(
            "band {lo}..{hi} done, found so far {}",
            found.lock().map(|f| f.len()).unwrap_or(0)
        );
    }
    for (h, id) in found.lock().map(|f| f.clone()).unwrap_or_default() {
        named.insert(h, (id, "brute"));
    }
    let mut w = std::io::BufWriter::new(File::create(out)?);
    writeln!(
        w,
        "area_id\thow\tarchive\thash\tsize\trooms\tassets\tends_at_section2\tfirst_rooms"
    )?;
    let mut rows: Vec<_> = targets.iter().collect();
    rows.sort_by_key(|(h, _)| named.get(h).map(|x| x.0).unwrap_or(0));
    let mut unnamed = 0;
    for (h, line) in rows {
        let (id, how) = named.get(h).copied().unwrap_or((0, "unnamed"));
        if how == "unnamed" {
            unnamed += 1;
        }
        writeln!(w, "{id}\t{how}\t{line}")?;
    }
    println!(
        "named known {}  brute {}  unnamed {unnamed}",
        named.values().filter(|x| x.1 == "known").count(),
        named.values().filter(|x| x.1 == "brute").count()
    );
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("scan") if a.len() == 4 => scan(&a[2], &a[3]),
        Some("name") if a.len() >= 7 && (a.len() - 5) % 2 == 0 => {
            let mut bands = Vec::new();
            for i in (5..a.len()).step_by(2) {
                bands.push((a[i].parse()?, a[i + 1].parse()?));
            }
            name(&a[2], &a[3], &a[4], &bands)
        }
        _ => bail!("usage: scan <assets> <out.tsv> | name <scan.tsv> <known ids> <out.tsv> <lo> <hi> [...]"),
    }
}
