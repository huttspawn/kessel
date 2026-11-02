//! How much of every object kind and singleton does `gom_reader` read? Per kind: objects read,
//! failed, objects with non-zero bytes after the field stream, and the commonest failure.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example reader_census -- <spice.sqlite>

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::read_framed_object;
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
struct Tally {
    read: u64,
    failed: u64,
    trailing: u64,
    errors: HashMap<String, u64>,
}

fn record(t: &mut Tally, fqn: &str, header_hex: &str, payload: &[u8]) {
    let header = hex::decode(header_hex).unwrap_or_default();
    match read_framed_object(&header, payload) {
        Ok((_, end)) => {
            t.read += 1;
            if payload[end..].iter().any(|b| *b != 0) {
                t.trailing += 1;
            }
        }
        Err(e) => {
            eprintln!("{fqn}: {e} (header {header_hex}, {} bytes)", payload.len());
            t.failed += 1;
            *t.errors.entry(e.to_string()).or_default() += 1;
        }
    }
}

fn main() -> Result<()> {
    let db = std::env::args().nth(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut kinds: BTreeMap<String, Tally> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "select kind, fqn, json_extract(json,'$.header_hex'), json_extract(json,'$.payload_b64') \
         from objects where is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (kind, fqn, header, b64) = row?;
        let (Some(header), Some(b64)) = (header, b64) else {
            continue;
        };
        record(
            kinds.entry(kind).or_default(),
            &fqn,
            &header,
            &engine.decode(b64)?,
        );
    }
    let mut stmt = conn.prepare("select fqn, header_hex, payload_b64 from singletons")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (fqn, header, b64) = row?;
        let tally = kinds.entry("(singleton)".to_string()).or_default();
        record(tally, &fqn, &header, &engine.decode(b64)?);
    }
    let (mut read, mut failed) = (0, 0);
    for (kind, t) in &kinds {
        read += t.read;
        failed += t.failed;
        let top = t.errors.iter().max_by_key(|(_, n)| **n);
        let top = top.map_or(String::new(), |(e, n)| format!("{n} x {e}"));
        println!(
            "{kind:<22} read {:>7} failed {:>7} trailing {:>7}  {top}",
            t.read, t.failed, t.trailing
        );
    }
    println!("total read {read} failed {failed}");
    Ok(())
}
