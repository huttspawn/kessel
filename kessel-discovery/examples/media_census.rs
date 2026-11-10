//! media_census -- dictionary-free content census of every archive entry
//! (FR-KESSEL-024), for the media decode (#429 criterion 7).
//!
//! For each entry of each `*.tor` it decodes only the first PREFIX bytes
//! (zstd/zlib streamed from the start of the entry), so a 55 GB install is
//! swept without inflating it, and writes one TSV row:
//!
//!   archive  hash(016X)  compression  csize  usize  class  head_hex(160 bytes)
//!
//! `class` is assigned from content alone (magic bytes / text root); the
//! dictionary is never read here. Analyses run over the TSV afterwards.
//!
//! Run:
//!   cargo build --release -p kessel-discovery --example media_census
//!   ./target/release/examples/media_census <assets_dir> <out.tsv>

use anyhow::{Context, Result};
use kessel::myp::{Archive, FileEntry};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const PREFIX: usize = 4096;
const HEAD: usize = 160;

/// Decode the first `PREFIX` bytes of an entry.
fn read_prefix(f: &mut BufReader<File>, e: &FileEntry) -> Result<Vec<u8>> {
    f.seek(SeekFrom::Start(e.position + e.header_size as u64))?;
    let take = f.by_ref().take(e.compressed_size as u64);
    let mut out = Vec::with_capacity(PREFIX);
    if e.compression == 0 {
        take.take(PREFIX as u64).read_to_end(&mut out)?;
        return Ok(out);
    }
    let mut take = take;
    let mut magic = [0u8; 4];
    take.read_exact(&mut magic)?;
    let chained = std::io::Cursor::new(magic.to_vec()).chain(take);
    if magic == [0x28, 0xB5, 0x2F, 0xFD] {
        let mut d = zstd::stream::read::Decoder::with_buffer(BufReader::new(chained))?;
        let _ = d.by_ref().take(PREFIX as u64).read_to_end(&mut out);
    } else {
        let mut d = flate2::read::ZlibDecoder::new(chained);
        let _ = d.by_ref().take(PREFIX as u64).read_to_end(&mut out);
    }
    Ok(out)
}

fn starts(d: &[u8], m: &[u8]) -> bool {
    d.len() >= m.len() && &d[..m.len()] == m
}

/// Text root of an XML file, UTF-16-LE (BOM optional) or UTF-8.
fn xml_root(d: &[u8]) -> Option<String> {
    let text: String = if starts(d, &[0xFF, 0xFE]) || (d.len() > 3 && d[0] == b'<' && d[1] == 0) {
        let s = if starts(d, &[0xFF, 0xFE]) { &d[2..] } else { d };
        let u: Vec<u16> = s
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else {
        let s = if starts(d, &[0xEF, 0xBB, 0xBF]) { &d[3..] } else { d };
        String::from_utf8_lossy(s).into_owned()
    };
    let t = text.trim_start();
    if !t.starts_with('<') {
        return None;
    }
    // Skip <?xml ...?> and comments.
    let mut rest = t;
    loop {
        rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix("<?") {
            rest = r.split_once("?>").map(|x| x.1).unwrap_or("");
        } else if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.split_once("-->").map(|x| x.1).unwrap_or("");
        } else {
            break;
        }
    }
    let r = rest.strip_prefix('<')?;
    let name: String = r
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':' || *c == '-')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn classify(d: &[u8]) -> String {
    let m4: &[u8] = if d.len() >= 4 { &d[..4] } else { d };
    let tag = match m4 {
        b"DDS " => "DDS",
        b"BKHD" => "BKHD",
        b"RIFF" => "RIFF",
        b"GAWB" => "GAWB",
        b"JAWB" => "JAWB",
        b"PROT" => "PROT",
        b"PBUK" => "PBUK",
        b"DBLB" => "DBLB",
        b"SCPT" => "SCPT",
        b"OggS" => "OGG",
        b"CFX\x0a" | b"FWS\x0a" | b"CWS\x0a" => "GFX",
        _ => "",
    };
    if !tag.is_empty() {
        return tag.to_string();
    }
    if let Some(root) = xml_root(d) {
        return format!("XML:{root}");
    }
    // Fallback: the 4 leading bytes as hex, so unknown kinds cluster.
    format!("RAW:{}", hex::encode(m4))
}

fn list_tor(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "tor").unwrap_or(false))
        .collect();
    v.sort();
    Ok(v)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (assets, out) = match args.as_slice() {
        [_, a, o] => (PathBuf::from(a), PathBuf::from(o)),
        _ => anyhow::bail!("usage: media_census <assets_dir> <out.tsv>"),
    };
    let tors = list_tor(&assets)?;
    eprintln!("{} archives", tors.len());
    let w = Mutex::new(BufWriter::new(File::create(&out)?));
    let next = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);
    let total = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..12 {
            s.spawn(|| -> Result<()> {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(p) = tors.get(i) else { break };
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let archive = Archive::open(p).with_context(|| name.clone())?;
                    let entries: Vec<FileEntry> = archive.entries()?.cloned().collect();
                    let mut f = BufReader::new(File::open(p)?);
                    let mut rows = String::new();
                    for e in &entries {
                        total.fetch_add(1, Ordering::Relaxed);
                        let d = match read_prefix(&mut f, e) {
                            Ok(d) => d,
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                rows.push_str(&format!(
                                    "{name}\t{:016X}\t{}\t{}\t{}\tERROR\t\n",
                                    e.filename_hash,
                                    e.compression,
                                    e.compressed_size,
                                    e.uncompressed_size
                                ));
                                continue;
                            }
                        };
                        let class = classify(&d);
                        rows.push_str(&format!(
                            "{name}\t{:016X}\t{}\t{}\t{}\t{}\t{}\n",
                            e.filename_hash,
                            e.compression,
                            e.compressed_size,
                            e.uncompressed_size,
                            class.replace(['\t', '\n'], " "),
                            hex::encode(&d[..d.len().min(HEAD)])
                        ));
                    }
                    let mut g = w.lock().map_err(|_| anyhow::anyhow!("poisoned"))?;
                    g.write_all(rows.as_bytes())?;
                    eprintln!("  {name}: {} entries", entries.len());
                }
                Ok(())
            });
        }
    });
    w.lock()
        .map_err(|_| anyhow::anyhow!("poisoned"))?
        .flush()?;
    eprintln!(
        "entries {} read errors {}",
        total.load(Ordering::SeqCst),
        errors.load(Ordering::SeqCst)
    );
    Ok(())
}
