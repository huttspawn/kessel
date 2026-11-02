//! prototype_census -- every PROT prototype in a build, found by content.
//!
//! Walks every archive entry, keeps the ones that start with the PROT magic,
//! and writes `guid<TAB>fqn<TAB>payload bytes` per prototype to stdout. Prints
//! a count per FQN prefix to stderr.
//!
//! Prediction, written before the run: the GUIDs that game objects reference
//! and kessel never extracts (the 0xE000 values in the field sweep) are these
//! prototypes, and their names cover abilities, effects and more than
//! conversations.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example prototype_census -- <assets dir> > prototypes.tsv

use kessel::myp::Archive;
use std::collections::BTreeMap;
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <assets dir>"))?;
    let mut tors: Vec<_> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "tor").unwrap_or(false))
        .collect();
    tors.sort();
    let mut prefixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut failed = 0usize;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for tor in tors {
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for entry in entries {
            let Ok(data) = archive.read_entry(&entry) else {
                continue;
            };
            if data.len() < 20 || &data[..4] != b"PROT" {
                continue;
            }
            match kessel::node::parse(&data, 0, 0) {
                Ok(rec) => {
                    let p = rec.fqn.split('.').next().unwrap_or("").to_string();
                    *prefixes.entry(p).or_insert(0) += 1;
                    writeln!(
                        out,
                        "{}\t{}\t{}",
                        rec.template_guid,
                        rec.fqn,
                        rec.payload.len()
                    )?;
                }
                Err(_) => failed += 1,
            }
        }
    }
    eprintln!("prototypes by prefix: {prefixes:?}");
    eprintln!("unparsed PROT entries: {failed}");
    Ok(())
}
