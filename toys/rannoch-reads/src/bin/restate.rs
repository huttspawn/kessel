//! A correction at one build, read before and after it was known. See README.md.
use std::{
    thread::sleep,
    time::{Duration, SystemTime},
};

use rannoch_store::{Class, Declaration, EventId, Graph, NodeRef, Value, View, open};

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn put(g: &mut Graph, val: &str, build: i64) -> Result<SystemTime, Box<dyn std::error::Error>> {
    let body = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.object","subject":"o1","data":{{"properties":{{"val":"{val}"}},"axis":{{"build":{build}}}}}}}"#,
        id()
    );
    for r in g.commit(&[&body], &[])? {
        r.map_err(|e| e.to_string())?;
    }
    g.fill()?;
    sleep(Duration::from_millis(10));
    let after = SystemTime::now();
    sleep(Duration::from_millis(10));
    Ok(after)
}

fn read(
    g: &Graph,
    build: i64,
    known: Option<SystemTime>,
) -> Result<String, Box<dyn std::error::Error>> {
    let view = View {
        axis: Some(vec![build]),
        known,
    };
    let v = g.state_at(&NodeRef::new("object", "o1"), &view)?;
    Ok(match v.and_then(|v| v.properties.get("val").cloned()) {
        Some(Value::Text(t)) => t,
        other => format!("{other:?}"),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("rannoch-restate-{}", EventId::now()));
    std::fs::create_dir_all(&dir)?;
    let mut g = open(dir.join("g.rannoch"), &[7u8; 32])?;
    let mut class = Class::new("object", ["val"]);
    class.axis = vec!["build".to_string()];
    let decl = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.object"}}"#,
        id()
    );
    g.declare(&Declaration::Class(class), &decl)?;
    let before_b = put(&mut g, "A", 2)?;
    let before_fix = put(&mut g, "B", 3)?;
    put(&mut g, "C", 3)?;
    println!("R1 build 3 now: predicted C, actual {}", read(&g, 3, None)?);
    println!(
        "R2 build 3 known before fix: predicted B, actual {}",
        read(&g, 3, Some(before_fix))?
    );
    println!("R3 build 2 now: predicted A, actual {}", read(&g, 2, None)?);
    println!(
        "R3 build 2 known before fix: predicted A, actual {}",
        read(&g, 2, Some(before_fix))?
    );
    println!(
        "R4 build 3 known before B: predicted A, actual {}",
        read(&g, 3, Some(before_b))?
    );
    Ok(())
}
