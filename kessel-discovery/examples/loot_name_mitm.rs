//! loot_name_mitm -- name loot packages by meeting FNV-1a 64 in the middle.
//!
//! A loot package id is FNV-1a 64 of the uppercased name `trp.<path>` (RESEARCH 01a1c27a).
//! FNV-1a 64 is invertible one byte at a time (the prime is odd), so the state before a
//! suffix can be computed from the final hash. This builds every forward state for
//! `<root>` + up to F tokens (each joined by '.' or '_'), and for each target every backward
//! state for up to B tokens (each preceded by '.' or '_'), and prints each target whose two
//! sides meet: name = forward string + backward string. A 64-bit meet is not chance: with
//! N forward and M backward states the expected false meets are N*M/2^64.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example loot_name_mitm -- \
//!     <targets.txt: one decimal u64 or i64 per line> <tokens.txt> <root> <F> <B>

use std::collections::HashMap;

const OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

fn fwd(mut h: u64, s: &[u8]) -> u64 {
    for &b in s {
        h = (h ^ u64::from(b.to_ascii_uppercase())).wrapping_mul(PRIME);
    }
    h
}

fn back(mut h: u64, s: &[u8], inv: u64) -> u64 {
    for &b in s.iter().rev() {
        h = h.wrapping_mul(inv) ^ u64::from(b.to_ascii_uppercase());
    }
    h
}

fn inverse(p: u64) -> u64 {
    // Newton iteration for the inverse of an odd number mod 2^64.
    let mut x = p;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(p.wrapping_mul(x)));
    }
    x
}

fn lines(path: &str) -> anyhow::Result<Vec<String>> {
    Ok(std::fs::read_to_string(path)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 6,
        "usage: <targets.txt> <tokens.txt> <root> <F> <B>"
    );
    let targets: Vec<u64> = lines(&args[1])?
        .iter()
        .filter_map(|l| {
            l.parse::<u64>()
                .ok()
                .or_else(|| l.parse::<i64>().ok().map(|v| v as u64))
        })
        .collect();
    let mut tokens = lines(&args[2])?;
    tokens.sort();
    tokens.dedup();
    // <root> is one literal root, or a file of roots (one per line) when such a file exists.
    let roots: Vec<String> = if std::path::Path::new(&args[3]).is_file() {
        lines(&args[3])?
    } else {
        vec![args[3].clone()]
    };
    let f_depth: usize = args[4].parse()?;
    let b_depth: usize = args[5].parse()?;
    let inv = inverse(PRIME);
    anyhow::ensure!(PRIME.wrapping_mul(inv) == 1, "inverse failed");
    let seps = [b".", b"_"];

    // Forward: state -> name.
    let mut forward: HashMap<u64, String> = HashMap::new();
    let mut frontier: Vec<(u64, String)> = roots
        .iter()
        .map(|r| (fwd(OFFSET, r.as_bytes()), r.clone()))
        .collect();
    for (h, r) in &frontier {
        forward.entry(*h).or_insert_with(|| r.clone());
    }
    for _ in 0..f_depth {
        let mut next = Vec::new();
        for (h, name) in &frontier {
            for sep in &seps {
                let hs = fwd(*h, *sep);
                for t in &tokens {
                    let h2 = fwd(hs, t.as_bytes());
                    let joined = format!("{name}{}{t}", sep[0] as char);
                    forward.entry(h2).or_insert_with(|| joined.clone());
                    next.push((h2, joined));
                }
            }
        }
        frontier = next;
    }
    eprintln!(
        "{} targets, {} tokens, {} forward states",
        targets.len(),
        tokens.len(),
        forward.len()
    );

    // Backward per target, depth-first over token suffixes, without allocating per step.
    let ctx = Ctx {
        forward: &forward,
        tokens: &tokens,
        inv,
        depth: b_depth,
    };
    for &target in &targets {
        let mut path = Vec::new();
        let mut found = Vec::new();
        ctx.search(target, &mut path, &mut found);
        for name in found {
            println!("{} {name}", target as i64);
        }
    }
    Ok(())
}

struct Ctx<'a> {
    forward: &'a HashMap<u64, String>,
    tokens: &'a [String],
    inv: u64,
    depth: usize,
}

impl Ctx<'_> {
    /// `path` holds (token index, separator) pairs from the end of the name backwards.
    fn search(&self, h: u64, path: &mut Vec<(usize, u8)>, found: &mut Vec<String>) {
        if let Some(prefix) = self.forward.get(&h) {
            let mut name = prefix.clone();
            for &(t, sep) in path.iter().rev() {
                name.push(sep as char);
                name.push_str(&self.tokens[t]);
            }
            found.push(name);
        }
        if path.len() == self.depth {
            return;
        }
        for (i, t) in self.tokens.iter().enumerate() {
            let h1 = back(h, t.as_bytes(), self.inv);
            for sep in [b'.', b'_'] {
                let h2 = back(h1, &[sep], self.inv);
                path.push((i, sep));
                self.search(h2, path, found);
                path.pop();
            }
        }
    }
}
