//! See README.md: predictions first, then this run.
use std::{
    thread::sleep,
    time::{Duration, SystemTime},
};

use rannoch_store::{
    At, Class, Cursor, Date, Declaration, EventId, Graph, HistoryKey, NodeRef, Relation, Value,
    open,
};

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn id() -> String {
    format!("{}:0", EventId::now())
}

fn tick() {
    sleep(Duration::from_millis(4));
}

/// A clock mark strictly between what was recorded before it and what is recorded after it.
fn mark() -> At {
    tick();
    let at = At::Time(SystemTime::now());
    tick();
    at
}

fn store(name: &str, key: u8) -> Res<Graph> {
    let dir = std::env::temp_dir().join(format!("rannoch-axis-{name}-{}", EventId::now()));
    std::fs::create_dir_all(&dir)?;
    let mut g = open(dir.join("g.rannoch"), &[key; 32])?;
    g.set_try_limit(1)?;
    declare(&mut g, "object", HistoryKey::Integer)?;
    Ok(g)
}

fn declare(g: &mut Graph, name: &str, history: HistoryKey) -> Res<Option<rannoch_store::Event>> {
    let mut class = Class::new(name, ["val"]);
    class.history = history;
    let decl = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.{name}"}}"#,
        id()
    );
    Ok(g.declare(&Declaration::Class(class), &decl)?)
}

fn node_body(key: &str, val: &str, build: i64) -> String {
    // Builds are days of January, so valid time and build number line up, as in rannoch-spine.
    let day = if build == 0 {
        "2025-12-31".to_string()
    } else {
        format!("2026-01-{:02}", (build % 28) + 1)
    };
    format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.object","subject":"{key}","data":{{"properties":{{"val":"{val}"}},"valid":{{"on":"{day}"}},"integer":{build}}}}}"#,
        id()
    )
}

fn send(g: &mut Graph, body: &str) -> Res<rannoch_store::Event> {
    let mut landed = g.commit(&[body], &[])?;
    let event = landed.remove(0).map_err(|e| e.to_string())?;
    g.fill()?;
    tick();
    Ok(event)
}

fn put(g: &mut Graph, key: &str, val: &str, build: i64) -> Res<rannoch_store::Event> {
    send(g, &node_body(key, val, build))
}

fn val(v: Option<&std::collections::BTreeMap<String, Value>>) -> String {
    match v.and_then(|p| p.get("val")) {
        Some(Value::Text(t)) => t.clone(),
        Some(other) => format!("{other:?}"),
        None => "none".into(),
    }
}

fn now_at(g: &Graph, key: &str, build: i64) -> Res<String> {
    let v = g.state_at(&NodeRef::new("object", key), At::Key(build))?;
    Ok(val(v.as_ref().map(|v| &v.properties)))
}

fn known(g: &Graph, key: &str, build: i64, at: At) -> Res<String> {
    let v = g.state_known_at(&NodeRef::new("object", key), At::Key(build), at)?;
    Ok(val(v.as_ref().map(|v| &v.properties)))
}

fn check(name: &str, predicted: &str, actual: &str) {
    let tag = if predicted == actual { "ok  " } else { "MISS" };
    println!("{tag} {name}: predicted {predicted}, actual {actual}");
}

fn amend_body(target: EventId, patch: &str) -> String {
    format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.amended","subject":"{target}","data":{patch}}}"#,
        id()
    )
}

/// The marks of the N sequence.
#[derive(Clone, Copy)]
struct Marks {
    m1: At,
    m3: At,
}

