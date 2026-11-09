//! Meet-in-the-middle search for FNV-1a 64 preimages of the form PREFIX + X, where X is up to
//! L1 + L2 characters from an alphabet (default A-Z, space, underscore). Used for the opaque
//! hashed ids that are FNV-1a 64 of uppercased names (quest task hooks qstHookId 64287098,
//! hydra selectors, achievement event codes before the 48-bit fold).
//!
//! The target is a full FNV state: for a name ending in a known counter, pass the state before
//! the counter (invert the last bytes first).
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example fnv_mitm -- <prefix> <L1> <L2> <hex,hex,...> [alphabet]

use anyhow::{Context, Result};
use std::collections::HashMap;

const P: u64 = 0x0000_0100_0000_01b3;
const OFF: u64 = 0xcbf2_9ce4_8422_2325;

fn inv(p: u64) -> u64 {
    // Newton iteration for the inverse mod 2^64 of an odd number.
    let mut x = p;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(p.wrapping_mul(x)));
    }
    x
}

fn feed(mut h: u64, b: &[u8]) -> u64 {
    for &c in b {
        h ^= c as u64;
        h = h.wrapping_mul(P);
    }
    h
}

fn decode(mut idx: u64, len: usize, alpha: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; len];
    for slot in out.iter_mut().rev() {
        *slot = alpha[(idx % alpha.len() as u64) as usize];
        idx /= alpha.len() as u64;
    }
    out
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let prefix = args.get(1).context("prefix")?.replace('~', " ");
    let l1: usize = args.get(2).context("L1")?.parse()?;
    let l2: usize = args.get(3).context("L2")?.parse()?;
    let targets: Vec<u64> = args
        .get(4)
        .context("targets")?
        .split(',')
        .map(|t| u64::from_str_radix(t.trim_start_matches("0x"), 16))
        .collect::<Result<_, _>>()?;
    let alpha: Vec<u8> = args
        .get(5)
        .map(|a| a.replace('~', " ").into_bytes())
        .unwrap_or_else(|| b"ABCDEFGHIJKLMNOPQRSTUVWXYZ _".to_vec());
    let pinv = inv(P);
    assert_eq!(P.wrapping_mul(pinv), 1);
    let start = feed(OFF, prefix.as_bytes());
    // forward: every string of length 0..=l1 -> (len, index)
    let mut fwd: HashMap<u64, (usize, u64)> = HashMap::new();
    let mut layer: Vec<u64> = vec![start];
    fwd.insert(start, (0, 0));
    for len in 1..=l1 {
        let mut next = Vec::with_capacity(layer.len() * alpha.len());
        for (i, &h) in layer.iter().enumerate() {
            for (j, &c) in alpha.iter().enumerate() {
                let h2 = (h ^ c as u64).wrapping_mul(P);
                let idx = i as u64 * alpha.len() as u64 + j as u64;
                fwd.entry(h2).or_insert((len, idx));
                next.push(h2);
            }
        }
        layer = next;
    }
    eprintln!("forward {}", fwd.len());
    for &t in &targets {
        let mut found: Vec<String> = Vec::new();
        // backward DFS over suffixes of length 0..=l2
        let mut stack: Vec<(u64, Vec<u8>)> = vec![(t, Vec::new())];
        while let Some((h, suf)) = stack.pop() {
            if let Some(&(len, idx)) = fwd.get(&h) {
                let mut s = prefix.as_bytes().to_vec();
                s.extend(decode(idx, len, &alpha));
                s.extend(&suf);
                if found.len() < 50 {
                    found.push(String::from_utf8_lossy(&s).into_owned());
                }
            }
            if suf.len() == l2 {
                continue;
            }
            let prev = h.wrapping_mul(pinv);
            for &c in &alpha {
                let mut s2 = Vec::with_capacity(suf.len() + 1);
                s2.push(c);
                s2.extend(&suf);
                stack.push((prev ^ c as u64, s2));
            }
        }
        found.sort();
        println!("{t:016x}\t{}", found.join(" | "));
    }
    Ok(())
}
