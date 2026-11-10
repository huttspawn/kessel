//! dds_decode_all -- decode every DDS texture in the install, every array
//! layer / cube face and every mip, and report what fails (#429 criterion 7).
//!
//! Entries are found by content (magic `DDS ` at byte 0, FR-KESSEL-024); no
//! dictionary. The decoder is the one kessel's icon extraction uses
//! (image_dds 0.4 over ddsfile), driven through `Surface::decode_rgba8`, which
//! decodes all layers and mips, not only mip 0 as `--icons` does.
//!
//!   dds_decode_all <assets_dir> <failures.tsv>
//!
//! Prints per-format counts: entries, decoded, failed, total decoded pixels.

use anyhow::Result;
use image_dds::ddsfile::Dds;
use image_dds::Surface;
use kessel::myp::{Archive, FileEntry};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Default)]
struct Tally {
    entries: u64,
    decoded: u64,
    failed: u64,
    pixels: u64,
}

fn fmt_key(d: &Dds) -> String {
    if let Some(f) = d.get_dxgi_format() {
        return format!("DXGI:{f:?}");
    }
    if let Some(f) = d.get_d3d_format() {
        return format!("D3D:{f:?}");
    }
    let pf = &d.header.spf;
    format!(
        "PF:{:?}/{:?}/bits{:?}",
        pf.flags,
        pf.fourcc.as_ref().map(|c| c.0),
        pf.rgb_bit_count
    )
}

/// Direct read of an uncompressed 32-bit X8R8G8B8 surface: B,G,R,X bytes per
/// pixel, mips (and cube faces) back to back. Returns the pixel count when the
/// data length is exactly what the header implies, so every byte is accounted.
fn bgrx_pixels(d: &Dds) -> Option<u64> {
    if d.get_d3d_format() != Some(image_dds::ddsfile::D3DFormat::X8R8G8B8) {
        return None;
    }
    let faces = if d.header.caps2.contains(image_dds::ddsfile::Caps2::CUBEMAP) {
        6
    } else {
        1
    };
    let (mut w, mut h) = (d.get_width() as u64, d.get_height() as u64);
    let mut px = 0u64;
    for _ in 0..d.get_num_mipmap_levels().max(1) {
        px += w * h;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    px *= faces;
    // Every pixel must be present; the X byte is padding (alpha = 255).
    (px * 4 == d.data.len() as u64).then_some(px)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, assets, fails] = args.as_slice() else {
        anyhow::bail!("usage: dds_decode_all <assets_dir> <failures.tsv>");
    };
    let mut tors: Vec<PathBuf> = std::fs::read_dir(assets)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "tor").unwrap_or(false))
        .collect();
    tors.sort();
    let tally: Mutex<BTreeMap<String, Tally>> = Mutex::new(BTreeMap::new());
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..14 {
            s.spawn(|| -> Result<()> {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(p) = tors.get(i) else { break };
                    let mut a = Archive::open(p)?;
                    let es: Vec<FileEntry> = a.entries()?.cloned().collect();
                    let mut local: BTreeMap<String, Tally> = BTreeMap::new();
                    let mut lf = Vec::new();
                    for e in &es {
                        let d = match a.read_entry(e) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        if !kessel::dds::is_dds(&d) {
                            continue;
                        }
                        let h = format!("{:016X}", e.filename_hash);
                        let dds = match Dds::read(&mut Cursor::new(&d[..])) {
                            Ok(x) => x,
                            Err(err) => {
                                let t = local.entry("HEADER-UNREADABLE".into()).or_default();
                                t.entries += 1;
                                t.failed += 1;
                                lf.push(format!("{h}\tHEADER\t{}\t{err}", d.len()));
                                continue;
                            }
                        };
                        let key = fmt_key(&dds);
                        let t = local.entry(key.clone()).or_default();
                        t.entries += 1;
                        let r = Surface::from_dds(&dds)
                            .map_err(|e| e.to_string())
                            .and_then(|s| s.decode_rgba8().map_err(|e| e.to_string()));
                        match r {
                            Ok(img) => {
                                t.decoded += 1;
                                t.pixels += (img.data.len() / 4) as u64;
                            }
                            Err(err) => {
                                // Fallbacks: X8R8G8B8 is plain 32-bit BGRX that
                                // image_dds 0.4 does not list; read it directly
                                // (every mip, size checked). Otherwise try mip 0.
                                if let Some(px) = bgrx_pixels(&dds) {
                                    t.decoded += 1;
                                    t.pixels += px;
                                    lf.push(format!("{h}\t{key}\t{}\tBGRX-DIRECT-OK", d.len()));
                                } else if let Ok(img) = image_dds::image_from_dds(&dds, 0) {
                                    t.failed += 1;
                                    lf.push(format!(
                                        "{h}\t{key}\t{}\tMIP0-ONLY {}x{}: {err}",
                                        d.len(),
                                        img.width(),
                                        img.height()
                                    ));
                                } else {
                                    t.failed += 1;
                                    lf.push(format!("{h}\t{key}\t{}\t{err}", d.len()));
                                }
                            }
                        }
                    }
                    let mut g = tally.lock().map_err(|_| anyhow::anyhow!("poison"))?;
                    for (k, v) in local {
                        let t = g.entry(k).or_default();
                        t.entries += v.entries;
                        t.decoded += v.decoded;
                        t.failed += v.failed;
                        t.pixels += v.pixels;
                    }
                    failures
                        .lock()
                        .map_err(|_| anyhow::anyhow!("poison"))?
                        .extend(lf);
                }
                Ok(())
            });
        }
    });
    let g = tally.lock().map_err(|_| anyhow::anyhow!("poison"))?;
    let (mut n, mut ok) = (0, 0);
    println!("{:<40} {:>9} {:>9} {:>7} {:>16}", "format", "entries", "decoded", "failed", "pixels");
    for (k, t) in g.iter() {
        n += t.entries;
        ok += t.decoded;
        println!(
            "{k:<40} {:>9} {:>9} {:>7} {:>16}",
            t.entries, t.decoded, t.failed, t.pixels
        );
    }
    println!("TOTAL {n} entries, {ok} decoded (all layers and mips)");
    let f = failures.lock().map_err(|_| anyhow::anyhow!("poison"))?;
    let mut w = std::fs::File::create(fails)?;
    for l in f.iter() {
        writeln!(w, "{l}")?;
    }
    println!("{} failures written", f.len());
    Ok(())
}