/// N1-N6, on any store holding the N sequence. `who` labels the lines.
fn n_reads(who: &str, g: &Graph, m: Marks) -> Res<()> {
    check(&format!("{who}N1 Key(3) known at mark 1"), "B", &known(g, "o1", 3, m.m1)?);
    check(&format!("{who}N2 Key(3) now"), "C", &now_at(g, "o1", 3)?);
    check(&format!("{who}N3 Key(2) known at mark 1"), "A", &known(g, "o1", 2, m.m1)?);
    check(&format!("{who}N3 Key(2) now"), "A", &now_at(g, "o1", 2)?);
    check(&format!("{who}N4 Key(0) known at mark 3"), "none", &known(g, "o1", 0, m.m3)?);
    check(&format!("{who}N4 Key(0) now"), "Z", &now_at(g, "o1", 0)?);
    check(&format!("{who}N5 Key(5) now"), "A", &now_at(g, "o1", 5)?);
    let found = g.find("object", "o1")?;
    check(&format!("{who}N5 find"), "A", &val(found.as_ref().map(|n| &n.properties)));
    let as_of = g.find_as_of("object", "o1", &Date::day(2026, 1, 4), Some(m.m1))?;
    check(&format!("{who}N6 find_as_of build 3's date, known at mark 1"), "B", &val(as_of.as_ref().map(|n| &n.properties)));
    Ok(())
}

fn replay(g: &mut Graph, bodies: &[String]) -> Res<()> {
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    for r in g.commit(&refs, &[])? {
        r.map_err(|e| e.to_string())?;
    }
    g.fill()?;
    Ok(())
}

