//! Word-level meet-in-the-middle FNV-1a 64 preimage search: PREFIX + w1 SEP w2 ... SEP wK, with
//! K = F forward words plus B backward words (B in 1..=2), every word from one word list.
//! Forward states (PREFIX + F words + SEP, F in 0..=2) go in a table; each target is stepped
//! back through the last B words (FNV-1a inverts: h_prev = (h * P^-1) ^ c) and looked up.
//! Used for qstHookId names (gap A3) once short-string MITM (fnv_mitm.rs) ran out.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example fnv_words -- \
//!     <words.txt> <prefix> <sep> <F> <B> <target_i64,target_i64,...>
//! `~` in prefix and sep stands for a space. Words are used as given (uppercase them first).

use anyhow::{Context, Result};
use std::collections::HashMap;

const P: u64 = 0x0000_0100_0000_01b3;
const OFF: u64 = 0xcbf2_9ce4_8422_2325;

fn inv(p: u64) -> u64 {
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

fn unfeed(mut h: u64, b: &[u8], pinv: u64) -> u64 {
    for &c in b.iter().rev() {
        h = h.wrapping_mul(pinv);
        h ^= c as u64;
    }
    h
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let words: Vec<Vec<u8>> = std::fs::read_to_string(a.get(1).context("words")?)?
        .lines()
        .map(|l| l.trim().as_bytes().to_vec())
        .filter(|w| !w.is_empty())
        .collect();
    let prefix = a.get(2).context("prefix")?.replace('~', " ").into_bytes();
    let sep = a.get(3).context("sep")?.replace('~', " ").into_bytes();
    let f: usize = a.get(4).context("F")?.parse()?;
    let b: usize = a.get(5).context("B")?.parse()?;
    let targets: Vec<u64> = a
        .get(6)
        .context("targets")?
        .split(',')
        .map(|t| t.parse::<i64>().map(|v| v as u64))
        .collect::<Result<_, _>>()?;
    let pinv = inv(P);
    // forward: state -> word indices
    let mut fwd: HashMap<u64, Vec<u32>> = HashMap::new();
    let base = feed(OFF, &prefix);
    match f {
        0 => {
            fwd.insert(base, vec![]);
        }
        1 => {
            for (i, w) in words.iter().enumerate() {
                fwd.insert(feed(feed(base, w), &sep), vec![i as u32]);
            }
        }
        _ => {
            for (i, w) in words.iter().enumerate() {
                let s1 = feed(feed(base, w), &sep);
                for (j, w2) in words.iter().enumerate() {
                    fwd.insert(feed(feed(s1, w2), &sep), vec![i as u32, j as u32]);
                }
            }
        }
    }
    eprintln!("words {} forward states {}", words.len(), fwd.len());
    let show = |fi: &[u32], bi: &[usize]| {
        let mut parts: Vec<String> = fi
            .iter()
            .map(|&i| String::from_utf8_lossy(&words[i as usize]).into_owned())
            .collect();
        parts.extend(
            bi.iter()
                .map(|&i| String::from_utf8_lossy(&words[i]).into_owned()),
        );
        format!(
            "{}{}",
            String::from_utf8_lossy(&prefix),
            parts.join(&String::from_utf8_lossy(&sep))
        )
    };
    for &t in &targets {
        for (k, wk) in words.iter().enumerate() {
            let s = unfeed(t, wk, pinv);
            if b == 1 {
                if let Some(fi) = fwd.get(&s) {
                    println!("HIT {} {:?}", t as i64, show(fi, &[k]));
                }
                continue;
            }
            let s = unfeed(s, &sep, pinv);
            for (j, wj) in words.iter().enumerate() {
                let s2 = unfeed(s, wj, pinv);
                if let Some(fi) = fwd.get(&s2) {
                    println!("HIT {} {:?}", t as i64, show(fi, &[j, k]));
                }
            }
        }
    }
    Ok(())
}
