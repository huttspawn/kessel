//! gom_name_guess -- propose names for client.gom type records a new build adds.
//!
//! A type record keeps FNV-1a 32 of its uppercased name. This walks every name of the form
//! `<prefix><Word1>..<WordN>` (N up to the depth given) over a prefix list and a word list and
//! prints each one whose hash is a target. It proposes; it does not prove. A 32-bit hit is a
//! name only when it fits the record's system, shape and siblings (see recover_gom_names.rs);
//! with P prefixes, V words and depth N, about P * V^N * targets / 2^32 hits are chance.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example gom_name_guess -- \
//!     <targets.txt: one hex hash per line> <prefixes.txt> <words.txt> <depth>

use std::collections::HashSet;

const OFFSET: u32 = 0x811C_9DC5;
const PRIME: u32 = 0x0100_0193;

fn step(mut h: u32, s: &[u8]) -> u32 {
    for &b in s {
        h = (h ^ u32::from(b.to_ascii_uppercase())).wrapping_mul(PRIME);
    }
    h
}

fn lines(path: &str) -> anyhow::Result<Vec<String>> {
    Ok(std::fs::read_to_string(path)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

fn walk(
    h: u32,
    name: &mut String,
    depth: usize,
    words: &[String],
    targets: &HashSet<u32>,
    hits: &mut Vec<String>,
) {
    if depth == 0 {
        return;
    }
    for w in words {
        let len = name.len();
        name.push_str(w);
        let next = step(h, w.as_bytes());
        if targets.contains(&next) {
            hits.push(format!("{next:08X} {name}"));
        }
        walk(next, name, depth - 1, words, targets, hits);
        name.truncate(len);
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 5,
        "usage: <targets.txt> <prefixes.txt> <words.txt> <depth>"
    );
    let targets: HashSet<u32> = lines(&args[1])?
        .iter()
        .filter_map(|l| u32::from_str_radix(l.trim_start_matches("0x"), 16).ok())
        .collect();
    let prefixes = lines(&args[2])?;
    let mut words = lines(&args[3])?;
    words.sort();
    words.dedup();
    let depth: usize = args[4].parse()?;
    eprintln!(
        "{} targets, {} prefixes, {} words, depth {depth}",
        targets.len(),
        prefixes.len(),
        words.len()
    );
    let mut hits = Vec::new();
    for p in &prefixes {
        let h = step(OFFSET, p.as_bytes());
        if targets.contains(&h) {
            hits.push(format!("{h:08X} {p}"));
        }
        let mut name = p.clone();
        walk(h, &mut name, depth, &words, &targets, &mut hits);
    }
    for hit in hits {
        println!("{hit}");
    }
    Ok(())
}
