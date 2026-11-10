//! world3_dir_files -- for each area id, which files under /resources/world/areas/<id>/
//! the archives hold.
//!
//! Decode push 3, world (the 203 map-data areas with no area file). Candidate names come
//! from two sources: fixed names (area.dat, mapnotes.not) hashed directly, and every
//! dictionary path under the directory, each checked against the archive index (the
//! dictionary only proposes names; presence is the archive's answer).
//!
//! Usage: world3_dir_files <assets dir> <ids.txt> <hashes_filename.txt>
//! Prints: id, fixed-name presence, dictionary paths listed, dictionary paths present,
//! and up to 3 present paths.

use anyhow::Result;
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use std::collections::{HashMap, HashSet};

fn key(s: &str) -> u64 {
    let (ph, sh) = kessel::hash::hashlittle2(s.as_bytes(), 0, 0);
    kessel::hash::combine_hash(ph, sh)
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    anyhow::ensure!(a.len() == 4, "usage: <assets> <ids.txt> <hashes>");
    let mut present: HashSet<u64> = HashSet::new();
    for e in std::fs::read_dir(&a[1])? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "tor") {
            if let Ok(ar) = Archive::open(&p) {
                present.extend(ar.entries()?.map(|f| f.filename_hash));
            }
        }
    }
    let mut dict = HashDictionary::new();
    dict.load(&a[3])?;
    let mut by_dir: HashMap<String, Vec<String>> = HashMap::new();
    for (_, p) in dict.paths_matching("/resources/world/areas/") {
        if let Some(rest) = p.strip_prefix("/resources/world/areas/") {
            if let Some((d, _)) = rest.split_once('/') {
                by_dir.entry(d.to_string()).or_default().push(p.clone());
            }
        }
    }
    println!("id\tarea_dat\tmapnotes_not\tdict_listed\tdict_present\texamples");
    for id in std::fs::read_to_string(&a[2])?.split_whitespace() {
        let ad = present.contains(&key(&format!("/resources/world/areas/{id}/area.dat")));
        let mn = present.contains(&key(&format!("/resources/world/areas/{id}/mapnotes.not")));
        let listed = by_dir.get(id).cloned().unwrap_or_default();
        let here: Vec<&String> = listed
            .iter()
            .filter(|p| present.contains(&key(p)))
            .collect();
        let ex: Vec<&str> = here.iter().take(3).map(|s| s.as_str()).collect();
        println!(
            "{id}\t{ad}\t{mn}\t{}\t{}\t{}",
            listed.len(),
            here.len(),
            ex.join(",")
        );
    }
    Ok(())
}
