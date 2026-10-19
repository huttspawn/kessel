//! Dump STB string tables from the .tor archives.
//!
//! Selects STB files by a path substring from the hash dictionary, plus any
//! `--guess <path>` candidates (hashed directly, for files the dictionary does
//! not know yet). Prints `path|id1|id2|text`, one entry per line.
//!
//! Usage:
//!   cargo run -p kessel-discovery --example dump_stb -- \
//!     <assets dir> <hashes_filename.txt> <path substring> [--id2 N]... [--guess PATH]...

use anyhow::{Context, Result};
use kessel::hash::{swtor_filename_hash, HashDictionary};
use kessel::myp::Archive;
use kessel::stb;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let usage =
        "usage: <assets dir> <hashes_filename.txt> <path substring> [--id2 N]... [--guess PATH]...";
    let assets = PathBuf::from(args.get(1).context(usage)?);
    let hashes = args.get(2).context(usage)?;
    let pattern = args.get(3).context(usage)?;

    let mut id2_filter = HashSet::new();
    let mut guesses = Vec::new();
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "--id2" => {
                id2_filter.insert(args.get(i + 1).context(usage)?.parse::<u32>()?);
                i += 2;
            }
            "--guess" => {
                guesses.push(args.get(i + 1).context(usage)?.clone());
                i += 2;
            }
            _ => i += 1,
        }
    }

    let mut dict = HashDictionary::new();
    dict.load(hashes)?;
    let mut wanted: HashMap<u64, String> = dict
        .paths_matching(pattern)
        .into_iter()
        .filter(|(_, p)| p.ends_with(".stb"))
        .map(|(h, p)| (h, p.clone()))
        .collect();
    for g in guesses {
        wanted.insert(swtor_filename_hash(&g), g);
    }

    let mut tors: Vec<PathBuf> = std::fs::read_dir(&assets)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "tor"))
        .collect();
    tors.sort();

    let mut found = HashSet::new();
    for tor in tors {
        let mut archive = Archive::open(&tor)?;
        let entries: Vec<_> = archive
            .entries()?
            .filter(|e| wanted.contains_key(&e.filename_hash))
            .cloned()
            .collect();
        for entry in entries {
            let path = &wanted[&entry.filename_hash];
            let data = archive.read_entry(&entry)?;
            let file = match stb::parse(&data, path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    continue;
                }
            };
            found.insert(entry.filename_hash);
            for s in file.entries {
                if id2_filter.is_empty() || id2_filter.contains(&s.id2) {
                    println!("{path}|{}|{}|{}", s.id1, s.id2, s.text.replace('\n', "\\n"));
                }
            }
        }
    }
    for (h, p) in &wanted {
        if !found.contains(h) {
            eprintln!("not found: {p}");
        }
    }
    Ok(())
}
