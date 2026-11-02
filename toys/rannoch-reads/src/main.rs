//! See README.md: predictions first, then this run.
use std::{
    fs::File,
    io::{BufRead, BufReader},
    time::Instant,
};

use rannoch_store::{Class, Declaration, Direction, EventId, NodeRef, Relation, View, open};

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn declared(name: &str) -> String {
    format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.{name}"}}"#,
        id()
    )
}

fn pct(sorted: &[u128], p: f64) -> u128 {
    sorted[((sorted.len() - 1) as f64 * p) as usize]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    std::fs::create_dir_all(&args[2])?;
    let mut g = open(
        format!("{}/{}.rannoch", args[2], EventId::now()),
        &[7u8; 32],
    )?;
    for name in ["npc", "item", "quest", "ability", "planet"] {
        let mut class = Class::new(name, ["fqn", "json"]);
        class.axis = vec!["build".to_string()];
        g.declare(&Declaration::Class(class), &declared(name))?;
    }
    let mut on = Relation::new("on", "npc", "planet", false);
    on.axis = vec!["build".to_string()];
    g.declare(&Declaration::Relation(on), &declared("on"))?;

    let started = Instant::now();
    let mut count = 0usize;
    let mut batch: Vec<String> = Vec::with_capacity(5000);
    for line in BufReader::new(File::open(&args[1])?).lines() {
        batch.push(line?.replace("@@ID@@", &id()));
        count += 1;
        if batch.len() == 5000 {
            let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
            for r in g.commit(&refs, &[])? {
                r.map_err(|e| e.to_string())?;
            }
            g.fill()?;
            batch.clear();
        }
    }
    let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
    for r in g.commit(&refs, &[])? {
        r.map_err(|e| e.to_string())?;
    }
    g.fill()?;
    let secs = started.elapsed().as_secs_f64();
    println!(
        "P1 load: {count} events in {secs:.1} s, {:.0} events/s",
        count as f64 / secs
    );
    println!("stopped events: {}", g.stopped()?.len());

    let view = View::default();
    for planet in ["hutta", "dromund_kaas", "balmorra", "tatooine"] {
        let node = NodeRef::new("planet", planet);
        let t = Instant::now();
        let linked = g.links(&node, "on", Direction::In, &view)?;
        let links_us = t.elapsed().as_micros();
        let mut each = Vec::with_capacity(linked.len());
        let walk = Instant::now();
        for (key, _) in &linked {
            let t = Instant::now();
            let found = g.state_at(&NodeRef::new("npc", key), &view)?;
            each.push(t.elapsed().as_micros());
            assert!(found.is_some(), "{key}");
        }
        let walk_ms = walk.elapsed().as_secs_f64() * 1000.0;
        each.sort_unstable();
        if each.is_empty() {
            println!("{planet}: no nodes");
            continue;
        }
        println!(
            "{planet}: {} nodes; links {links_us} us; state_at p50 {} us, p99 {} us, max {} us; walk {walk_ms:.1} ms",
            each.len(),
            pct(&each, 0.5),
            pct(&each, 0.99),
            each[each.len() - 1],
        );
    }
    Ok(())
}
