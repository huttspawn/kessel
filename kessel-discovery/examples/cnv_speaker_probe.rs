//! Probe: where does a conversation node name its speaker?
//!
//! For each dialog node of one conversation, prints the node id, its line id,
//! the player-choice flag (259e02f8), and every top-level field whose value is
//! an E0-prefixed GUID, resolved to an FQN when it names an object. Also prints
//! the line's text so a human can judge who speaks.
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example cnv_speaker_probe -- \
//!     <spice.sqlite> <cnv fqn>

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;
use std::collections::HashMap;

const NODES_FIELD: u32 = 0x6683_c66d;
const NODE_ID_FIELD: u32 = 0x24c9_78e5;
const PLAYER_CHOICE_FIELD: u32 = 0x259e_02f8;
const TEXT_MAP_FIELD: u32 = 0xa787_ee87;
const TEXT_REF_FIELD: u32 = 0x5ce8_7488;

fn e0(v: &GomValue) -> Option<u64> {
    let n = match v {
        GomValue::U64(n) => *n,
        GomValue::I64(n) => *n as u64,
        _ => return None,
    };
    (n >> 56 == 0xE0).then_some(n)
}

/// Every (field path, GUID) under `v`, paths as dotted field ids.
fn guids_in(v: &GomValue, path: &str, out: &mut Vec<(String, u64)>) {
    if let Some(g) = e0(v) {
        out.push((path.to_string(), g));
        return;
    }
    match v {
        GomValue::Embedded(fields) => {
            for (id, val) in fields {
                guids_in(val, &format!("{path}.{:08x}", *id as u32), out);
            }
        }
        GomValue::List(items) => {
            for item in items {
                guids_in(item, &format!("{path}[]"), out);
            }
        }
        GomValue::Map(pairs) => {
            for (k, val) in pairs {
                guids_in(k, &format!("{path}{{k}}"), out);
                guids_in(val, &format!("{path}{{v}}"), out);
            }
        }
        _ => {}
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).context("usage: <spice.sqlite> <cnv fqn>")?;
    let fqn = args.get(2).context("usage: <spice.sqlite> <cnv fqn>")?;
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let b64: String = conn.query_row(
        "SELECT json_extract(json, '$.payload_b64') FROM objects \
         WHERE fqn = ?1 AND kind = 'Conversation' AND is_canonical = 1",
        [fqn],
        |r| r.get(0),
    )?;
    let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
    let mut reader = Reader::new(&payload, 33);
    let root = reader.read_value(0x09)?;

    let mut text: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT line_group, text FROM conversation_lines WHERE cnv_fqn = ?1 AND text <> ''",
        )?;
        for row in stmt.query_map([fqn], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, t) = row?;
            text.insert(id, t);
        }
    }
    let mut name = |g: u64| -> String {
        conn.query_row(
            "SELECT kind || ' ' || fqn FROM objects WHERE guid = ?1 LIMIT 1",
            [format!("{g:016X}")],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "-".to_string())
    };

    let nodes = root
        .embedded_field(NODES_FIELD)
        .and_then(GomValue::as_map)
        .context("no node map")?;
    for (_, node) in nodes {
        let id = node
            .embedded_field(NODE_ID_FIELD)
            .and_then(GomValue::as_i64);
        let line = node
            .embedded_field(TEXT_MAP_FIELD)
            .and_then(GomValue::as_map)
            .into_iter()
            .flatten()
            .find_map(|(_, s)| s.embedded_field(TEXT_REF_FIELD)?.as_i64());
        let player = matches!(
            node.embedded_field(PLAYER_CHOICE_FIELD),
            Some(GomValue::Bool(true))
        );
        let mut gs = Vec::new();
        guids_in(node, "", &mut gs);
        let t = line
            .and_then(|l| text.get(&(l as u32 as i64)))
            .map(|s| s.chars().take(70).collect::<String>())
            .unwrap_or_default();
        println!(
            "node {:?} line {:?} player={player} :: {t}",
            id,
            line.map(|l| l as u32)
        );
        for (path, g) in gs {
            println!("    {path} = {g:016X} {}", name(g));
        }
    }
    Ok(())
}
