//! ryloth_dictless -- FR-KESSEL-024 proof on the one archive 8.0 PTS added
//! (swtor_test_main_8_0_areas_1.tor, Ryloth and Children of Nul).
//!
//! Prediction, written before the run: every entry decompresses, every entry is
//! identified by its content with no dictionary, and the game objects inside its
//! buckets come out whole with their FQNs. Entries the dictionary does not list
//! are found the same way as the ones it does.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example ryloth_dictless -- \
//!     <archive.tor> <hashes_filename.txt>

use kessel::dds;
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use kessel::pbuk;
use std::collections::BTreeMap;

fn class_of(data: &[u8]) -> &'static str {
    if data.len() >= 4 && &data[..4] == b"PROT" {
        "PROT"
    } else if pbuk::is_pbuk(data) {
        "PBUK"
    } else if pbuk::is_dblb(data) {
        "DBLB"
    } else if data.len() >= 4 && &data[..4] == b"SCPT" {
        "SCPT"
    } else if dds::is_dds(data) {
        "DDS"
    } else if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
        "UTF16_XML"
    } else if data.len() >= 4 && &data[..4] == b"GAWB" {
        "GAWB"
    } else if data.len() >= 4 && &data[..4] == b"JAWB" {
        "JAWB"
    } else if !data.is_empty() && data[0] == 0x01 {
        "STB"
    } else {
        "OTHER"
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (tor, dict_path) = match (args.get(1), args.get(2)) {
        (Some(t), Some(d)) => (t, d),
        _ => anyhow::bail!("usage: <archive.tor> <hashes_filename.txt>"),
    };
    let mut dict = HashDictionary::new();
    dict.load(dict_path)?;

    let mut archive = Archive::open(std::path::Path::new(tor))?;
    let entries: Vec<_> = archive.entries()?.cloned().collect();

    // (in_dict, class) -> count
    let mut by_class: BTreeMap<(bool, &'static str), usize> = BTreeMap::new();
    let mut read_errors = 0usize;
    let mut objects_in_dict = 0usize;
    let mut objects_not_in_dict = 0usize;
    let mut object_errors = 0usize;
    let mut prefixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut ryloth_fqns: Vec<String> = Vec::new();
    let mut other_first_bytes: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &entries {
        let in_dict = dict.get(entry.filename_hash).is_some();
        let data = match archive.read_entry(entry) {
            Ok(d) => d,
            Err(_) => {
                read_errors += 1;
                continue;
            }
        };
        let class = class_of(&data);
        *by_class.entry((in_dict, class)).or_insert(0) += 1;
        if class == "OTHER" {
            let head: String = data.iter().take(4).map(|b| format!("{b:02x}")).collect();
            // The dictionary is a cross-check only: what does it say these are?
            let ext = dict
                .get(entry.filename_hash)
                .map(|p| {
                    let name = p.rsplit('/').next().unwrap_or(p);
                    let dir = p.split('/').take(4).collect::<Vec<_>>().join("/");
                    format!("{dir} *.{}", name.rsplit('.').next().unwrap_or(""))
                })
                .unwrap_or_else(|| "unlisted".to_string());
            *other_first_bytes
                .entry(format!("{head} {ext}"))
                .or_insert(0) += 1;
        }
        if class == "PBUK" || class == "DBLB" {
            let parsed = if class == "PBUK" {
                pbuk::parse(&data)
            } else {
                pbuk::parse_dblb_direct(&data)
            };
            match parsed {
                Ok(objs) => {
                    for o in objs {
                        if in_dict {
                            objects_in_dict += 1;
                        } else {
                            objects_not_in_dict += 1;
                        }
                        let p = o.fqn.split('.').next().unwrap_or("").to_string();
                        *prefixes.entry(p).or_insert(0) += 1;
                        if o.fqn.contains("ryloth") && ryloth_fqns.len() < 12 {
                            ryloth_fqns.push(o.fqn.clone());
                        }
                    }
                }
                Err(_) => object_errors += 1,
            }
        }
    }

    // Path derivation with no dictionary: every printable string inside the
    // archive's area files, tried as a path (as written, and under /resources),
    // hashed the way the archive keys entries, and matched against the archive.
    let all: std::collections::HashSet<u64> = entries.iter().map(|e| e.filename_hash).collect();
    let mut derived: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for entry in &entries {
        let Ok(data) = archive.read_entry(entry) else {
            continue;
        };
        if class_of(&data) != "OTHER" {
            continue;
        }
        let mut cur = Vec::new();
        for &b in data.iter().chain(std::iter::once(&0u8)) {
            if (0x20..0x7f).contains(&b) {
                cur.push(b);
                continue;
            }
            if cur.len() >= 5 {
                let s = String::from_utf8_lossy(&cur)
                    .replace('\\', "/")
                    .to_lowercase();
                for cand in [
                    s.clone(),
                    format!("/resources{s}"),
                    format!("/resources/{s}"),
                ] {
                    let (ph, sh) = kessel::hash::hashlittle2(cand.as_bytes(), 0, 0);
                    let h = kessel::hash::combine_hash(ph, sh);
                    if all.contains(&h) {
                        derived.insert(h);
                    }
                }
            }
            cur.clear();
        }
    }
    // Conventions, seeded only from what the area data itself named: the area
    // folders of the derived room paths, and every short name in the area data.
    let hash_of = |s: &str| {
        let (ph, sh) = kessel::hash::hashlittle2(s.as_bytes(), 0, 0);
        kessel::hash::combine_hash(ph, sh)
    };
    let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in &entries {
        let Ok(data) = archive.read_entry(entry) else {
            continue;
        };
        if class_of(&data) != "OTHER" {
            continue;
        }
        let mut cur = Vec::new();
        for &b in data.iter().chain(std::iter::once(&0u8)) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                cur.push(b.to_ascii_lowercase());
                continue;
            }
            // Display names store spaces where file names use underscores.
            if b == b' ' && !cur.is_empty() {
                names.insert(String::from_utf8_lossy(&cur).into_owned());
                cur.push(b'_');
                continue;
            }
            if cur.len() >= 3 {
                names.insert(String::from_utf8_lossy(&cur).into_owned());
            }
            cur.clear();
        }
    }
    let derived_paths: Vec<String> = derived
        .iter()
        .filter_map(|h| dict.get(*h).cloned())
        .collect();
    let dirs: std::collections::HashSet<String> = derived_paths
        .iter()
        .filter_map(|p| p.rsplit_once('/').map(|(d, _)| d.to_string()))
        .collect();
    for d in &dirs {
        for fixed in ["area.dat", "mapnotes.not"] {
            let h = hash_of(&format!("{d}/{fixed}"));
            if all.contains(&h) {
                derived.insert(h);
            }
        }
        for n in &names {
            let h = hash_of(&format!("{d}/{n}_r.dds"));
            if all.contains(&h) {
                derived.insert(h);
            }
            for x in 0..24 {
                for y in 0..24 {
                    let h = hash_of(&format!("{d}/minimaps/{n}_{x:02}_{y:02}_r.dds"));
                    if all.contains(&h) {
                        derived.insert(h);
                    }
                }
            }
        }
    }
    let by_kind = |pred: &dyn Fn(&str) -> bool| {
        entries
            .iter()
            .filter(|e| dict.get(e.filename_hash).map(|p| pred(p)).unwrap_or(false))
            .map(|e| e.filename_hash)
            .collect::<std::collections::HashSet<u64>>()
    };
    for (label, set) in [
        ("area/room .dat", by_kind(&|p: &str| p.ends_with(".dat"))),
        ("map notes .not", by_kind(&|p: &str| p.ends_with(".not"))),
        ("textures .dds", by_kind(&|p: &str| p.ends_with(".dds"))),
    ] {
        let hit = set.iter().filter(|h| derived.contains(h)).count();
        println!(
            "derived from area data, no dictionary: {label}: {hit} of {}",
            set.len()
        );
        let mut missed: Vec<&String> = set
            .iter()
            .filter(|h| !derived.contains(h))
            .filter_map(|h| dict.get(*h))
            .collect();
        missed.sort();
        for p in missed.iter().step_by((missed.len() / 8).max(1)).take(8) {
            println!("    not derived: {p}");
        }
    }
    let mut seen: Vec<&String> = derived.iter().filter_map(|h| dict.get(*h)).collect();
    seen.sort();
    for p in seen.iter().take(3) {
        println!("    derived e.g.: {p}");
    }
    println!("archive: {tor}");
    println!("entries: {}  read errors: {read_errors}", entries.len());
    let listed = entries
        .iter()
        .filter(|e| dict.get(e.filename_hash).is_some())
        .count();
    println!(
        "listed in the dictionary: {listed}  not listed: {}",
        entries.len() - listed
    );
    println!("by content class (in_dict, class): count");
    for ((d, c), n) in &by_class {
        println!("  {:<5} {c:<10} {n}", if *d { "dict" } else { "none" });
    }
    println!("OTHER by first 4 bytes: {other_first_bytes:?}");
    println!(
        "game objects from buckets: {} (bucket in dict {objects_in_dict}, bucket not in dict {objects_not_in_dict}), bucket parse errors {object_errors}",
        objects_in_dict + objects_not_in_dict
    );
    println!("objects by prefix: {prefixes:?}");
    println!("sample Ryloth FQNs: {ryloth_fqns:#?}");
    Ok(())
}
