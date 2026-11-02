//! See README.md: predictions first, then this run.
use std::{thread::sleep, time::Duration};

use rannoch_store::{
    At, Class, Date, Declaration, EventId, Graph, HistoryKey, NodeRef, Value, open,
};

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn put(g: &mut Graph, val: &str, build: i64) -> Result<(), Box<dyn std::error::Error>> {
    // Builds are days of January, so valid time and build number line up.
    let day = if build == 0 {
        "2025-12-31".to_string()
    } else {
        format!("2026-01-{build:02}")
    };
    let body = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.object","subject":"o1","data":{{"properties":{{"val":"{val}"}},"valid":{{"on":"{day}"}},"integer":{build}}}}}"#,
        id()
    );
    for r in g.commit(&[&body], &[])? {
        r.map_err(|e| e.to_string())?;
    }
    g.fill()?;
    sleep(Duration::from_millis(5));
    Ok(())
}

fn at(g: &Graph, build: i64) -> Result<String, Box<dyn std::error::Error>> {
    let v = g.state_at(&NodeRef::new("object", "o1"), At::Key(build))?;
    Ok(match v.and_then(|v| v.properties.get("val").cloned()) {
        Some(Value::Text(t)) => t,
        other => format!("{other:?}"),
    })
}

fn check(name: &str, predicted: &str, actual: &str) {
    let mark = if predicted == actual { "ok  " } else { "MISS" };
    println!("{mark} {name}: predicted {predicted}, actual {actual}");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("rannoch-spine-{}", EventId::now()));
    std::fs::create_dir_all(&dir)?;
    let mut g = open(dir.join("spine.rannoch"), &[7u8; 32])?;
    let mut class = Class::new("object", ["val"]);
    class.history = HistoryKey::Integer;
    let decl = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.object"}}"#,
        id()
    );
    g.declare(&Declaration::Class(class), &decl)?;
    let node = NodeRef::new("object", "o1");

    put(&mut g, "A", 1)?;
    put(&mut g, "A", 2)?;
    check(
        "P1 hearings after unchanged build",
        "1",
        &g.hearings(&node)?.len().to_string(),
    );
    check("P2 build 1", "A", &at(&g, 1)?);
    check("P2 build 2", "A", &at(&g, 2)?);

    put(&mut g, "B", 3)?;
    put(&mut g, "C", 3)?;
    check("P3 build 3 after restatement", "C", &at(&g, 3)?);
    check("P4 build 2 undisturbed", "A", &at(&g, 2)?);

    put(&mut g, "C", 4)?;
    check("P4 build 4", "C", &at(&g, 4)?);
    put(&mut g, "A", 5)?;
    check("P5 build 4 before return", "C", &at(&g, 4)?);
    check("P5 build 5 returned", "A", &at(&g, 5)?);

    put(&mut g, "Z", 0)?;
    check("P6 build 0 late", "Z", &at(&g, 0)?);
    check("P6 build 5 after late import", "A", &at(&g, 5)?);
    let now = g.find("object", "o1")?;
    println!("     now: {now:?}");
    check(
        "P7 hearings at end",
        "4",
        &g.hearings(&node)?.len().to_string(),
    );

    let known = g.find_as_of("object", "o1", &Date::day(2026, 1, 3), Some(At::Key(3)))?;
    println!("P8   as of build 3's date, known at Key(3): {known:?}");
    for h in g.hearings(&node)? {
        println!(
            "     hearing integer={:?} aliases={} valid={:?}",
            h.integer,
            h.aliases.len(),
            h.valid
        );
    }
    if let Some(s) = g.seen(&node)? {
        println!(
            "     seen count={} first={:?} last={:?}",
            s.count, s.first_heard.integer, s.last_heard.integer
        );
    }
    Ok(())
}
