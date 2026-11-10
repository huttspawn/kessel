//! wem_id_sweep -- name streamed wems with no dictionary and no bank reference
//! by hashing /resources/bnk2/streamed/<n>.wem for every u32 n and matching
//! the archive hashes given (one 016X per line).
//!
//!   wem_id_sweep <hashes.txt>     -> prints HASH \t n for each match

use std::collections::HashSet;
use std::sync::Mutex;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, list] = args.as_slice() else {
        anyhow::bail!("usage: wem_id_sweep <hashes.txt>");
    };
    let want: HashSet<u64> = std::fs::read_to_string(list)?
        .lines()
        .filter_map(|l| u64::from_str_radix(l.split('\t').next().unwrap_or("").trim(), 16).ok())
        .collect();
    let found: Mutex<Vec<(u64, u32)>> = Mutex::new(Vec::new());
    let threads: u64 = 16;
    std::thread::scope(|s| {
        for t in 0..threads {
            let want = &want;
            let found = &found;
            s.spawn(move || {
                let mut buf = String::with_capacity(64);
                let mut n = t;
                while n <= u32::MAX as u64 {
                    buf.clear();
                    buf.push_str("/resources/bnk2/streamed/");
                    buf.push_str(&n.to_string());
                    buf.push_str(".wem");
                    let (ph, sh) = kessel::hash::hashlittle2(buf.as_bytes(), 0, 0);
                    let h = kessel::hash::combine_hash(ph, sh);
                    if want.contains(&h) {
                        if let Ok(mut g) = found.lock() {
                            g.push((h, n as u32));
                        }
                    }
                    n += threads;
                }
            });
        }
    });
    let g = found.lock().map_err(|_| anyhow::anyhow!("poison"))?;
    for (h, n) in g.iter() {
        println!("{h:016X}\t{n}");
    }
    eprintln!("matched {} of {}", g.len(), want.len());
    Ok(())
}
