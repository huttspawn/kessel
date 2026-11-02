//! Checks the conversation graph across a database: nodes, dangling children, decisions and
//! alignment, plus one conversation's decisions with their text.
//!
//! Run:
//!   cargo run --release -p kessel-discovery --example cnv_options_check -- <spice.sqlite> [fqn]

use anyhow::{Context, Result};
use base64::Engine;
use kessel::schema::conversation_graph::ConversationGraph;
use rusqlite::Connection;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let conn = Connection::open(args.get(1).context("usage: <spice.sqlite> [fqn]")?)?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut stmt = conn.prepare(
        "select fqn, json_extract(json,'$.payload_b64') from objects \
         where kind='Conversation' and is_canonical=1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let (mut read, mut failed, mut nodes, mut dangling, mut dangling_cnv) = (0u64, 0, 0, 0u64, 0);
    let (mut decisions, mut multi, mut choices, mut light, mut dark, mut targeted) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut unreachable = 0u64;
    for row in rows {
        let (fqn, b64) = row?;
        let Some(graph) = ConversationGraph::read(&engine.decode(b64)?) else {
            failed += 1;
            eprintln!("no graph: {fqn}");
            continue;
        };
        read += 1;
        nodes += graph.nodes.len() as u64;
        let missing = graph
            .nodes
            .values()
            .flat_map(|n| n.children.iter().chain(n.link_target.iter()))
            .chain(graph.roots.iter())
            .filter(|id| !graph.nodes.contains_key(id))
            .count() as u64;
        dangling += missing;
        dangling_cnv += u64::from(missing > 0);
        let mut seen = std::collections::HashSet::new();
        let mut stack = graph.roots.clone();
        while let Some(id) = stack.pop() {
            if seen.insert(id) {
                if let Some(node) = graph.nodes.get(&id) {
                    stack.extend(node.children.iter().chain(node.link_target.iter()));
                }
            }
        }
        unreachable += (graph.nodes.len()
            - seen
                .iter()
                .filter(|id| graph.nodes.contains_key(id))
                .count()) as u64;
        for decision in graph.decisions() {
            decisions += 1;
            multi += u64::from(decision.len() > 1);
            for choice in &decision {
                choices += 1;
                targeted += u64::from(choice.next_line_id.is_some());
                match choice.alignment.map(|a| a.as_str()) {
                    Some("lightside") => light += 1,
                    Some("darkside") => dark += 1,
                    _ => {}
                }
            }
        }
    }
    println!("conversations read {read}, failed {failed}; nodes {nodes}, unreachable from roots {unreachable}");
    println!("dangling references {dangling} in {dangling_cnv} conversations");
    println!("decisions {decisions}, with 2+ choices {multi}; choices {choices}, with a next line {targeted}; lightside {light}, darkside {dark}");
    Ok(())
}
