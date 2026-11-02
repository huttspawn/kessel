//! find_bytes -- list every archive entry whose bytes contain any given text.
//!
//! Searches each entry (decompressed) for each needle as ASCII and as
//! UTF-16-LE, and prints the entry hash, archive and which needle matched.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example find_bytes -- <assets dir> <text> [text...]

use kessel::myp::Archive;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let Some((dir, texts)) = args.get(1).zip(args.get(2..)) else {
        anyhow::bail!("usage: <assets dir> <text> [text...]");
    };
    let mut needles: Vec<(String, Vec<u8>)> = Vec::new();
    for t in texts {
        needles.push((format!("{t} (ascii)"), t.as_bytes().to_vec()));
        let wide: Vec<u8> = t.encode_utf16().flat_map(u16::to_le_bytes).collect();
        needles.push((format!("{t} (utf16)"), wide));
    }
    let (mut scanned, mut hits) = (0u64, 0u64);
    for tor in std::fs::read_dir(dir)? {
        let tor = tor?.path();
        if tor.extension().is_none_or(|e| e != "tor") {
            continue;
        }
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for entry in &entries {
            let Ok(data) = archive.read_entry(entry) else {
                continue;
            };
            scanned += 1;
            for (label, n) in &needles {
                if data.windows(n.len()).any(|w| w == n.as_slice()) {
                    hits += 1;
                    println!(
                        "{:016X} {} bytes in {}: {label}",
                        entry.filename_hash,
                        data.len(),
                        tor.display()
                    );
                }
            }
        }
    }
    println!("scanned {scanned} entries, {hits} hits");
    Ok(())
}
