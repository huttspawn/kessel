//! Every quest hook a conversation node names, one TSV line each, for gap A3 (shared hashed
//! hook ids). Columns: conversation fqn, node id, role (`entry` for a root child's
//! cnvNodeQuestHookIds 6428709a map, `action` for cnvActionQuest b21f4106 + cnvActionHookId
//! 0336685a), quest guid (hex), hook id (signed i64), action flags (0336685b, or empty), and
//! the node's entry quest lists (grants c45852d0 / ends c45852d1 / progress c45852d2) that
//! name the same quest, as g/e/p letters.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_hook_dump -- <spice.sqlite> > hooks.tsv

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;

const EMBEDDED: u8 = 0x09;
const PREAMBLE: usize = 33;
const NODES: u32 = 0x6683_c66d;
const LINK_NODES: u32 = 0x6683_c66e;
const NODE_ID: u32 = 0x24c9_78e5;
const QUEST_GRANTS: u32 = 0xc458_52d0;
const QUEST_ENDS: u32 = 0xc458_52d1;
const QUEST_PROGRESS: u32 = 0xc458_52d2;
const QUEST_HOOK_IDS: u32 = 0x6428_709a;
const ACTION_QUEST: u32 = 0xb21f_4106;
const ACTION_HOOK: u32 = 0x0336_685a;
const ACTION_HOOK_FLAGS: u32 = 0x0336_685b;

fn guid_of(v: &GomValue) -> Option<u64> {
    match v {
        GomValue::U64(n) => Some(*n),
        GomValue::I64(n) => Some(*n as u64),
        _ => None,
    }
}

fn map_keys(node: &GomValue, field: u32) -> Vec<u64> {
    node.embedded_field(field)
        .and_then(GomValue::as_map)
        .into_iter()
        .flatten()
        .filter_map(|(k, _)| guid_of(k))
        .collect()
}

fn main() -> Result<()> {
    let db = std::env::args().nth(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1 order by fqn",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (fqn, b64) = row?;
        let payload = engine.decode(b64)?;
        let mut reader = Reader::new(&payload, PREAMBLE);
        let Ok(root) = reader.read_value(EMBEDDED) else {
            eprintln!("unread {fqn}");
            continue;
        };
        for field in [NODES, LINK_NODES] {
            for (_, node) in root
                .embedded_field(field)
                .and_then(GomValue::as_map)
                .into_iter()
                .flatten()
            {
                let id = node
                    .embedded_field(NODE_ID)
                    .and_then(GomValue::as_i64)
                    .unwrap_or(-1);
                let lists = [
                    ('g', map_keys(node, QUEST_GRANTS)),
                    ('e', map_keys(node, QUEST_ENDS)),
                    ('p', map_keys(node, QUEST_PROGRESS)),
                ];
                let tags = |q: u64| -> String {
                    lists
                        .iter()
                        .filter(|(_, l)| l.contains(&q))
                        .map(|(c, _)| *c)
                        .collect()
                };
                for (k, v) in node
                    .embedded_field(QUEST_HOOK_IDS)
                    .and_then(GomValue::as_map)
                    .into_iter()
                    .flatten()
                {
                    let Some(q) = guid_of(k) else { continue };
                    for h in v
                        .as_list()
                        .into_iter()
                        .flatten()
                        .filter_map(GomValue::as_i64)
                    {
                        println!("{fqn}\t{id}\tentry\t{q:016X}\t{h}\t\t{}", tags(q));
                    }
                }
                if let Some(q) = node.embedded_field(ACTION_QUEST).and_then(guid_of) {
                    let h = node.embedded_field(ACTION_HOOK).and_then(GomValue::as_i64);
                    let f = node
                        .embedded_field(ACTION_HOOK_FLAGS)
                        .and_then(GomValue::as_i64);
                    println!(
                        "{fqn}\t{id}\taction\t{q:016X}\t{}\t{}\t{}",
                        h.map(|x| x.to_string()).unwrap_or_default(),
                        f.map(|x| x.to_string()).unwrap_or_default(),
                        tags(q)
                    );
                }
            }
        }
    }
    Ok(())
}
