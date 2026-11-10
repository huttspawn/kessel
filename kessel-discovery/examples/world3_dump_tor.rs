//! world3_dump_tor -- write every entry of one .tor to a directory, named by entry hash.
//!
//! Decode push 3, world: reads main_gfx_1.tor (the client UI's GFx movies, kept beside
//! swtor.exe, not under Assets). A GFx/CFX movie (zlib-compressed SWF, "CFX" header) is
//! also written inflated as <hash>.swf so its ActionScript strings can be read.
//!
//! Usage: world3_dump_tor <archive.tor> <out dir> [hashes_filename.txt]

use anyhow::Result;
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use std::io::{Read, Write};

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    anyhow::ensure!(a.len() >= 3, "usage: <archive.tor> <out dir> [hashes]");
    let mut dict = HashDictionary::new();
    if let Some(h) = a.get(3) {
        dict.load(h)?;
    }
    std::fs::create_dir_all(&a[2])?;
    let mut ar = Archive::open(std::path::Path::new(&a[1]))?;
    let list: Vec<_> = ar.entries()?.cloned().collect();
    let mut index = std::fs::File::create(format!("{}/index.tsv", a[2]))?;
    for e in &list {
        let d = ar.read_entry(e)?;
        let name = format!("{:016x}", e.filename_hash);
        std::fs::write(format!("{}/{name}.bin", a[2]), &d)?;
        let head = String::from_utf8_lossy(&d[..d.len().min(3)]).into_owned();
        if d.len() > 8 && (&d[..3] == b"CFX" || &d[..3] == b"CWS") {
            let mut z = flate2::read::ZlibDecoder::new(&d[8..]);
            let mut out = Vec::new();
            if z.read_to_end(&mut out).is_ok() {
                std::fs::write(format!("{}/{name}.swf", a[2]), &out)?;
            }
        }
        writeln!(
            index,
            "{name}\t{}\t{head}\t{}",
            d.len(),
            dict.get(e.filename_hash).map(|s| s.as_str()).unwrap_or("-")
        )?;
    }
    println!("entries {}", list.len());
    Ok(())
}
