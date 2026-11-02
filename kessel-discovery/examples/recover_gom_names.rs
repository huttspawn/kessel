//! recover_gom_names -- find the blanked names of client.gom types by their hash.
//!
//! Every type record in client.gom keeps FNV-1a 32 of its name in uppercase,
//! though the name string itself is blank (instance records keep theirs and
//! store hash 0). A candidate name is accepted only when its hash equals an
//! unnamed record's hash AND its prefix is used by named records in that
//! record's id run (ids are allocated in runs per system), so chance 32-bit
//! collisions under other prefixes drop out.
//!
//! Two modes:
//!   names  <gom_names.json>  start from the names in that file.
//!   game   <vocab.json>      start from nothing but the client's own plain-text
//!                            names, with a vocabulary built only from game
//!                            text; grow in rounds; score against a truth file.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example recover_gom_names -- \
//!     names <client.gom> <gom_names.json> <out.json>
//!   cargo run --release -p kessel-discovery --example recover_gom_names -- \
//!     game <client.gom> <vocab.json> <truth gom_names.json> <out.json>

use std::collections::{BTreeMap, HashMap, HashSet};

const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;
/// Named records within this many ids of an unnamed one count as its run.
const RUN_WINDOW: u64 = 256;
/// Vocabulary words tried in two-word chains, most frequent first.
const CHAIN_WORDS: usize = 5000;
const MAX_ROUNDS: usize = 12;

fn fnv_upper(state: u32, text: &str) -> u32 {
    text.bytes().fold(state, |h, c| {
        (h ^ u32::from(c.to_ascii_uppercase())).wrapping_mul(FNV_PRIME)
    })
}

struct Record {
    id: u64,
    hash: u32,
    kind: u16,
    plain_name: Option<String>,
}

fn walk(gom: &[u8]) -> anyhow::Result<Vec<Record>> {
    let start = gom
        .windows(4)
        .position(|w| w == b"DBLB")
        .ok_or_else(|| anyhow::anyhow!("no DBLB header"))?
        + 8;
    let u32_at = |o: usize| u32::from_le_bytes([gom[o], gom[o + 1], gom[o + 2], gom[o + 3]]);
    let mut out = Vec::new();
    let mut off = start;
    while off + 24 <= gom.len() {
        let len = u32_at(off) as usize;
        if len == 0 {
            break;
        }
        anyhow::ensure!(
            len >= 24 && off + len <= gom.len(),
            "bad record length at {off:#x}"
        );
        let rec = &gom[off..off + len];
        let id = u64::from(u32_at(off + 8)) | (u64::from(u32_at(off + 12)) << 32);
        let flags = u16::from_le_bytes([rec[16], rec[17]]);
        let name_off = usize::from(u16::from_le_bytes([rec[0x14], rec[0x15]]));
        let plain_name = (0 < name_off && name_off < len)
            .then(|| {
                let end = rec[name_off..]
                    .iter()
                    .position(|&c| c == 0)
                    .map_or(len, |p| name_off + p);
                String::from_utf8_lossy(&rec[name_off..end]).into_owned()
            })
            .filter(|n| !n.is_empty());
        out.push(Record {
            id,
            hash: u32_at(off + 4),
            kind: (flags >> 3) & 0xF,
            plain_name,
        });
        off += len + ((8 - (len & 7)) & 7);
    }
    Ok(out)
}

