//! dump_by_path -- write one archive entry to disk, found by its path's hash.
//!
//! Hashes the given resource path the way the archive keys entries, finds the
//! entry in the build's archives, and writes its bytes to the output file.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example dump_by_path -- <assets dir> <resource path> <out file>

use kessel::hash;
use kessel::myp::Archive;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, path, out] = args.as_slice() else {
        anyhow::bail!("usage: <assets dir> <resource path> <out file>");
    };
    let (ph, sh) = hash::hashlittle2(path.as_bytes(), 0, 0);
    let wanted = hash::combine_hash(ph, sh);
    for tor in std::fs::read_dir(dir)? {
        let tor = tor?.path();
        if tor.extension().is_none_or(|e| e != "tor") {
            continue;
        }
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for entry in entries.iter().filter(|e| e.filename_hash == wanted) {
            let data = archive.read_entry(entry)?;
            std::fs::write(out, &data)?;
            println!("{path}: {} bytes from {}", data.len(), tor.display());
            return Ok(());
        }
    }
    anyhow::bail!("{path} not found (hash {wanted:016X})")
}
