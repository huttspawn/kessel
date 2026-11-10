//! extract_by_hash -- write archive entries named by their filename hash,
//! with no dictionary (FR-KESSEL-024): the hashes come from a census
//! (media_census) or any list, one 016X hash per line (first column).
//!
//!   extract_by_hash <assets_dir> <hashes.txt> <out_dir>
//!
//! Each entry lands at <out_dir>/<HASH>. Missing hashes are reported.

use anyhow::Result;
use kessel::myp::{Archive, FileEntry};
use std::collections::HashSet;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, assets, list, out] = args.as_slice() else {
        anyhow::bail!("usage: extract_by_hash <assets_dir> <hashes.txt> <out_dir>");
    };
    let mut want: HashSet<u64> = HashSet::new();
    for l in std::fs::read_to_string(list)?.lines() {
        let h = l.split(['\t', ' ']).next().unwrap_or("");
        if let Ok(v) = u64::from_str_radix(h.trim(), 16) {
            want.insert(v);
        }
    }
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let mut tors: Vec<PathBuf> = std::fs::read_dir(assets)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "tor").unwrap_or(false))
        .collect();
    tors.sort();
    let mut got = 0usize;
    for p in &tors {
        let mut a = Archive::open(p)?;
        let es: Vec<FileEntry> = a
            .entries()?
            .filter(|e| want.contains(&e.filename_hash))
            .cloned()
            .collect();
        for e in &es {
            let d = a.read_entry(e)?;
            std::fs::write(out.join(format!("{:016X}", e.filename_hash)), d)?;
            want.remove(&e.filename_hash);
            got += 1;
        }
    }
    println!("extracted {got}, missing {}", want.len());
    Ok(())
}
