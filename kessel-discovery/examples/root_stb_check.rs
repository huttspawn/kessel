//! root_stb_check -- the root string tables, found with no dictionary.
//!
//! For each locale and each root table name, hash the known path the way the
//! archive keys entries, find the entry in the build's archives, parse it, and
//! print its entry count. Compare against an extraction made with the dictionary.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example root_stb_check -- <assets dir>

use kessel::hash;
use kessel::myp::Archive;
use kessel::stb;
use std::collections::HashMap;

const ROOTS: [&str; 12] = [
    "itm", "abl", "ach", "qst", "npc", "cdx", "tal", "plc", "dec", "mtx", "gld", "spn",
];
const LOCALES: [&str; 3] = ["en-us", "fr-fr", "de-de"];

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <assets dir>"))?;
    let mut wanted: HashMap<u64, String> = HashMap::new();
    for loc in LOCALES {
        for root in ROOTS {
            let path = format!("/resources/{loc}/str/{root}.stb");
            let (ph, sh) = hash::hashlittle2(path.as_bytes(), 0, 0);
            wanted.insert(hash::combine_hash(ph, sh), path);
        }
    }
    for extra in [
        "/resources/en-us/str/gui/achievementcategories.stb",
        "/resources/en-us/str/gui/galacticcommand.stb",
        "/resources/en-us/str/gui/planetaryconquest.stb",
    ] {
        let (ph, sh) = hash::hashlittle2(extra.as_bytes(), 0, 0);
        wanted.insert(hash::combine_hash(ph, sh), extra.to_string());
    }
    let mut found: HashMap<String, usize> = HashMap::new();
    let mut tors: Vec<_> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "tor").unwrap_or(false))
        .collect();
    tors.sort();
    for tor in tors {
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for entry in entries {
            let Some(path) = wanted.get(&entry.filename_hash) else {
                continue;
            };
            let data = archive.read_entry(&entry)?;
            let n = stb::parse(&data, path)
                .map(|f| f.entries.len())
                .unwrap_or(0);
            found.insert(path.clone(), n);
        }
    }
    let mut paths: Vec<_> = wanted.values().collect();
    paths.sort();
    for p in paths {
        match found.get(p) {
            Some(n) => println!("found  {p}  {n} strings"),
            None => println!("absent {p}"),
        }
    }
    Ok(())
}
