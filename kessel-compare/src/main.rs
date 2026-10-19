//! kessel-compare: the SWTOR patch changelog.
//!
//! Diffs two kessel spice databases (old patch vs new) and reports what changed
//! per object kind -- new quests, retuned abilities/talents, added items, etc.
//!
//! Why a custom tool instead of `smugglr diff`: every kessel table's primary key
//! embeds `game_id = sha256(fqn:guid)`, and the GUID *shifts on every patch* -- so a
//! PK-keyed diff would report every object as removed+added. kessel carries the
//! cross-patch primitives for exactly this: `stable_id = sha256(fqn)` (survives a
//! patch) and `payload_hash = sha256(payload)` (the content-change signal). The
//! diff is keyed by `stable_id` and compared on `payload_hash`:
//!   - in new, not old        -> ADDED
//!   - in old, not new        -> REMOVED
//!   - same stable_id, payload differs -> CHANGED
//!
//! `classify` is local rather than smugglr-core's `classify_diff`: spice has no
//! timestamps and the hash covers the whole payload, so only the set partition
//! applies, and owning it keeps smugglr's API changes out of this build.
//!
//! Usage: kessel-compare <old.sqlite> <new.sqlite>

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

struct ObjInfo {
    kind: String,
    fqn: String,
}

/// stable_id -> payload_hash for every canonical object.
type Hashes = HashMap<String, String>;

/// Stable ids partitioned by how they differ between two builds.
#[derive(Debug, Default, PartialEq)]
struct Diff {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
}

/// Partition stable ids: in `new` only, in `old` only, or in both with a
/// different payload hash. A hash match means the payload is unchanged.
fn classify(new: &Hashes, old: &Hashes) -> Diff {
    let mut diff = Diff::default();
    for (sid, new_hash) in new {
        match old.get(sid) {
            None => diff.added.push(sid.clone()),
            Some(old_hash) if old_hash != new_hash => diff.changed.push(sid.clone()),
            Some(_) => {}
        }
    }
    for sid in old.keys() {
        if !new.contains_key(sid) {
            diff.removed.push(sid.clone());
        }
    }
    diff
}

/// Load every canonical object as (stable_id -> payload_hash) for the diff,
/// plus (stable_id -> kind/fqn) for reporting.
fn load(path: &str) -> Result<(Hashes, HashMap<String, ObjInfo>)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {path}"))?;
    let mut stmt = conn
        .prepare("SELECT stable_id, kind, fqn, payload_hash FROM objects WHERE is_canonical = 1")?;
    let mut meta = HashMap::new();
    let mut info = HashMap::new();
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (stable_id, kind, fqn, payload_hash) = row?;
        meta.insert(stable_id.clone(), payload_hash);
        info.insert(stable_id, ObjInfo { kind, fqn });
    }
    Ok((meta, info))
}

#[derive(Default)]
struct KindDelta {
    added: Vec<String>,
    changed: Vec<String>,
    removed: Vec<String>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: kessel-compare <old.sqlite> <new.sqlite>");
        std::process::exit(2);
    }
    let (old_meta, old_info) = load(&args[1])?;
    let (new_meta, new_info) = load(&args[2])?;

    let diff = classify(&new_meta, &old_meta);

    let mut by_kind: BTreeMap<String, KindDelta> = BTreeMap::new();
    for sid in &diff.added {
        if let Some(o) = new_info.get(sid) {
            by_kind
                .entry(o.kind.clone())
                .or_default()
                .added
                .push(o.fqn.clone());
        }
    }
    for sid in &diff.changed {
        if let Some(o) = new_info.get(sid) {
            by_kind
                .entry(o.kind.clone())
                .or_default()
                .changed
                .push(o.fqn.clone());
        }
    }
    for sid in &diff.removed {
        if let Some(o) = old_info.get(sid) {
            by_kind
                .entry(o.kind.clone())
                .or_default()
                .removed
                .push(o.fqn.clone());
        }
    }

    println!("kessel patch diff: {} -> {}", args[1], args[2]);
    println!(
        "objects: {} old, {} new  |  +{} added  ~{} changed  -{} removed\n",
        old_meta.len(),
        new_meta.len(),
        diff.added.len(),
        diff.changed.len(),
        diff.removed.len(),
    );

    for (kind, d) in &by_kind {
        if d.added.is_empty() && d.changed.is_empty() && d.removed.is_empty() {
            continue;
        }
        println!(
            "== {kind} ==  +{} added  ~{} changed  -{} removed",
            d.added.len(),
            d.changed.len(),
            d.removed.len()
        );
        print_sample("added", &d.added);
        print_sample("changed", &d.changed);
        print_sample("removed", &d.removed);
        println!();
    }
    Ok(())
}

/// Print up to 15 fqns for a bucket (full lists get long on a big patch).
fn print_sample(label: &str, fqns: &[String]) {
    if fqns.is_empty() {
        return;
    }
    let mut sorted = fqns.to_vec();
    sorted.sort();
    let shown = sorted.len().min(15);
    println!("   {label} ({}):", fqns.len());
    for f in &sorted[..shown] {
        println!("     {f}");
    }
    if fqns.len() > shown {
        println!("     ... and {} more", fqns.len() - shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashes(pairs: &[(&str, &str)]) -> Hashes {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn classify_partitions_added_removed_changed() {
        let old = hashes(&[("same", "h1"), ("edited", "h2"), ("gone", "h3")]);
        let new = hashes(&[("same", "h1"), ("edited", "h9"), ("fresh", "h4")]);

        let diff = classify(&new, &old);

        assert_eq!(diff.added, vec!["fresh".to_string()]);
        assert_eq!(diff.removed, vec!["gone".to_string()]);
        assert_eq!(diff.changed, vec!["edited".to_string()]);
    }

    #[test]
    fn classify_identical_builds_is_empty() {
        let both = hashes(&[("a", "h1"), ("b", "h2")]);
        assert_eq!(classify(&both, &both), Diff::default());
    }
}
