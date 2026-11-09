//! Extract archive files by resource path, from the dictionary or from a list.
//!
//! Two modes:
//!   extract_by_path <assets_dir> dict <hashes_filename.txt> <prefix> <exts,comma> <out_dir>
//!     every dictionary path starting with <prefix> whose extension is listed
//!   extract_by_path <assets_dir> list <paths.txt> <out_dir>
//!     every path in the file (one per line), hashed the way the archive keys
//!     entries, so paths the dictionary lacks are found too; misses are reported
//!
//! Paths are written under <out_dir> keeping their own layout.

use anyhow::{Context, Result};
use kessel::hash::{self, HashDictionary};
use kessel::myp::Archive;
use std::collections::HashMap;
use std::path::PathBuf;

fn hash_path(path: &str) -> u64 {
    let (ph, sh) = hash::hashlittle2(path.as_bytes(), 0, 0);
    hash::combine_hash(ph, sh)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (assets, wanted, out_dir): (&String, HashMap<u64, String>, &String) = match args.as_slice()
    {
        [_, assets, mode, hash_file, prefix, exts, out_dir] if mode == "dict" => {
            let mut hashes = HashDictionary::new();
            hashes.load(hash_file)?;
            let exts: Vec<String> = exts.split(',').map(|e| format!(".{e}")).collect();
            let wanted = hashes
                .paths_matching(prefix)
                .into_iter()
                .filter(|(_, p)| {
                    p.starts_with(prefix.as_str()) && exts.iter().any(|e| p.ends_with(e))
                })
                .map(|(h, p)| (h, p.clone()))
                .collect();
            (assets, wanted, out_dir)
        }
        [_, assets, mode, list, out_dir] if mode == "list" => {
            let text = std::fs::read_to_string(list)?;
            let wanted = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|p| (hash_path(p), p.to_string()))
                .collect();
            (assets, wanted, out_dir)
        }
        _ => anyhow::bail!(
            "usage: <assets> dict <hashes> <prefix> <exts> <out> | <assets> list <paths.txt> <out>"
        ),
    };
    eprintln!("{} paths wanted", wanted.len());
    let out = PathBuf::from(out_dir);
    let mut found: HashMap<u64, ()> = HashMap::new();
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
            let Some(path) = wanted.get(&e.filename_hash) else {
                continue;
            };
            if found.contains_key(&e.filename_hash) {
                continue;
            }
            let data = archive.read_entry(e)?;
            let dest = out.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(dest.parent().context("no parent")?)?;
            std::fs::write(&dest, &data)?;
            found.insert(e.filename_hash, ());
        }
    }
    let missing: Vec<&String> = wanted
        .iter()
        .filter(|(h, _)| !found.contains_key(h))
        .map(|(_, p)| p)
        .collect();
    eprintln!(
        "wrote {} files, {} not in the archives",
        found.len(),
        missing.len()
    );
    for p in missing.iter().take(20) {
        eprintln!("  missing {p}");
    }
    Ok(())
}
