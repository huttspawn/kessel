//! Extract every archive file whose dictionary path contains a pattern.
//! Usage: extract_paths <assets_dir> <hashes_filename.txt> <pattern> <out_dir>

use anyhow::{Context, Result};
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, assets, hash_file, pattern, out_dir] = args.as_slice() else {
        anyhow::bail!("usage: extract_paths <assets_dir> <hashes> <pattern> <out_dir>");
    };
    let mut hashes = HashDictionary::new();
    hashes.load(hash_file)?;
    let wanted: HashMap<u64, String> = hashes
        .paths_matching(pattern)
        .into_iter()
        .map(|(h, p)| (h, p.clone()))
        .collect();
    eprintln!("{} paths match", wanted.len());
    let out = PathBuf::from(out_dir);
    let mut written = 0;
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
            let data = archive.read_entry(e)?;
            let dest = out.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(dest.parent().context("no parent")?)?;
            std::fs::write(&dest, &data)?;
            written += 1;
        }
    }
    eprintln!("wrote {written} files");
    Ok(())
}
