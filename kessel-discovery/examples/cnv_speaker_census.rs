//! Census: does the node speaker field (ada8873c) / addressee field (ada8873f)
//! plus the player-choice flag (259e02f8) cover every line node, and how often
//! does the shipped byte-scan speaker in conversation_dialogue disagree?
//!
//! Usage:
//!   cargo run --release -p kessel-discovery --example cnv_speaker_census -- <spice.sqlite> [fqn LIKE]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap};

const NODES_FIELD: u32 = 0x6683_c66d;
const PLAYER_CHOICE_FIELD: u32 = 0x259e_02f8;
const TEXT_MAP_FIELD: u32 = 0xa787_ee87;
const TEXT_REF_FIELD: u32 = 0x5ce8_7488;
const SPEAKER: u32 = 0xada8_873c;
const ADDRESSEE: u32 = 0xada8_873f;

fn guid(v: Option<&GomValue>) -> Option<String> {
    let n = match v? {
        GomValue::U64(n) => *n,
        GomValue::I64(n) => *n as u64,
        _ => return None,
    };
    Some(format!("{n:016X}"))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).context("usage: <spice.sqlite> [fqn LIKE]")?;
    let like = args.get(2).cloned().unwrap_or_else(|| "%".into());
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let npc: HashMap<String, String> = conn
        .prepare("SELECT guid, fqn FROM objects WHERE kind='Npc' AND is_canonical=1")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut convs = conn.prepare(
        "SELECT fqn, json_extract(json,'$.payload_b64') FROM objects \
         WHERE kind='Conversation' AND is_canonical=1 AND fqn LIKE ?1 \
         AND json_extract(json,'$.payload_b64') IS NOT NULL",
    )?;
    let mut shipped = conn.prepare(
        "SELECT line_id, speaker_role, speaker_npc_fqn FROM conversation_dialogue WHERE cnv_fqn=?1",
    )?;
    let mut shape: BTreeMap<String, u64> = BTreeMap::new();
    let (mut convs_n, mut undecoded, mut lines) = (0u64, 0u64, 0u64);
    let (mut agree, mut role_wrong, mut npc_wrong, mut unresolved_speaker) =
        (0u64, 0u64, 0u64, 0u64);
    let mut examples = Vec::new();
    let mut other: BTreeMap<String, u64> = BTreeMap::new();
    let mut samples: Vec<String> = Vec::new();
    let mut absent_live = 0u64;
    let rows: Vec<(String, String)> = convs
        .query_map([&like], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (fqn, b64) in rows {
        convs_n += 1;
        let payload = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let mut reader = Reader::new(&payload, 33);
        let Ok(root) = reader.read_value(0x09) else {
            undecoded += 1;
            continue;
        };
        if reader.pos() != payload.len() {
            undecoded += 1;
            continue;
        }
        let have: HashMap<u32, (String, Option<String>)> = shipped
            .query_map([&fqn], |r| {
                Ok((r.get::<_, u32>(0)?, (r.get(1)?, r.get(2)?)))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let Some(nodes) = root.embedded_field(NODES_FIELD).and_then(GomValue::as_map) else {
            continue;
        };
        for (_, node) in nodes {
            let Some(line) = node
                .embedded_field(TEXT_MAP_FIELD)
                .and_then(GomValue::as_map)
                .into_iter()
                .flatten()
                .find_map(|(_, s)| s.embedded_field(TEXT_REF_FIELD)?.as_i64())
                .map(|l| l as u32)
            else {
                continue;
            };
            lines += 1;
            let player = matches!(
                node.embedded_field(PLAYER_CHOICE_FIELD),
                Some(GomValue::Bool(true))
            );
            let sp = guid(node.embedded_field(SPEAKER));
            let ad = guid(node.embedded_field(ADDRESSEE));
            let key = format!(
                "player={player} speaker={} addressee={} speaker_is_npc={}",
                sp.is_some(),
                ad.is_some(),
                sp.as_ref().map(|g| npc.contains_key(g)).unwrap_or(false)
            );
            *shape.entry(key).or_default() += 1;
            let truth_role = if player { "player" } else { "npc" };
            let truth_npc = if player {
                None
            } else {
                sp.as_ref().and_then(|g| npc.get(g)).cloned()
            };
            if !player && truth_npc.is_none() {
                unresolved_speaker += 1;
                if let Some(g) = &sp {
                    *other.entry(g.clone()).or_default() += 1;
                    if !fqn.starts_with("cnv.test")
                        && !npc.contains_key(g)
                        && conn
                            .query_row("SELECT 1 FROM objects WHERE guid=?1", [g], |_| Ok(()))
                            .is_err()
                    {
                        absent_live += 1;
                    }
                    if samples.len() < 25
                        && !fqn.starts_with("cnv.test")
                        && conn
                            .query_row("SELECT 1 FROM objects WHERE guid=?1", [g], |_| Ok(()))
                            .is_err()
                    {
                        let t: String = conn.query_row("SELECT text FROM conversation_lines WHERE cnv_fqn=?1 AND line_group=?2", rusqlite::params![fqn, line], |r| r.get(0)).unwrap_or_default();
                        samples.push(format!(
                            "{fqn} {g} ad={:?}: {}",
                            ad,
                            t.chars().take(80).collect::<String>()
                        ));
                    }
                }
            }
            if let Some((role, sfqn)) = have.get(&line) {
                if role != truth_role {
                    role_wrong += 1;
                } else if !player && sfqn != &truth_npc {
                    npc_wrong += 1;
                    if examples.len() < 8 {
                        examples.push(format!("{fqn} {line}: shipped {sfqn:?} node {truth_npc:?}"));
                    }
                } else {
                    agree += 1;
                }
            }
        }
    }
    println!("conversations {convs_n} undecoded {undecoded} line nodes {lines}");
    for (k, v) in &shape {
        println!("  {v:>8}  {k}");
    }
    println!("vs shipped: agree {agree} role_wrong {role_wrong} npc_wrong {npc_wrong}; npc lines with no Npc speaker {unresolved_speaker}");
    for e in examples {
        println!("  e.g. {e}");
    }
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    for (g, n) in &other {
        let k: String = conn
            .query_row(
                "SELECT kind || ' ' || fqn FROM objects WHERE guid=?1 LIMIT 1",
                [g],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| format!("absent {g}"));
        let short = if k.starts_with("absent") {
            "absent".to_string()
        } else {
            k.split('.').take(2).collect::<Vec<_>>().join(".")
        };
        *kinds.entry(short).or_default() += n;
        if other.len() <= 20 {
            println!("  non-npc speaker {g} x{n}: {k}");
        }
    }
    println!("absent speakers outside cnv.test: {absent_live}");
    for x in &samples {
        println!("  sample {x}");
    }
    for (k, n) in kinds.iter() {
        println!("  speaker-kind {n:>7} {k}");
    }
    Ok(())
}