fn main() -> Res<()> {
    let mut k = store("kessel", 7)?;

    println!("-- N: restatement under clock known-at");
    put(&mut k, "o1", "A", 1)?;
    put(&mut k, "o1", "A", 2)?;
    put(&mut k, "o1", "B", 3)?;
    let m1 = mark();
    put(&mut k, "o1", "C", 3)?;
    put(&mut k, "o1", "C", 4)?;
    put(&mut k, "o1", "A", 5)?;
    let m3 = mark();
    put(&mut k, "o1", "Z", 0)?;
    let m = Marks { m1, m3 };
    n_reads("", &k, m)?;
    let refused = k.find_as_of("object", "o1", &Date::day(2026, 1, 4), Some(At::Key(3)));
    check("N7 find_as_of known-at Key(3)", "refused", if refused.is_err() { "refused" } else { "answered" });

    println!("-- A: amendment in an integer class");
    let c = put(&mut k, "o2", "C", 3)?;
    let before = mark();
    send(&mut k, &amend_body(c.id, r#"{"data":{"properties":{"val":"C2"}}}"#))?;
    check("A1 Key(3) before the amendment", "C", &known(&k, "o2", 3, before)?);
    check("A1 Key(3) after", "C2", &now_at(&k, "o2", 3)?);
    let o2 = NodeRef::new("object", "o2");
    check("A2 hearings", "1", &k.hearings(&o2)?.len().to_string());

    let c3 = put(&mut k, "o3", "C", 3)?;
    send(&mut k, &amend_body(c3.id, r#"{"data":{"integer":9,"properties":{"val":"moved"}}}"#))?;
    check("A3 Key(3) after patching data.integer to 9", "moved", &now_at(&k, "o3", 3)?);
    let stopped_before = k.stopped()?.len();
    let c4 = put(&mut k, "o4", "C", 3)?;
    send(&mut k, &amend_body(c4.id, r#"{"data":{"integer":null}}"#))?;
    k.fill()?;
    check("A4 stopped", "1", &(k.stopped()?.len() - stopped_before).to_string());
    check("A4 Key(3) keeps its value", "C", &now_at(&k, "o4", 3)?);
    let stopped_a = k.stopped()?.len();
    println!("     stopped so far: {:?}", k.stopped()?);

    println!("-- R: a correction that reverts");
    put(&mut k, "o5", "A", 3)?;
    tick();
    put(&mut k, "o5", "B", 3)?;
    let r_mid = mark();
    put(&mut k, "o5", "A", 3)?;
    let hs = k.hearings(&NodeRef::new("object", "o5"))?;
    let aliases: usize = hs.iter().map(|h| h.aliases.len()).sum();
    check("R1 hearings, aliases", "2,1", &format!("{},{aliases}", hs.len()));
    check("R2 Key(3) known between", "B", &known(&k, "o5", 3, r_mid)?);
    check("R2 Key(3) now", "A", &now_at(&k, "o5", 3)?);

    println!("-- D: write-only-what-changed meets a restatement");
    put(&mut k, "o6", "B", 3)?;
    put(&mut k, "o6", "C", 3)?;
    check("D1 Key(4), truth is B", "C", &now_at(&k, "o6", 4)?);
    let d_mid = mark();
    put(&mut k, "o6", "B", 4)?;
    check("D2 Key(4) now", "B", &now_at(&k, "o6", 4)?);
    check("D2 Key(4) known before the re-diff", "C", &known(&k, "o6", 4, d_mid)?);

    println!("-- C: compound axis, (build, step) packed as build << 16 | step");
    let pack = |b: i64, s: i64| (b << 16) | s;
    let top = |b: i64| pack(b, 0xFFFF);
    put(&mut k, "o7", "B", pack(3, 0))?;
    put(&mut k, "o7", "C", pack(3, 1))?;
    put(&mut k, "o7", "D", pack(3, 2))?;
    let c_mid = mark();
    put(&mut k, "o7", "E", pack(3, 1))?;
    check("C1 (3, top): newest recording loses to the higher step", "D", &now_at(&k, "o7", top(3))?);
    check("C2 (3, 0) misses steps 1 and 2", "B", &now_at(&k, "o7", pack(3, 0))?);
    check("C3 (3, 1) known before E", "C", &known(&k, "o7", pack(3, 1), c_mid)?);
    check("C3 (3, 1) now", "E", &now_at(&k, "o7", pack(3, 1))?);
    put(&mut k, "o7", "F", pack(4, 0))?;
    put(&mut k, "o7", "G", pack(4, 0))?;
    check("C4 tie at (4, 0)", "G", &now_at(&k, "o7", pack(4, 0))?);

    println!("-- W: replay into a second store, another content key, one batch, unchanged bodies");
    let bodies: Vec<String> = k
        .events()
        .since(Cursor::START)?
        .into_iter()
        .map(|e| e.body)
        .filter(|b| !b.contains("rannoch.declared."))
        .collect();
    println!("     {} events", bodies.len());
    let w_start = SystemTime::now();
    let mut w = store("warden", 9)?;
    replay(&mut w, &bodies)?;
    for _ in 0..2 {
        w.fill()?;
    }
    n_reads("W1 ", &w, m)?;
    let o1 = NodeRef::new("object", "o1");
    let (kh, wh) = (k.hearings(&o1)?, w.hearings(&o1)?);
    let same_events = kh.iter().map(|h| h.event).eq(wh.iter().map(|h| h.event));
    let same_identity = kh.iter().map(|h| h.identity).eq(wh.iter().map(|h| h.identity));
    check("W2 hearing events equal across stores", "true", &same_events.to_string());
    check("W2 content identities equal across stores", "false", &same_identity.to_string());
    let (kn, wn) = (k.find("object", "o1")?, w.find("object", "o1")?);
    if let (Some(kn), Some(wn)) = (&kn, &wn) {
        println!("     Node.recorded differs: first store {:?}, second {:?}, replay began {:?}", kn.recorded, wn.recorded, w_start);
    }
    check("W3 amendment does not replay: Key(3) on o2 here", "C", &now_at(&w, "o2", 3)?);
    check("W3 and in the first store", "C2", &now_at(&k, "o2", 3)?);
    let w_stopped = w.stopped()?;
    println!("     second store stopped {} events (first store {stopped_a}); first reason: {:?}", w_stopped.len(), w_stopped.first().map(|(_, r)| r));

    let count = w.hearings(&o1)?.len();
    replay(&mut w, &bodies)?;
    let after = w.hearings(&o1)?;
    let aliases: usize = after.iter().map(|h| h.aliases.len()).sum();
    check("W4 redelivery: hearings unchanged, aliases 0", &format!("{count},0"), &format!("{},{aliases}", after.len()));
    n_reads("W4 ", &w, m)?;

    let mut third = store("reverse", 11)?;
    let mut reversed = bodies.clone();
    reversed.reverse();
    replay(&mut third, &reversed)?;
    third.fill()?;
    n_reads("W5 reversed ", &third, m)?;

    check("W6 before the re-mint: Key(3)", "C", &now_at(&third, "o1", 3)?);
    put(&mut third, "o1", "B", 3)?;
    check("W6 old B under a new id: Key(3)", "B", &now_at(&third, "o1", 3)?);
    let hs = third.hearings(&o1)?;
    println!("     re-mint hearings {} aliases {}", hs.len(), hs.iter().map(|h| h.aliases.len()).sum::<usize>());

    println!("-- M: key part missing, changing the axis");
    let m_stopped = k.stopped()?.len();
    let body = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.object","subject":"o8","data":{{"properties":{{"val":"X"}},"valid":"unknown"}}}}"#,
        id()
    );
    send(&mut k, &body)?;
    check("M1 stopped", "1", &(k.stopped()?.len() - m_stopped).to_string());
    check("M1 hearings", "0", &k.hearings(&NodeRef::new("object", "o8"))?.len().to_string());
    check("M2 declared again as Time", "refused", match declare(&mut k, "object", HistoryKey::Time) {
        Err(_) => "refused",
        Ok(_) => "accepted",
    });
    check("M3 declared again unchanged", "None", match declare(&mut k, "object", HistoryKey::Integer) {
        Ok(None) => "None",
        Ok(Some(_)) => "wrote",
        Err(_) => "refused",
    });

    println!("-- T: Time and Origin classes");
    let mut t = store("time", 7)?;
    declare(&mut t, "dated", HistoryKey::Time)?;
    declare(&mut t, "plain", HistoryKey::Origin)?;
    let ms = |ago_days: u64| -> Res<(i64, SystemTime)> {
        let at = SystemTime::now() - Duration::from_secs(ago_days * 86_400);
        Ok((i64::try_from(at.duration_since(SystemTime::UNIX_EPOCH)?.as_millis())?, at))
    };
    let (week_ago, _) = ms(7)?;
    let (_, three_days_ago) = ms(3)?;
    let dated = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.dated","subject":"t1","data":{{"properties":{{"val":"late"}},"valid":{{"on":"2026-01-01"}},"time":{week_ago}}}}}"#,
        id()
    );
    send(&mut t, &dated)?;
    let plain = format!(
        r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.node.plain","subject":"p1","data":{{"properties":{{"val":"late"}},"valid":{{"on":"2026-01-01"}}}}}}"#,
        id()
    );
    send(&mut t, &plain)?;
    let day = Date::day(2026, 1, 1);
    let as_known = At::Time(three_days_ago);
    let v = t.find_as_of("dated", "t1", &day, Some(as_known))?;
    check("T1 Time class known 3 days ago", "late", &val(v.as_ref().map(|n| &n.properties)));
    let v = t.find_as_of("plain", "p1", &day, Some(as_known))?;
    check("T2 Origin class known 3 days ago", "none", &val(v.as_ref().map(|n| &n.properties)));

    println!("-- L: links on the build axis");
    let mut l = store("link", 7)?;
    l.declare(
        &Declaration::Relation(Relation::new("rel", "object", "object", false)),
        &format!(r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.declared.rel"}}"#, id()),
    )?;
    put(&mut l, "a", "A", 1)?;
    put(&mut l, "b", "B", 1)?;
    let link = |build: i64| {
        format!(
            r#"{{"specversion":"1.0","id":"{}","source":"kessel","type":"rannoch.link.rel","data":{{"from":{{"class":"object","key":"a"}},"to":{{"class":"object","key":"b"}},"valid":{{"on":"2026-01-01"}},"integer":{build}}}}}"#,
            id()
        )
    };
    send(&mut l, &link(3))?;
    send(&mut l, &link(4))?;
    check("L1 link_state_at Key(3)", "refused", match l.link_state_at("rel", "a", "b", At::Key(3)) {
        Err(_) => "refused",
        Ok(_) => "answered",
    });
    let hs = l.link_hearings("rel", "a", "b")?;
    let aliases: usize = hs.iter().map(|h| h.aliases.len()).sum();
    check("L2 hearings, aliases", "1,1", &format!("{},{aliases}", hs.len()));
    println!("     link hearing integers: {:?}", hs.iter().map(|h| h.integer).collect::<Vec<_>>());
    Ok(())
}
