//! hash_paths -- print the archive filename hash of each path on stdin, the
//! way the archive keys entries (combine_hash(hashlittle2(path, 0, 0))), so a
//! derived path can be tested against a content census with no dictionary.
//!
//!   hash_paths < paths.txt > hashes.tsv     (HASH \t path)

use std::io::{BufRead, Write};

fn main() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout());
    for line in stdin.lock().lines() {
        let p = line?;
        let (ph, sh) = kessel::hash::hashlittle2(p.as_bytes(), 0, 0);
        writeln!(out, "{:016X}\t{p}", kessel::hash::combine_hash(ph, sh))?;
    }
    Ok(())
}
