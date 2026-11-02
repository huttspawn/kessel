//! pinf_dump -- the prototype registry (systemgenerated/prototypes.info), found
//! by its known path with no dictionary, written as `guid<TAB>flag` per record.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example pinf_dump -- <assets dir> > pinf.tsv

use kessel::hash;
use kessel::myp::Archive;
use std::collections::BTreeMap;
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: <assets dir>"))?;
    let path = "/resources/systemgenerated/prototypes.info";
    let (ph, sh) = hash::hashlittle2(path.as_bytes(), 0, 0);
    let want = hash::combine_hash(ph, sh);
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
        for entry in entries.iter().filter(|e| e.filename_hash == want) {
            let data = archive.read_entry(entry)?;
            let recs = kessel::prototypes_info::parse(&data)?;
            let mut flags: BTreeMap<u8, usize> = BTreeMap::new();
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            for r in &recs {
                *flags.entry(r.flag).or_insert(0) += 1;
                writeln!(out, "{}\t{}", r.content_guid, r.flag)?;
            }
            eprintln!(
                "{}: {} records, by flag {flags:?}",
                tor.display(),
                recs.len()
            );
            return Ok(());
        }
    }
    anyhow::bail!("prototypes.info not found")
}
