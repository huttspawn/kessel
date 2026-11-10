//! world3_bytes_scan -- search every decompressed entry of every archive for byte patterns.
//!
//! Decode push 3, world. Proves an object is named nowhere in the client: not by a GOM
//! reference (0xCF + big-endian GUID), a bare little-endian GUID, its FQN as ASCII or
//! UTF-16LE, or its leaf name, in any bucket, prototype, singleton, string table,
//! conversation, script, area or room file.
//!
//! Usage: world3_bytes_scan <assets dir> <patterns.tsv> [hashes_filename.txt]
//!   patterns.tsv: label <TAB> guid|ascii|utf16 <TAB> value  (guid: 16 hex digits; it
//!   expands to the CF+BE and LE forms)
//! Prints one line per hit: label, form, archive, entry hash, dictionary path (only
//! as a label for the reader), offset, and the entry's first four bytes; then hits per
//! label and the number of entries and bytes scanned.

use anyhow::{Context, Result};
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use regex::bytes::RegexBuilder;
use std::collections::{BTreeMap, HashMap};

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    anyhow::ensure!(a.len() >= 3, "usage: <assets> <patterns.tsv> [hashes]");
    let mut dict = HashDictionary::new();
    if let Some(h) = a.get(3) {
        dict.load(h)?;
    }
    let mut pats: Vec<(Vec<u8>, String, String)> = Vec::new();
    for line in std::fs::read_to_string(&a[2])?.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 3 {
            continue;
        }
        let (label, form, value) = (c[0].to_string(), c[1], c[2]);
        match form {
            "guid" => {
                let v = u64::from_str_radix(value, 16).context("guid hex")?;
                let mut cf = vec![0xCFu8];
                cf.extend_from_slice(&v.to_be_bytes());
                pats.push((cf, label.clone(), "cf_be".into()));
                pats.push((v.to_le_bytes().to_vec(), label, "le".into()));
            }
            "ascii" => pats.push((value.as_bytes().to_vec(), label, "ascii".into())),
            "utf16" => {
                let b: Vec<u8> = value.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
                pats.push((b, label, "utf16".into()));
            }
            _ => {}
        }
    }
    let alt: Vec<String> = pats
        .iter()
        .map(|(p, _, _)| p.iter().map(|b| format!("\\x{b:02x}")).collect::<String>())
        .collect();
    let re = RegexBuilder::new(&format!("(?-u:{})", alt.join("|")))
        .unicode(false)
        .size_limit(1 << 28)
        .build()?;
    let by_bytes: HashMap<Vec<u8>, usize> = pats
        .iter()
        .enumerate()
        .map(|(i, (p, _, _))| (p.clone(), i))
        .collect();

    let mut tors: Vec<_> = std::fs::read_dir(&a[1])?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "tor"))
        .collect();
    tors.sort();
    let (mut entries, mut bytes, mut errors) = (0u64, 0u64, 0u64);
    let mut per: BTreeMap<String, u64> = BTreeMap::new();
    for tor in tors {
        let Ok(mut ar) = Archive::open(&tor) else {
            continue;
        };
        let list: Vec<_> = ar.entries()?.cloned().collect();
        let name = tor
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for e in &list {
            let Ok(d) = ar.read_entry(e) else {
                errors += 1;
                continue;
            };
            entries += 1;
            bytes += d.len() as u64;
            for m in re.find_iter(&d) {
                let Some(&i) = by_bytes.get(m.as_bytes()) else {
                    continue;
                };
                let (_, label, form) = &pats[i];
                *per.entry(label.clone()).or_insert(0) += 1;
                let head: String = d.iter().take(4).map(|b| format!("{b:02x}")).collect();
                println!(
                    "HIT\t{label}\t{form}\t{name}\t{:016x}\t{}\t{}\t{head}",
                    e.filename_hash,
                    dict.get(e.filename_hash).map(|s| s.as_str()).unwrap_or("-"),
                    m.start()
                );
            }
        }
    }
    println!("entries {entries} bytes {bytes} read errors {errors}");
    for (l, n) in per {
        println!("label {l} hits {n}");
    }
    Ok(())
}
