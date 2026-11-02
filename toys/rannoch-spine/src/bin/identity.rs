//! Which part of an event makes two sends of the same content two hearings? See README.md.
use std::{thread::sleep, time::Duration};

use rannoch_store::{Class, Declaration, EventId, Graph, HistoryKey, NodeRef, open};

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn send(g: &mut Graph, class: &str, data_tail: &str) -> Result<(), Box<dyn std::error::Error>> {
    let body = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.{class}","subject":"o1","data":{{"properties":{{"val":"A"}},{data_tail}}}}}"#,
        id()
    );
    for r in g.commit(&[&body], &[])? {
        r.map_err(|e| e.to_string())?;
    }
    g.fill()?;
    sleep(Duration::from_millis(5));
    Ok(())
}

fn variant(
    name: &str,
    predicted: &str,
    history: HistoryKey,
    tails: [&str; 2],
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("rannoch-identity-{}", EventId::now()));
    std::fs::create_dir_all(&dir)?;
    let mut g = open(dir.join("g.rannoch"), &[7u8; 32])?;
    let mut class = Class::new("object", ["val"]);
    class.history = history;
    let decl = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.object"}}"#,
        id()
    );
    g.declare(&Declaration::Class(class), &decl)?;
    for tail in tails {
        send(&mut g, "object", tail)?;
    }
    let hearings = g.hearings(&NodeRef::new("object", "o1"))?;
    let aliases: usize = hearings.iter().map(|h| h.aliases.len()).sum();
    println!(
        "{name}: predicted {predicted}; actual {} hearings, {aliases} aliases",
        hearings.len()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let v = r#""valid":{"on":"2026-01-01"}"#;
    let v2 = r#""valid":{"on":"2026-01-02"}"#;
    variant(
        "V1 integer differs",
        "2 hearings",
        HistoryKey::Integer,
        [
            &format!(r#"{v},"integer":1"#),
            &format!(r#"{v},"integer":2"#),
        ],
    )?;
    variant(
        "V2 nothing differs",
        "1 hearing, 1 alias",
        HistoryKey::Origin,
        [v, v],
    )?;
    variant(
        "V3 valid differs",
        "2 hearings",
        HistoryKey::Origin,
        [v, v2],
    )?;
    variant(
        "V4 time differs",
        "2 hearings",
        HistoryKey::Time,
        [
            &format!(r#"{v},"time":1000"#),
            &format!(r#"{v},"time":2000"#),
        ],
    )?;
    Ok(())
}
