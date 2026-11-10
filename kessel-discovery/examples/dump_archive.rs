//! Write every entry of one .tor archive to <out_dir>/<filename hash, 16 hex>.bin, and print
//! one line per entry: hash, size, first 4 bytes, and the dictionary path when the optional
//! hashes_filename.txt names it. Used to find files the dictionary does not name (for example
//! the 8.0 GFx movies in publictest/retailclient/main_gfx_1.tor).
//!
//! Usage: dump_archive <archive.tor> <out_dir> [hashes_filename.txt]

use anyhow::Result;
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3 || args.len() == 4,
        "usage: <archive.tor> <out_dir> [hashes_filename.txt]"
    );
    let mut dict = HashDictionary::new();
    if let Some(path) = args.get(3) {
        dict.load(path)?;
    }
    let names: std::collections::HashMap<u64, String> = dict
        .paths_matching("/")
        .into_iter()
        .map(|(h, p)| (h, p.clone()))
        .collect();
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out)?;
    let mut archive = Archive::open(&PathBuf::from(&args[1]))?;
    let entries: Vec<_> = archive.entries()?.cloned().collect();
    for e in &entries {
        let data = archive.read_entry(e)?;
        let magic: String = data
            .iter()
            .take(4)
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
            .collect();
        std::fs::write(out.join(format!("{:016x}.bin", e.filename_hash)), &data)?;
        println!(
            "{:016x}\t{}\t{}\t{}",
            e.filename_hash,
            data.len(),
            magic,
            names.get(&e.filename_hash).map_or("", String::as_str)
        );
    }
    Ok(())
}
