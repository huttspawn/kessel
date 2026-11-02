//! Lists every numeric run of 40 or more values inside every singleton, as JSON lines, with the
//! path to it: lists, maps and columns of a field across rows of embedded objects. A level curve is such a run, so this is what a curve hunt searches.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example singleton_curves -- <spice.sqlite> > curves.jsonl

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{read_framed_object, GomValue};
use rusqlite::Connection;
use serde_json::json;

fn number(v: &GomValue) -> Option<f64> {
    match v {
        GomValue::U64(n) => Some(*n as f64),
        GomValue::I64(n) => Some(*n as f64),
        GomValue::F32(f) => Some(f64::from(*f)),
        _ => None,
    }
}

/// A column: one numeric field read across rows of embedded objects.
fn columns(fqn: &str, path: &str, rows: &[(Option<f64>, &GomValue)]) {
    let mut ids: Vec<u32> = Vec::new();
    for (_, row) in rows {
        if let GomValue::Embedded(fields) = row {
            for (id, value) in fields {
                let id = *id as u32;
                if number(value).is_some() && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    for id in ids {
        let (mut keys, mut values) = (Vec::new(), Vec::new());
        for (i, (key, row)) in rows.iter().enumerate() {
            if let Some(value) = row.embedded_field(id).and_then(number) {
                keys.push(key.unwrap_or(i as f64));
                values.push(value);
            }
        }
        if values.len() >= 40 {
            println!(
                "{}",
                json!({"fqn": fqn, "path": format!("{path}[].{id:08x}"), "keys": keys, "values": values})
            );
        }
    }
}

fn walk(fqn: &str, path: &str, v: &GomValue) {
    match v {
        GomValue::List(items) => {
            let nums: Vec<f64> = items.iter().filter_map(number).collect();
            if nums.len() >= 40 {
                println!(
                    "{}",
                    json!({"fqn": fqn, "path": path, "keys": null, "values": nums})
                );
            }
            let rows: Vec<(Option<f64>, &GomValue)> = items.iter().map(|r| (None, r)).collect();
            columns(fqn, path, &rows);
            for (i, item) in items.iter().enumerate() {
                walk(fqn, &format!("{path}/{i}"), item);
            }
        }
        GomValue::Map(pairs) => {
            let nums: Vec<(f64, f64)> = pairs
                .iter()
                .filter_map(|(k, v)| number(k).zip(number(v)))
                .collect();
            if nums.len() >= 40 {
                let (keys, values): (Vec<f64>, Vec<f64>) = nums.into_iter().unzip();
                println!(
                    "{}",
                    json!({"fqn": fqn, "path": path, "keys": keys, "values": values})
                );
            }
            let rows: Vec<(Option<f64>, &GomValue)> =
                pairs.iter().map(|(k, r)| (number(k), r)).collect();
            columns(fqn, path, &rows);
            for (i, (k, val)) in pairs.iter().enumerate() {
                let key = number(k).map_or_else(|| i.to_string(), |n| format!("k{n}"));
                walk(fqn, &format!("{path}/{key}"), val);
            }
        }
        GomValue::Embedded(fields) => {
            for (id, val) in fields {
                walk(fqn, &format!("{path}/{:08x}", *id as u32), val);
            }
        }
        _ => {}
    }
}

fn main() -> Result<()> {
    let db = std::env::args().nth(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare("select fqn, payload_b64, header_hex from singletons")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let (mut read, mut failed) = (0u64, 0u64);
    for row in rows {
        let (fqn, b64, header_hex) = row?;
        let header: Vec<u8> = (0..header_hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(header_hex.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect();
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        match read_framed_object(&header, &payload) {
            Ok((v, _)) => {
                read += 1;
                walk(&fqn, "", &v);
            }
            Err(_) => failed += 1,
        }
    }
    eprintln!("singletons read {read}, failed {failed}");
    Ok(())
}
