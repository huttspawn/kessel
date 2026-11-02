//! Walks planets on an evicted file, then reads quests. See README.md.
use std::time::Instant;

use rannoch_store::{Direction, NodeRef, View, open};

fn walk(
    g: &rannoch_store::Graph,
    label: &str,
    planet: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let view = View::default();
    let start = Instant::now();
    let linked = g.links(&NodeRef::new("planet", planet), "on", Direction::In, &view)?;
    let links_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut each = Vec::with_capacity(linked.len());
    for (key, _) in &linked {
        let t = Instant::now();
        g.state_at(&NodeRef::new("npc", key), &view)?;
        each.push(t.elapsed().as_micros());
    }
    each.sort_unstable();
    println!(
        "{label} {planet}: {} nodes; links {links_ms:.1} ms; p50 {} us, p99 {} us, max {} us; total {:.1} ms",
        each.len(),
        each[each.len() / 2],
        each[each.len() * 99 / 100],
        each[each.len() - 1],
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let t = Instant::now();
    let g = open(&args[1], &[7u8; 32])?;
    println!("open: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    walk(&g, "C1 cold", "hutta")?;
    walk(&g, "C2 warm", "hutta")?;
    walk(&g, "C3 cold", "tatooine")?;
    let view = View::default();
    let quests = g.nodes("quest", &Default::default(), &view)?;
    let mut each = Vec::with_capacity(quests.len());
    for (key, _) in &quests {
        let t = Instant::now();
        g.state_at(&NodeRef::new("quest", key), &view)?;
        each.push(t.elapsed().as_micros());
    }
    each.sort_unstable();
    println!(
        "C4 warm quests: {} nodes; p50 {} us, p99 {} us, max {} us",
        each.len(),
        each[each.len() / 2],
        each[each.len() * 99 / 100],
        each[each.len() - 1]
    );
    Ok(())
}