/// `achConditionBoolParams` -> ("ach", ["Condition", "Bool", "Params"]).
fn split_name(name: &str) -> Option<(String, Vec<String>)> {
    let prefix: String = name
        .chars()
        .take_while(|c| c.is_ascii_lowercase())
        .collect();
    if prefix.is_empty() || prefix.len() == name.len() {
        return None;
    }
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in name[prefix.len()..].chars() {
        if c.is_ascii_uppercase() && !cur.is_empty() && !cur.chars().all(|x| x.is_ascii_uppercase())
        {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    Some((prefix, words))
}

/// Unnamed hashed records grouped by hash, each with its id run's prefixes.
type Targets = HashMap<u32, Vec<(u64, HashSet<String>)>>;

fn targets(records: &[Record], named: &BTreeMap<u64, String>) -> Targets {
    let mut t: Targets = HashMap::new();
    for r in records
        .iter()
        .filter(|r| r.hash != 0 && !named.contains_key(&r.id))
    {
        let run: HashSet<String> = named
            .range(r.id.saturating_sub(RUN_WINDOW)..=r.id.saturating_add(RUN_WINDOW))
            .filter_map(|(_, n)| split_name(n).map(|(p, _)| p))
            .collect();
        t.entry(r.hash).or_default().push((r.id, run));
    }
    t
}

/// prefix + one word, and prefix + two words (from the first CHAIN_WORDS),
/// kept when the hash hits a record whose run uses that prefix -- or, when
/// `global` is set, any unnamed record (round 0, one word only).
fn chains(prefixes: &[String], words: &[String], t: &Targets, global: bool) -> Vec<(u64, String)> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = prefixes.len().div_ceil(threads).max(1);
    let two = &words[..words.len().min(CHAIN_WORDS)];
    std::thread::scope(|s| {
        let handles: Vec<_> = prefixes
            .chunks(chunk)
            .map(|ps| {
                s.spawn(move || {
                    let mut found = Vec::new();
                    let hit = |prefix: &str,
                               h: u32,
                               name: &dyn Fn() -> String,
                               found: &mut Vec<(u64, String)>| {
                        for (id, run) in t.get(&h).into_iter().flatten() {
                            if global || run.contains(prefix) {
                                found.push((*id, name()));
                            }
                        }
                    };
                    for prefix in ps {
                        let p = fnv_upper(FNV_OFFSET, prefix);
                        for (rank, w1) in words.iter().enumerate() {
                            let s1 = fnv_upper(p, w1);
                            hit(prefix, s1, &|| format!("{prefix}{w1}"), &mut found);
                            if global {
                                continue;
                            }
                            if rank < CHAIN_WORDS {
                                for w2 in two {
                                    let s2 = fnv_upper(s1, w2);
                                    if t.contains_key(&s2) {
                                        hit(
                                            prefix,
                                            s2,
                                            &|| format!("{prefix}{w1}{w2}"),
                                            &mut found,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    found
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

/// Each unnamed record's named neighbours with one word swapped, inserted or
/// dropped. Designers name a series this way.
fn siblings(
    records: &[Record],
    named: &BTreeMap<u64, String>,
    words: &[String],
) -> Vec<(u64, String)> {
    let todo: Vec<&Record> = records
        .iter()
        .filter(|r| r.hash != 0 && !named.contains_key(&r.id))
        .collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = todo.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = todo
            .chunks(chunk)
            .map(|rs| {
                s.spawn(move || {
                    let mut found = Vec::new();
                    for r in rs {
                        let lo = r.id.saturating_sub(RUN_WINDOW);
                        for sib in named
                            .range(lo..=r.id.saturating_add(RUN_WINDOW))
                            .map(|(_, n)| n)
                        {
                            let Some((prefix, sw)) = split_name(sib) else {
                                continue;
                            };
                            let mut try_name = |parts: &[&str]| {
                                let name = format!("{prefix}{}", parts.concat());
                                if fnv_upper(FNV_OFFSET, &name) == r.hash {
                                    found.push((r.id, name));
                                }
                            };
                            let base: Vec<&str> = sw.iter().map(String::as_str).collect();
                            for i in 0..base.len() {
                                let mut v = base.clone();
                                v.remove(i);
                                try_name(&v);
                                for w in words {
                                    let mut v = base.clone();
                                    v[i] = w;
                                    try_name(&v);
                                }
                            }
                            for i in 0..=base.len() {
                                for w in words {
                                    let mut v = base.clone();
                                    v.insert(i, w);
                                    try_name(&v);
                                }
                            }
                        }
                    }
                    found
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

/// Accept every record with exactly one distinct candidate; returns how many.
fn accept(
    hits: Vec<(u64, String)>,
    named: &mut BTreeMap<u64, String>,
    how: &mut HashMap<u64, String>,
    round: &str,
) -> usize {
    let mut by_id: BTreeMap<u64, HashSet<String>> = BTreeMap::new();
    for (id, n) in hits {
        by_id.entry(id).or_default().insert(n);
    }
    let mut added = 0;
    for (id, ns) in by_id {
        if ns.len() == 1 && !named.contains_key(&id) {
            if let Some(n) = ns.into_iter().next() {
                named.insert(id, n);
                how.insert(id, round.to_string());
                added += 1;
            }
        }
    }
    added
}

fn vocabulary(named: &BTreeMap<u64, String>, seed: &[String]) -> (Vec<String>, Vec<String>) {
    let mut freq: HashMap<String, usize> = HashMap::new();
    let mut prefixes: HashSet<String> = HashSet::new();
    for n in named.values() {
        if let Some((p, ws)) = split_name(n) {
            prefixes.insert(p);
            for w in ws {
                *freq.entry(w).or_default() += 1_000;
            }
        }
    }
    for (rank, w) in seed.iter().enumerate() {
        *freq.entry(w.clone()).or_default() += seed.len() - rank;
    }
    let mut words: Vec<(String, usize)> = freq.into_iter().collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (
        words.into_iter().map(|(w, _)| w).collect(),
        prefixes.into_iter().collect(),
    )
}

fn read_names(path: &str) -> anyhow::Result<BTreeMap<u64, String>> {
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut named = BTreeMap::new();
    for r in doc["records"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (r["id"].as_str(), r["name"].as_str()) {
            named.insert(id.parse()?, name.to_string());
        }
    }
    Ok(named)
}

fn write(
    out: &str,
    records: &[Record],
    named: &BTreeMap<u64, String>,
    how: &HashMap<u64, String>,
) -> anyhow::Result<()> {
    let kinds: HashMap<u64, u16> = records.iter().map(|r| (r.id, r.kind)).collect();
    let rows: Vec<serde_json::Value> = how
        .keys()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id.to_string(),
                "id_hex": format!("{id:016X}"),
                "kind": kinds.get(id),
                "name": named.get(id),
                "found_in": how.get(id),
            })
        })
        .collect();
    std::fs::write(out, serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("names") if args.len() == 5 => {
            let records = walk(&std::fs::read(&args[2])?)?;
            let mut named = read_names(&args[3])?;
            let (words, _) = vocabulary(&named, &[]);
            let t = targets(&records, &named);
            let prefixes: Vec<String> =
                t.values().flatten().flat_map(|(_, run)| run.iter().cloned()).collect::<HashSet<_>>().into_iter().collect();
            let mut hits = chains(&prefixes, &words, &t, false);
            hits.extend(siblings(&records, &named, &words));
            let mut how = HashMap::new();
            let added = accept(hits, &mut named, &mut how, "names");
            eprintln!("records {}, recovered {added}", records.len());
            write(&args[4], &records, &named, &how)
        }
        Some("game") if args.len() == 6 => {
            let records = walk(&std::fs::read(&args[2])?)?;
            let vocab: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
            let as_list = |k: &str| -> Vec<String> {
                vocab[k].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect()
            };
            let (seed_words, seed_prefixes) = (as_list("words"), as_list("prefixes"));
            let truth = read_names(&args[4])?;

            let mut named: BTreeMap<u64, String> = records
                .iter()
                .filter(|r| r.hash == 0)
                .filter_map(|r| r.plain_name.clone().map(|n| (r.id, n)))
                .collect();
            let mut how: HashMap<u64, String> = named.keys().map(|id| (*id, "client plain text".to_string())).collect();
            eprintln!("start: {} records, {} named in plain text", records.len(), named.len());

            let t = targets(&records, &named);
            let added = accept(chains(&seed_prefixes, &seed_words, &t, true), &mut named, &mut how, "round 0");
            eprintln!("round 0 (prefix + one word, any record): +{added}");
            for round in 1..=MAX_ROUNDS {
                let (words, prefixes) = vocabulary(&named, &seed_words);
                let t = targets(&records, &named);
                let mut hits = chains(&prefixes, &words, &t, false);
                hits.extend(siblings(&records, &named, &words[..words.len().min(CHAIN_WORDS)]));
                let added = accept(hits, &mut named, &mut how, &format!("round {round}"));
                eprintln!("round {round}: +{added} (named {})", named.len());
                if added == 0 {
                    break;
                }
            }

            let (mut agree, mut differ, mut beyond) = (0, 0, 0);
            for (id, n) in &named {
                match truth.get(id) {
                    Some(t) if t.eq_ignore_ascii_case(n) => agree += 1,
                    Some(_) => differ += 1,
                    None => beyond += 1,
                }
            }
            let hashed_truth = truth.keys().filter(|id| records.iter().any(|r| r.id == **id && r.hash != 0)).count();
            eprintln!(
                "named {} of {} records; vs truth: {agree} agree, {differ} differ (same hash, other spelling), {beyond} named that truth lacks; truth hashed names found {agree} of {hashed_truth}",
                named.len(),
                records.len()
            );
            write(&args[5], &records, &named, &how)
        }
        _ => anyhow::bail!(
            "usage: names <client.gom> <gom_names.json> <out.json> | game <client.gom> <vocab.json> <truth.json> <out.json>"
        ),
    }
}
