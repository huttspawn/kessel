//! Every conversation node as one TSV row joining node number to its STB line (gap B9).
//!
//! Columns: cnv_fqn, link (0/1), node number (24c978e5), line id (the low 32 bits of the
//! 5ce87488 value found anywhere inside the node's text field a787ee87, empty when absent),
//! is_pc (259e02f8), alien VO node (7332dbd6, empty when absent), alien VO conversation
//! (7332dbd7), speaker field (ada8873c, empty when absent), and the comma-joined low-32 ids of
//! every non-empty node field (for correlating a flag bit with a node field).
//!
//! The node number is the number in the en-us VO bank's wem names
//! (bnk2/<cnv>.acb holds <cnv>_<node>[_<variant>]_<m|f>.wem), so this table is the join from a
//! VO file to its STB entry and its flags.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_node_lines -- <spice.sqlite> > lines.tsv

use anyhow::{Context, Result};
use base64::Engine;
use kessel::gom_reader::{GomValue, Reader};
use rusqlite::Connection;

const EMBEDDED: u8 = 0x09;
const PREAMBLE: usize = 33;
const NODES: u32 = 0x6683_c66d;
const LINK_NODES: u32 = 0x6683_c66e;
const NODE_ID: u32 = 0x24c9_78e5;
const TEXT: u32 = 0xa787_ee87;
const LINE_ID: u32 = 0x5ce8_7488;
const IS_PC: u32 = 0x259e_02f8;
const ALIEN_NODE: u32 = 0x7332_dbd6;
const ALIEN_CNV: u32 = 0x7332_dbd7;
const SPEAKER: u32 = 0xada8_873c;

fn find_line(v: &GomValue) -> Option<i64> {
    match v {
        GomValue::Embedded(fields) => {
            for (id, val) in fields {
                if *id as u32 == LINE_ID {
                    if let Some(n) = val.as_i64() {
                        return Some(n & 0xffff_ffff);
                    }
                }
                if let Some(n) = find_line(val) {
                    return Some(n);
                }
            }
            None
        }
        GomValue::List(l) => l.iter().find_map(find_line),
        GomValue::Map(m) => m.iter().find_map(|(_, x)| find_line(x)),
        _ => None,
    }
}

fn is_empty(v: &GomValue) -> bool {
    match v {
        GomValue::Null => true,
        GomValue::List(l) => l.is_empty(),
        GomValue::Map(m) => m.is_empty(),
        GomValue::I64(n) => *n == -1,
        GomValue::F32(f) => *f == -1.0,
        _ => false,
    }
}

fn scalar(v: Option<&GomValue>) -> String {
    match v {
        Some(GomValue::Str(s)) => s.clone(),
        Some(GomValue::Bool(b)) => u8::from(*b).to_string(),
        Some(GomValue::U64(n)) => format!("{n:016X}"),
        Some(GomValue::I64(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).context("usage: <spice.sqlite>")?;
    let conn = Connection::open(db)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1 order by fqn",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut unread = 0u64;
    for row in rows {
        let (fqn, b64) = row?;
        let payload = engine.decode(b64)?;
        let mut reader = Reader::new(&payload, PREAMBLE);
        let Ok(root) = reader.read_value(EMBEDDED) else {
            unread += 1;
            continue;
        };
        for field in [NODES, LINK_NODES] {
            for (_, node) in root
                .embedded_field(field)
                .and_then(GomValue::as_map)
                .into_iter()
                .flatten()
            {
                let num = node.embedded_field(NODE_ID).and_then(GomValue::as_i64);
                let line = node.embedded_field(TEXT).and_then(find_line);
                let present: Vec<String> = match node {
                    GomValue::Embedded(fields) => fields
                        .iter()
                        .filter(|(_, v)| !is_empty(v))
                        .map(|(id, _)| format!("{:08x}", *id as u32))
                        .collect(),
                    _ => Vec::new(),
                };
                println!(
                    "{fqn}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    u8::from(field == LINK_NODES),
                    num.map(|n| n.to_string()).unwrap_or_default(),
                    line.map(|n| n.to_string()).unwrap_or_default(),
                    scalar(node.embedded_field(IS_PC)),
                    scalar(node.embedded_field(ALIEN_NODE)),
                    scalar(node.embedded_field(ALIEN_CNV)),
                    scalar(node.embedded_field(SPEAKER)),
                    present.join(","),
                );
            }
        }
    }
    eprintln!("unread conversations: {unread}");
    Ok(())
}
