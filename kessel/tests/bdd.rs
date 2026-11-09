//! Runs the `.feature` files under this crate's `tests/` directory.
//!
//! A scenario with no matching step code fails `cargo test`, and the failure
//! names the scenario. The tag filter below runs the scenarios that have step
//! code: the `@smoke` checks and FR-KESSEL-025 c1 to c10. The `@local`
//! scenarios read a full local extract named by `KESSEL_SPICE` and real combat
//! logs named by `KESSEL_COMBAT_LOG` (a file or a directory), and are skipped
//! when those are unset (CI never has them).

use kessel::hash::compute_icon_id;
use rstest_bdd_macros::{given, scenarios, then, when};
use rusqlite::{Connection, OpenFlags};

#[derive(Default)]
struct World {
    name: String,
    icon_id: String,
    /// The local extract, opened read-only.
    spice: Option<Connection>,
    /// The placeable, volume or codex entry the scenario holds.
    subject: String,
    /// The placeable the scenario says grants the subject.
    granter: String,
    /// Codex-granting trigger volumes: log area id, volume, log-frame box.
    volumes: Vec<(i64, String, [f64; 6])>,
    /// The log area id and the log-frame positions the scenario's log lines print.
    log_area_id: i64,
    positions: Vec<[f64; 3]>,
    /// Granting use or conversation targets: log area id, the id the log
    /// prints (cast id or TargetSet id), fqn, log-frame position.
    targets: Vec<(i64, i64, String, [f64; 3])>,
    /// The logged cast id of a use line.
    cast_id: i64,
    /// The NPC the player's TargetSet named just before a conversation: its
    /// log id and logged position.
    target_set: Option<(i64, [f64; 3])>,
    /// The player's casts at the spot of a conversation since the last one
    /// began, latest first: cast id and position.
    casts: Vec<(i64, [f64; 3])>,
    /// The answer: one line per fact found.
    answer: Vec<String>,
}

#[rstest::fixture]
fn world() -> World {
    World::default()
}

#[given("the icon name {name:string}")]
fn icon_name(world: &mut World, name: String) {
    world.name = name;
}

#[when("its icon id is computed")]
fn compute(world: &mut World) {
    world.icon_id = compute_icon_id(&world.name);
}

#[then("the icon id is {expected:string}")]
fn icon_id_is(world: &World, expected: String) {
    assert_eq!(world.icon_id, expected);
}

/// The local extract, or skip the scenario when none is named.
fn spice(world: &mut World) -> &Connection {
    if world.spice.is_none() {
        let Ok(path) = std::env::var("KESSEL_SPICE") else {
            rstest_bdd::skip!("@local: set KESSEL_SPICE to a default extract of a live build");
        };
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap_or_else(|e| panic!("open {path}: {e}"));
        world.spice = Some(conn);
    }
    world.spice.as_ref().unwrap()
}

// FR-KESSEL-025 c4: a codex entry answers its sources.

#[given("a build holding a lore codex entry granted by using a placeable")]
fn lore_codex(world: &mut World) {
    // A lore object: a placeable played through an abl.placeables.examine.*
    // ability ('Examining...') that names a cdx.lore.* entry. Read from placeable_grants, not codex_sources, so
    // the Then can fail.
    let (placeable, codex): (String, String) = spice(world)
        .query_row(
            "SELECT placeable_fqn, codex_fqn FROM placeable_grants \
             WHERE codex_fqn LIKE 'cdx.lore.%' AND use_ability_fqn LIKE 'abl.placeables.examine.%' \
             ORDER BY placeable_fqn LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("a lore placeable naming a codex entry");
    world.granter = placeable;
    world.subject = codex;
}

#[when("kessel's agent asks for that codex entry")]
fn ask_codex(world: &mut World) {
    let codex = world.subject.clone();
    let mut stmt = spice(world)
        .prepare("SELECT source_kind, source_fqn, via FROM codex_sources WHERE codex_fqn = ?1")
        .expect("codex sources query");
    let answer = stmt
        .query_map([&codex], |r| {
            Ok(format!(
                "{} {} {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .expect("codex source rows")
        .map(|r| r.expect("codex source row"))
        .collect();
    drop(stmt);
    world.answer = answer;
}

#[then("the answer names the placeable that grants it")]
fn names_granting_placeable(world: &World) {
    let placeable = format!("placeable {} ", world.granter);
    assert!(
        world.answer.iter().any(|a| a.starts_with(&placeable)),
        "{} answered no source naming {}: {:?}",
        world.subject,
        world.granter,
        world.answer
    );
}

// FR-KESSEL-025 c8: an item answers its sources.

#[given("a build holding an armor piece dropped only by a world event")]
fn world_event_armor(world: &mut World) {
    // An armor piece wevEventInfoPrototype_client names as an event's drop
    // (1bdc8ed4), read from the singleton itself, not item_sources, so the
    // Then can fail. The event's quest (6f568b11) is the one the answer names.
    use base64::Engine as _;
    use kessel::gom_reader::{read_framed_object, GomValue};
    let conn = spice(world);
    let (header, payload): (String, String) = conn
        .query_row(
            "SELECT header_hex, payload_b64 FROM singletons WHERE fqn = 'wevEventInfoPrototype_client'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("wevEventInfoPrototype_client");
    let header = hex::decode(header).expect("header hex");
    let payload = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("payload base64");
    let (proto, _) = read_framed_object(&header, &payload).expect("decode world events");
    let fqn = |v: &GomValue| -> Option<String> {
        let guid = match v {
            GomValue::U64(n) => *n,
            GomValue::I64(n) => *n as u64,
            _ => return None,
        };
        conn.query_row(
            "SELECT fqn FROM objects WHERE guid = ?1 AND is_canonical = 1",
            [format!("{guid:016X}")],
            |r| r.get(0),
        )
        .ok()
    };
    let mut drops: Vec<(String, String)> = Vec::new();
    for (_, event) in proto
        .embedded_field(0x395e_03f8)
        .and_then(GomValue::as_map)
        .unwrap_or_default()
    {
        let Some(quest) = event.embedded_field(0x6f56_8b11).and_then(fqn) else {
            continue;
        };
        let items = event
            .embedded_field(0x1bdc_8ed4)
            .and_then(GomValue::as_list);
        for item in items.unwrap_or_default().iter().filter_map(fqn) {
            if item.contains(".armor") {
                drops.push((item, quest.clone()));
            }
        }
    }
    drops.sort();
    let (item, quest) = drops
        .into_iter()
        .next()
        .expect("an armor piece a world event drops");
    world.subject = item;
    world.granter = quest;
}

#[when("kessel's agent asks for that item")]
fn ask_item(world: &mut World) {
    let item = world.subject.clone();
    let mut stmt = spice(world)
        .prepare("SELECT source_kind, source_fqn, detail FROM item_sources WHERE item_fqn = ?1")
        .expect("item sources query");
    let answer = stmt
        .query_map([&item], |r| {
            Ok(format!(
                "{} {} {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .expect("item source rows")
        .map(|r| r.expect("item source row"))
        .collect();
    drop(stmt);
    println!("c8 {item}: {answer:?}");
    world.answer = answer;
}

#[then("the answer names the world event that drops it")]
fn names_world_event(world: &World) {
    let event = format!("world_event {} ", world.granter);
    assert!(
        world.answer.iter().any(|a| a.starts_with(&event)),
        "{} answered no world event {}: {:?}",
        world.subject,
        world.granter,
        world.answer
    );
}

// FR-KESSEL-025 c5: a granting placeable answers what and where.

#[given("a build holding a datacron placeable")]
fn datacron_placeable(world: &mut World) {
    // A real datacron: the placeable a `data_holocron` spawner places. No grant
    // field is filtered on, so the Then can fail.
    let fqn: String = spice(world)
        .query_row(
            "SELECT entity_fqn FROM placements \
             WHERE spawner_fqn LIKE 'spn.location.%.data_holocron.%' AND entity_fqn IS NOT NULL \
             ORDER BY spawner_fqn = 'spn.location.tython.data_holocron.datacron_willpower_tython' DESC, \
                      spawner_fqn LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("a placed datacron (spn.location.*.data_holocron.*)");
    world.subject = fqn;
}

#[when("kessel's agent asks for that placeable")]
fn ask_placeable(world: &mut World) {
    let fqn = world.subject.clone();
    let conn = spice(world);
    let row: Option<[Option<String>; 4]> = conn
        .query_row(
            "SELECT use_ability_fqn, codex_fqn, conversation_fqn, script_fqn \
             FROM placeable_grants WHERE placeable_fqn = ?1",
            [&fqn],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )
        .ok();
    let [ability, codex, convo, script] = row.unwrap_or_default();
    let mut answer = Vec::new();
    // The use ability is the cast the player plays and the conversation is
    // what opens; neither is a grant. What the conversation gives is.
    if let Some(a) = ability {
        answer.push(format!("plays use ability {a}"));
    }
    if let Some(c) = codex {
        answer.push(format!("grants codex {c}"));
    }
    if let Some(c) = convo {
        answer.push(format!("opens conversation {c}"));
        let mut stmt = conn
            .prepare("SELECT quest_fqn FROM conversation_quest_refs WHERE cnv_fqn = ?1")
            .expect("conversation quest query");
        let quests: Vec<String> = stmt
            .query_map([&c], |r| r.get(0))
            .expect("conversation quest rows")
            .map(|q| q.expect("conversation quest row"))
            .collect();
        answer.extend(quests.into_iter().map(|q| format!("grants quest {q}")));
    }
    if let Some(s) = script {
        answer.push(format!("grants script {s}"));
    }
    let mut stmt = conn
        .prepare(
            "SELECT area_dir_id, world_x, world_y, world_z FROM placements \
             WHERE entity_fqn = ?1 AND world_x IS NOT NULL",
        )
        .expect("placements query");
    let places = stmt
        .query_map([&fqn], |r| {
            Ok(format!(
                "stands in area {} at ({:.2}, {:.2}, {:.2})",
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?
            ))
        })
        .expect("placements rows");
    answer.extend(places.map(|p| p.expect("placement row")));
    drop(stmt);
    println!("c5 {fqn}: {answer:?}");
    world.answer = answer;
}

#[then("the answer names what it grants")]
fn names_grant(world: &World) {
    // c5's kinds of grant: a codex entry, an achievement event, a quest, an item.
    let kinds = [
        "grants codex ",
        "grants achievement event ",
        "grants quest ",
        "grants item ",
    ];
    assert!(
        world
            .answer
            .iter()
            .any(|a| kinds.iter().any(|k| a.starts_with(k))),
        "{} grants no codex, achievement event, quest or item: {:?}",
        world.subject,
        world.answer
    );
}

#[then("gives its area and world position")]
fn gives_place(world: &World) {
    assert!(
        world
            .answer
            .iter()
            .any(|a| a.starts_with("stands in area ")),
        "{} has no placement: {:?}",
        world.subject,
        world.answer
    );
}

// FR-KESSEL-025 c6: a title answers its grant path.

#[given("a build holding a title granted by a script")]
fn script_title(world: &mut World) {
    // A hydra 'Grant Title' action, read from script_actions, not title_grants,
    // so the Then can fail. Its value is a player title id.
    let (script, id): (String, String) = spice(world)
        .query_row(
            "SELECT script_fqn, value FROM script_actions WHERE action = 'Grant Title' \
             ORDER BY value = '114' DESC, script_fqn LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("a script with a Grant Title action");
    world.granter = script;
    world.subject = id;
}

#[when("kessel's agent asks for that title")]
fn ask_title(world: &mut World) {
    let id: i64 = world.subject.parse().expect("a title id");
    let mut stmt = spice(world)
        .prepare(
            "SELECT title_text, source_kind, source_fqn, via FROM title_grants \
             WHERE title_kind = 'player' AND title_id = ?1",
        )
        .expect("title grants query");
    let answer = stmt
        .query_map([id], |r| {
            Ok(format!(
                "{} {} {} ({})",
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(0)?.unwrap_or_default()
            ))
        })
        .expect("title grant rows")
        .map(|r| r.expect("title grant row"))
        .collect();
    drop(stmt);
    println!("c6 title {id}: {answer:?}");
    world.answer = answer;
}

#[then("the answer names the script that grants it")]
fn names_granting_script(world: &World) {
    let script = format!("script {} ", world.granter);
    assert!(
        world.answer.iter().any(|a| a.starts_with(&script)),
        "title {} answered no grant naming {}: {:?}",
        world.subject,
        world.granter,
        world.answer
    );
}

// FR-KESSEL-025 c10: entering a place names the codex it granted.

#[given("a build holding a trigger volume that grants a codex entry, placed in an area")]
fn codex_volume(world: &mut World) {
    let mut stmt = spice(world)
        .prepare(
            "SELECT DISTINCT l.log_area_id, v.area_dir_id || '/' || v.instance_id, \
                    v.log_min_x, v.log_min_y, v.log_min_z, v.log_max_x, v.log_max_y, v.log_max_z \
             FROM trigger_volumes v \
             JOIN script_actions a ON a.script_fqn = v.script_fqn \
             JOIN area_log_ids l ON l.area_dir_id = v.area_dir_id \
             WHERE a.action = 'Grant Codex' AND a.handler = 'On Enter' \
               AND v.log_min_x IS NOT NULL",
        )
        .expect("volume query");
    let volumes: Vec<(i64, String, [f64; 6])> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                [
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ],
            ))
        })
        .expect("volume rows")
        .map(|r| r.expect("volume row"))
        .collect();
    drop(stmt);
    assert!(
        !volumes.is_empty(),
        "no codex-granting trigger volume in an area the log can name"
    );
    world.volumes = volumes;
}

/// The combat-log files `KESSEL_COMBAT_LOG` names (files or directories of
/// `.txt` logs, separated as in `PATH`), or skip the scenario when it is unset.
fn combat_logs() -> Vec<std::path::PathBuf> {
    let Some(paths) = std::env::var_os("KESSEL_COMBAT_LOG") else {
        rstest_bdd::skip!("@local: set KESSEL_COMBAT_LOG to a combat log or a directory of them");
    };
    let mut files = Vec::new();
    for path in std::env::split_paths(&paths) {
        if !path.is_dir() {
            files.push(path);
            continue;
        }
        let mut found: Vec<_> = std::fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect();
        found.sort();
        files.extend(found);
    }
    files
}

#[given(
    "combat-log lines whose player positions in that area move from outside to inside the volume"
)]
fn positions_enter(world: &mut World) {
    let area_re = regex::Regex::new(r"AreaEntered \{\d+\}: [^{]*\{(\d+)\}").expect("regex");
    let pos_re = regex::Regex::new(
        r"^\[[^\]]+\] \[@[^#|\]]+#\d+\|\(([-\d.]+),([-\d.]+),([-\d.]+),[-\d.]+\)",
    )
    .expect("regex");
    let inside = |b: &[f64; 6], p: [f64; 3]| (0..3).all(|i| b[i] <= p[i] && p[i] <= b[i + 3]);
    for file in combat_logs() {
        let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let text = String::from_utf8_lossy(&bytes);
        let (mut area, mut prev): (Option<i64>, Option<[f64; 3]>) = (None, None);
        for (n, line) in text.lines().enumerate() {
            if let Some(m) = area_re.captures(line) {
                area = m[1].parse().ok();
                prev = None;
            }
            let (Some(a), Some(m)) = (area, pos_re.captures(line)) else {
                continue;
            };
            let p = [1, 2, 3].map(|i| m[i].parse::<f64>().expect("position"));
            if let Some(q) = prev.filter(|q| *q != p) {
                if let Some((_, volume, _)) = world
                    .volumes
                    .iter()
                    .find(|(la, _, b)| *la == a && !inside(b, q) && inside(b, p))
                {
                    println!(
                        "c10 crossing: {} line {} area {a} into volume {volume}: {q:?} -> {p:?}",
                        file.display(),
                        n + 1
                    );
                    world.subject = volume.clone();
                    world.log_area_id = a;
                    world.positions = vec![q, p];
                    return;
                }
            }
            prev = Some(p);
        }
    }
    panic!("no logged player positions cross into a codex-granting trigger volume");
}

#[when("kessel's agent asks what those positions granted")]
fn ask_positions(world: &mut World) {
    let (log_area_id, positions) = (world.log_area_id, world.positions.clone());
    let conn = spice(world);
    let inside = |p: [f64; 3]| -> Vec<(String, String)> {
        let mut stmt = conn
            .prepare(
                "SELECT v.area_dir_id || '/' || v.instance_id, v.script_fqn FROM trigger_volumes v \
                 JOIN area_log_ids l ON l.area_dir_id = v.area_dir_id AND l.log_area_id = ?1 \
                 WHERE ?2 BETWEEN v.log_min_x AND v.log_max_x \
                   AND ?3 BETWEEN v.log_min_y AND v.log_max_y \
                   AND ?4 BETWEEN v.log_min_z AND v.log_max_z",
            )
            .expect("volume query");
        stmt.query_map(rusqlite::params![log_area_id, p[0], p[1], p[2]], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .expect("volume rows")
        .map(|r| r.expect("volume row"))
        .collect()
    };
    let mut answer = Vec::new();
    for pair in positions.windows(2) {
        let before = inside(pair[0]);
        for (volume, script) in inside(pair[1]) {
            if before.iter().any(|(v, _)| *v == volume) {
                continue;
            }
            let mut stmt = conn
                .prepare(
                    "SELECT target_fqn FROM script_actions WHERE script_fqn = ?1 \
                     AND handler = 'On Enter' AND action = 'Grant Codex'",
                )
                .expect("grant query");
            for codex in stmt
                .query_map([&script], |r| r.get::<_, Option<String>>(0))
                .expect("grant rows")
            {
                if let Some(codex) = codex.expect("grant row") {
                    answer.push(format!("volume {volume} ({script}) grants {codex}"));
                }
            }
        }
    }
    world.answer = answer;
}

#[then("the answer names the volume and its codex entry")]
fn names_volume_codex(world: &World) {
    let volume = format!("volume {} ", world.subject);
    assert!(
        world
            .answer
            .iter()
            .any(|a| a.starts_with(&volume) && a.contains(" grants cdx.")),
        "entering {} granted nothing: {:?}",
        world.subject,
        world.answer
    );
}

// FR-KESSEL-025 c9: a logged object use or conversation names what it granted.

/// Within reach of a logged position: 10 log units (1 m).
const REACH: f64 = 10.0;
/// The ability every logged conversation shows, and its In Conversation effect.
/// A conversation start: abl.state.conversation applying In Conversation.
const CONVERSATION_START: &str =
    "[Conversation {806968520343552}] [ApplyEffect {836045448945477}: In Conversation {806968520343876}]";

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// Seconds since midnight of a `HH:MM:SS.mmm` stamp.
fn stamp(s: &str) -> f64 {
    s.split(':')
        .map(|x| x.parse::<f64>().expect("timestamp"))
        .fold(0.0, |t, x| t * 60.0 + x)
}

/// One player line of a combat log that c9 reads.
enum LogLine {
    /// The player casts an ability with nothing targeted: cast id, position.
    Cast(i64, [f64; 3]),
    /// The player enters a conversation, at a position.
    Conversation([f64; 3]),
    /// The player targets an NPC: its log id and position.
    TargetSet(i64, [f64; 3]),
}

/// A c9 line: file, line number, log area id, time, player, the line.
type Logged = (String, usize, i64, f64, String, LogLine);

/// Every c9 line of every combat log.
fn log_lines() -> Vec<Logged> {
    let area_re = regex::Regex::new(r"AreaEntered \{\d+\}: [^{]*\{(\d+)\}").expect("regex");
    let player =
        r"^\[([\d:.]+)\] \[@([^#|\]/]+)#\d+\|\(([-\d.]+),([-\d.]+),([-\d.]+),[-\d.]+\)\|[^\]]*\] ";
    let cast_re = regex::Regex::new(&format!(
        r"{player}\[=\] \[[^\]]*\{{(\d+)\}}\] \[Event \{{\d+\}}: AbilityActivate "
    ))
    .expect("regex");
    let target_re = regex::Regex::new(&format!(
        r"{player}\[[^@\[\]|][^\[\]|]*? \{{(\d+)\}}:\d+\|\(([-\d.]+),([-\d.]+),([-\d.]+),[-\d.]+\)\|[^\]]*\] \[\] \[Event \{{\d+\}}: TargetSet "
    ))
    .expect("regex");
    let player_re = regex::Regex::new(player).expect("regex");
    let pos = |m: &regex::Captures, at: usize| {
        [at, at + 1, at + 2].map(|i| m[i].parse::<f64>().expect("position"))
    };
    let mut out = Vec::new();
    for file in combat_logs() {
        let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let text = String::from_utf8_lossy(&bytes);
        let name = file.display().to_string();
        let mut area = None;
        for (n, line) in text.lines().enumerate() {
            if let Some(m) = area_re.captures(line) {
                area = m[1].parse().ok();
            }
            let Some(a) = area else { continue };
            let (m, kind) = if let Some(m) = cast_re.captures(line) {
                let kind = LogLine::Cast(m[6].parse().expect("cast id"), pos(&m, 3));
                (m, kind)
            } else if let Some(m) = target_re.captures(line) {
                let kind = LogLine::TargetSet(m[6].parse().expect("npc id"), pos(&m, 7));
                (m, kind)
            } else if line.contains(CONVERSATION_START) {
                let Some(m) = player_re.captures(line) else {
                    continue;
                };
                let kind = LogLine::Conversation(pos(&m, 3));
                (m, kind)
            } else {
                continue;
            };
            out.push((name.clone(), n + 1, a, stamp(&m[1]), m[2].to_string(), kind));
        }
    }
    out
}

/// A granting target: log area id, the id the log prints, fqn, position.
type Target = (i64, i64, String, [f64; 3]);

fn targets(conn: &Connection, sql: &str) -> Vec<Target> {
    let mut stmt = conn.prepare(sql).expect("targets query");
    stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            [r.get(3)?, r.get(4)?, r.get(5)?],
        ))
    })
    .expect("target rows")
    .map(|r| r.expect("target row"))
    .collect()
}

/// The non-codex script actions that grant something.
const GRANT_ACTIONS: &str = "'Achievement Event', 'Grant Title', 'Give Items'";

/// What a conversation grants: its codex sources, the quest tasks its nodes
/// start or complete (a 'quest' node action's hook, named in
/// `quest_task_hooks`), and the grant actions of the scripts its nodes call.
/// Every line starts "grants ".
fn grants(conn: &Connection, convo: &str) -> Vec<String> {
    let mut answer = Vec::new();
    let mut add = |sql: &str| {
        let mut stmt = conn.prepare(sql).expect("grant query");
        let rows = stmt
            .query_map([convo], |r| r.get::<_, String>(0))
            .expect("grant rows");
        answer.extend(rows.map(|r| r.expect("grant row")));
    };
    add(
        "SELECT 'grants codex ' || codex_fqn || ' via ' || source_fqn || ' ' || via \
         FROM codex_sources WHERE source_kind = 'conversation' AND source_fqn = ?1",
    );
    add(
        "SELECT DISTINCT 'grants quest ' || a.quest_fqn || ' node ' || a.node_id || \
                CASE WHEN h.step IS NULL THEN ' advances it' \
                     WHEN h.step = 0 THEN ' starts it' \
                     ELSE ' completes task (' || h.branch || ',' || h.step || ',' || h.task \
                          || ') ''' || COALESCE(t.text, '') || '''' END \
         FROM conversation_node_actions a \
         LEFT JOIN quest_task_hooks h ON h.quest_fqn = a.quest_fqn AND h.hook_id = a.task_hook_id \
         LEFT JOIN quest_tasks t ON t.quest_fqn = h.quest_fqn AND t.branch = h.branch \
           AND t.step = h.step AND t.task = h.task \
         WHERE a.conversation_fqn = ?1 AND a.kind = 'quest' \
         ORDER BY a.node_id, h.branch, h.step, h.task",
    );
    add(&format!(
        "SELECT DISTINCT 'grants ' || s.action || ' ' || COALESCE(s.target_fqn, s.value) \
                || ' via ' || s.script_fqn || ' ' || s.handler \
         FROM conversation_node_actions a \
         JOIN script_actions s ON s.script_fqn = a.script_fqn AND s.handler = a.handler \
         WHERE a.conversation_fqn = ?1 AND a.kind = 'script' AND s.action IN ({GRANT_ACTIONS})"
    ));
    answer
}

/// What a placeable is and what using it triggers, from `placeable_triggers`,
/// then what each conversation it plays grants.
fn placeable_answer(conn: &Connection, placeable: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT kind, target_fqn, detail FROM placeable_triggers WHERE placeable_fqn = ?1")
        .expect("placeable triggers query");
    let triggers: Vec<(String, String, String)> = stmt
        .query_map([placeable], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("placeable trigger rows")
        .map(|r| r.expect("placeable trigger row"))
        .collect();
    let mut answer = Vec::new();
    let mut convos: Vec<&str> = Vec::new();
    for (kind, target, detail) in &triggers {
        if kind == "codex" || detail.starts_with("Grant Codex ") {
            answer.push(format!("grants codex {target} ({detail})"));
        } else {
            answer.push(format!("triggers {kind} {target} ({detail})"));
        }
        let plays = kind == "conversation" || detail.starts_with("Play Conversation ");
        if plays && !convos.contains(&target.as_str()) {
            convos.push(target);
        }
    }
    if triggers.is_empty() {
        answer.push("triggers nothing the build shows".to_string());
    }
    for c in convos {
        answer.extend(grants(conn, c));
    }
    answer
}

#[given("a build holding a lore placeable that grants a codex entry, placed in an area")]
fn lore_placeable_placed(world: &mut World) {
    // Lore objects ('Examining...') that name a codex entry, placed where a
    // log area can name them.
    let found = targets(
        spice(world),
        "SELECT t.log_area_id, t.use_ability_log_id, t.placeable_fqn, t.log_x, t.log_y, t.log_z \
         FROM log_use_targets t JOIN placeable_grants g ON g.placeable_fqn = t.placeable_fqn \
         WHERE g.use_ability_fqn LIKE 'abl.placeables.examine.%' AND t.codex_fqn IS NOT NULL \
           AND t.log_x IS NOT NULL",
    );
    assert!(
        !found.is_empty(),
        "no placed lore placeable naming a codex entry"
    );
    world.targets = found;
}

#[given(
    "a combat-log line where the player casts 'Examining...' at a position beside that placeable in that area"
)]
fn examining_line(world: &mut World) {
    for (file, n, area, _, _, line) in log_lines() {
        let LogLine::Cast(cast, p) = line else {
            continue;
        };
        if let Some((_, _, fqn, _)) = world
            .targets
            .iter()
            .find(|(a, id, _, q)| *a == area && id >> 32 == cast >> 32 && dist(*q, p) <= REACH)
        {
            println!("c9 use: {file} line {n} area {area} cast {cast} at {p:?} beside {fqn}");
            world.subject = fqn.clone();
            (world.log_area_id, world.cast_id, world.positions) = (area, cast, vec![p]);
            return;
        }
    }
    panic!("no logged 'Examining...' cast beside a codex-granting lore placeable");
}

/// A resolved use: placeable, codex, conversation, script, distance.
type Use = (String, Option<String>, Option<String>, Option<String>, f64);

/// The `log_use_targets` row of the log area and cast id nearest the logged
/// position, within reach.
fn use_target(conn: &Connection, area: i64, cast: i64, p: [f64; 3]) -> Option<Use> {
    let mut stmt = conn
        .prepare(
            "SELECT placeable_fqn, codex_fqn, conversation_fqn, script_fqn, log_x, log_y, log_z \
             FROM log_use_targets WHERE log_area_id = ?1 AND use_ability_log_id >> 32 = ?2 >> 32 \
               AND log_x IS NOT NULL",
        )
        .expect("use target query");
    let nearest: Option<Use> = stmt
        .query_map(rusqlite::params![area, cast], |r| {
            let q = [r.get(4)?, r.get(5)?, r.get(6)?];
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, dist(p, q)))
        })
        .expect("use target rows")
        .map(|r| r.expect("use target row"))
        .filter(|u| u.4 <= REACH)
        .min_by(|a, b| a.4.total_cmp(&b.4));
    nearest
}

#[when("kessel's agent asks what that use granted")]
fn ask_use(world: &mut World) {
    let (area, cast, p) = (world.log_area_id, world.cast_id, world.positions[0]);
    world.answer = resolve_use(spice(world), area, cast, p);
    println!("c9 use answer: {:?}", world.answer);
}

/// What a logged use cast used and what using it triggers: the nearest
/// placeable of the area and cast id within reach, and `placeable_answer`.
fn resolve_use(conn: &Connection, area: i64, cast: i64, p: [f64; 3]) -> Vec<String> {
    match use_target(conn, area, cast, p) {
        Some((fqn, _, _, _, d)) => {
            let mut answer = vec![format!("placeable {fqn} at {d:.2}")];
            answer.extend(placeable_answer(conn, &fqn));
            answer
        }
        None => vec!["nothing that grants stands within reach".to_string()],
    }
}

#[then("the answer names the placeable and its codex entry")]
fn names_placeable_codex(world: &World) {
    let placeable = format!("placeable {} ", world.subject);
    assert!(
        world.answer.iter().any(|a| a.starts_with(&placeable))
            && world
                .answer
                .iter()
                .any(|a| a.starts_with("grants codex cdx.")),
        "the use beside {} granted no codex entry: {:?}",
        world.subject,
        world.answer
    );
}

#[given("a build holding an NPC whose conversation grants a codex entry, placed in an area")]
fn codex_npc_placed(world: &mut World) {
    let found = targets(
        spice(world),
        "SELECT DISTINCT t.log_area_id, t.owner_log_id, t.owner_fqn, t.log_x, t.log_y, t.log_z \
         FROM log_conversation_targets t \
         JOIN codex_sources s ON s.source_kind = 'conversation' \
           AND s.source_fqn = t.conversation_fqn \
         WHERE t.owner_kind = 'npc' AND t.owner_log_id IS NOT NULL AND t.log_x IS NOT NULL",
    );
    assert!(
        !found.is_empty(),
        "no placed NPC whose conversation grants a codex entry"
    );
    world.targets = found;
}

/// A logged conversation start: file, line number, log area id, the
/// player's position, the NPC (log id, position) the player's TargetSet named
/// within 1 s before, and the player's casts at that spot since the last
/// conversation began, latest first.
type Conversation = (
    String,
    usize,
    i64,
    [f64; 3],
    Option<(i64, [f64; 3])>,
    Vec<(i64, [f64; 3])>,
);

fn conversations() -> Vec<Conversation> {
    let mut last: Option<(String, f64, i64, [f64; 3])> = None;
    let mut casts: Vec<(String, i64, [f64; 3])> = Vec::new();
    let mut out = Vec::new();
    for (file, n, area, t, player, line) in log_lines() {
        match line {
            LogLine::TargetSet(id, q) => last = Some((player, t, id, q)),
            LogLine::Cast(id, q) => casts.push((player, id, q)),
            LogLine::Conversation(p) => {
                let fresh = last
                    .as_ref()
                    .filter(|(who, at, _, _)| *who == player && (0.0..=1.0).contains(&(t - at)))
                    .map(|(_, _, id, q)| (*id, *q));
                let here = casts
                    .drain(..)
                    .rev()
                    .filter(|(who, _, q)| *who == player && dist(*q, p) <= REACH)
                    .map(|(_, id, q)| (id, q))
                    .collect();
                out.push((file, n, area, p, fresh, here));
            }
        }
    }
    out
}

#[given(
    "a combat-log line where the player enters a conversation at a position beside that NPC in that area"
)]
fn conversation_line(world: &mut World) {
    for (file, n, area, p, target, casts) in conversations() {
        let Some((id, q)) = target else { continue };
        if let Some((_, _, fqn, _)) = world
            .targets
            .iter()
            .find(|(a, owner, _, s)| *a == area && owner >> 32 == id >> 32 && dist(*s, q) <= REACH)
        {
            println!(
                "c9 conversation: {file} line {n} area {area} at {p:?}, target {id} beside {fqn}"
            );
            world.subject = fqn.clone();
            (world.log_area_id, world.positions, world.target_set) = (area, vec![p], target);
            world.casts = casts;
            return;
        }
    }
    panic!("no logged conversation with an NPC whose conversation grants a codex entry");
}

#[when("kessel's agent asks what that conversation granted")]
fn ask_conversation(world: &mut World) {
    let (area, p, target) = (world.log_area_id, world.positions[0], world.target_set);
    let casts = world.casts.clone();
    let answer = resolve_conversation(spice(world), area, p, target, &casts);
    println!("c9 conversation answer: {answer:?}");
    world.answer = answer;
}

/// What a logged conversation start is and grants: the NPC its fresh
/// TargetSet named; else the conversation-owning placeable a use cast at the
/// spot landed on; else the conversation owner nearest the player; else the
/// conversation that completes the used placeable's quest task.
fn resolve_conversation(
    conn: &Connection,
    area: i64,
    p: [f64; 3],
    target: Option<(i64, [f64; 3])>,
    casts: &[(i64, [f64; 3])],
) -> Vec<String> {
    type Owner = (Option<i64>, String, String, String, [f64; 3]);
    let mut stmt = conn
        .prepare(
            "SELECT owner_log_id, owner_kind, owner_fqn, conversation_fqn, log_x, log_y, log_z \
             FROM log_conversation_targets WHERE log_area_id = ?1 AND log_x IS NOT NULL",
        )
        .expect("conversation target query");
    let rows: Vec<Owner> = stmt
        .query_map([area], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                [r.get(4)?, r.get(5)?, r.get(6)?],
            ))
        })
        .expect("conversation target rows")
        .map(|r| r.expect("conversation target row"))
        .collect();
    drop(stmt);
    // The NPC the TargetSet named, at its placement nearest where the log
    // shows it; else the placeable a use cast at this spot landed on, when it
    // owns a conversation; else whatever owns a conversation nearest the player.
    let nearest = |at: [f64; 3], keep: &dyn Fn(&Owner) -> bool, reach: f64| {
        rows.iter()
            .filter(|r| keep(r))
            .map(|r| (r, dist(at, r.4)))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    };
    let used = || {
        casts.iter().find_map(|(cast, q)| {
            let (fqn, ..) = use_target(conn, area, *cast, *q)?;
            nearest(*q, &|r| r.2 == fqn, REACH)
        })
    };
    let found = match target {
        Some((id, q)) => nearest(
            q,
            &|r| r.0.is_some_and(|o| o >> 32 == id >> 32),
            f64::INFINITY,
        ),
        None => used().or_else(|| nearest(p, &|_| true, REACH)),
    };
    match found {
        Some(((_, kind, owner, convo, _), d)) => {
            let mut answer = vec![format!("{kind} {owner} conversation {convo} at {d:.2}")];
            answer.extend(grants(conn, convo));
            answer
        }
        None => casts
            .iter()
            .find_map(|(cast, q)| {
                let (placeable, ..) = use_target(conn, area, *cast, *q)?;
                quest_conversation(conn, &placeable)
            })
            .unwrap_or_else(|| vec!["nothing that grants stands within reach".to_string()]),
    }
}

/// The conversation that completes the quest task a used placeable belongs
/// to: the task naming it (or its spawner) in qstTaskInteractionObjects, and
/// the conversation whose quest node action names that quest and the hook
/// id of that task, or else of a task of the next step. With no single such
/// conversation, the quest and its conversations, marked unresolved.
fn quest_conversation(conn: &Connection, placeable: &str) -> Option<Vec<String>> {
    let rows = |sql: &str| -> Vec<(String, i64, String)> {
        let mut stmt = conn.prepare(sql).expect("quest conversation query");
        stmt.query_map([placeable], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("quest conversation rows")
            .map(|r| r.expect("quest conversation row"))
            .collect()
    };
    // (quest, steps past the placeable's step, conversation)
    let completing = rows(
        "SELECT DISTINCT o.quest_fqn, h.step - o.step, a.conversation_fqn \
         FROM quest_task_objects o \
         JOIN quest_task_hooks h ON h.quest_fqn = o.quest_fqn AND h.branch = o.branch \
           AND h.step IN (o.step, o.step + 1) \
         JOIN conversation_node_actions a ON a.kind = 'quest' \
           AND a.quest_fqn = o.quest_fqn AND a.task_hook_id = h.hook_id \
         WHERE o.object_fqn = ?1 \
            OR o.object_fqn IN (SELECT spawner_fqn FROM placements WHERE entity_fqn = ?1)",
    );
    if let Some(nearest) = completing.iter().map(|c| c.1).min() {
        let found: Vec<&(String, i64, String)> =
            completing.iter().filter(|c| c.1 == nearest).collect();
        if let [(quest, _, convo)] = found.as_slice() {
            let mut answer = vec![format!(
                "placeable {placeable} quest {quest} conversation {convo}"
            )];
            answer.extend(placeable_answer(conn, placeable));
            answer.extend(grants(conn, convo));
            return Some(answer);
        }
    }
    let quests = rows(
        "SELECT DISTINCT o.quest_fqn, 0, r.cnv_fqn FROM quest_task_objects o \
         JOIN conversation_quest_refs r ON r.quest_fqn = o.quest_fqn \
         WHERE o.object_fqn = ?1 \
            OR o.object_fqn IN (SELECT spawner_fqn FROM placements WHERE entity_fqn = ?1)",
    );
    let quest = quests.first()?.0.clone();
    let candidates: Vec<&str> = quests.iter().map(|q| q.2.as_str()).collect();
    Some(vec![format!(
        "placeable {placeable} quest {quest} conversation unresolved, one of: {}",
        candidates.join(", ")
    )])
}

#[then("the answer names the NPC, its conversation and the codex entry")]
fn names_npc_conversation_codex(world: &World) {
    let npc = format!("npc {} conversation cnv.", world.subject);
    assert!(
        world.answer.iter().any(|a| a.starts_with(&npc))
            && world
                .answer
                .iter()
                .any(|a| a.starts_with("grants codex cdx.")),
        "the conversation with {} granted no codex entry: {:?}",
        world.subject,
        world.answer
    );
}

// FR-KESSEL-025 c1, c2, c3: an achievement answers what each task counts,
// whether the log can show it, and the buff it requires.

/// The first achievement `sql` finds (it orders by preference), or fail.
fn pick_achievement(world: &mut World, sql: &str, what: &str) {
    let fqn: String = spice(world)
        .query_row(sql, [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{what}: {e}"));
    world.subject = fqn;
}

#[given("a build holding a kill-count achievement")]
fn kill_count_achievement(world: &mut World) {
    // A bestiary achievement: named for the kills it counts.
    pick_achievement(
        world,
        "SELECT fqn FROM achievements WHERE fqn LIKE 'ach.%.bestiary.killed_%' \
         ORDER BY fqn = 'ach.location.hutta.bestiary.killed_faathra_thugs_01' DESC, fqn LIMIT 1",
        "a bestiary achievement",
    );
}

#[given("a build holding an achievement counted only by a script event")]
fn script_event_achievement(world: &mut World) {
    pick_achievement(
        world,
        "SELECT achievement_fqn FROM achievement_task_counts GROUP BY achievement_fqn \
         HAVING SUM(kind = 'script_event') = COUNT(*) \
         ORDER BY achievement_fqn = 'ach.alliance.followers.hk_55.simulated_meatbag' DESC, \
                  achievement_fqn LIMIT 1",
        "an achievement whose tasks count only script events",
    );
}

#[given("a build holding an achievement that counts kills while a toy buff is active")]
fn toy_buff_achievement(world: &mut World) {
    // Killing jawas with the Party Jawa toy up; the buff field is not filtered on.
    pick_achievement(
        world,
        "SELECT fqn FROM achievements \
         WHERE fqn = 'ach.location.tatooine.fun.kill_jawas_with_jawa'",
        "ach.location.tatooine.fun.kill_jawas_with_jawa",
    );
}

#[when("kessel's agent asks for that achievement")]
fn ask_achievement(world: &mut World) {
    let fqn = world.subject.clone();
    let conn = spice(world);
    let mut answer = Vec::new();
    let mut rules = conn
        .prepare(
            "SELECT task_index, count_needed, log_visible, log_reason, \
                    buff_effect_fqn, buff_log_id, buff_on \
             FROM achievement_task_rules WHERE achievement_fqn = ?1 ORDER BY task_index",
        )
        .expect("rules query");
    let rows = rules
        .query_map([&fqn], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })
        .expect("rule rows");
    for row in rows {
        let (task, count, visible, reason, effect, log_id, on) = row.expect("rule row");
        if let Some(count) = count {
            answer.push(format!("task {task} needs {count}"));
        }
        answer.push(match reason {
            Some(r) if !visible => format!("task {task} log cannot show: {r}"),
            _ => format!("task {task} log can show"),
        });
        if let Some(effect) = effect {
            let id = log_id.map_or_else(|| "?".into(), |i| i.to_string());
            let on = on.unwrap_or_default();
            answer.push(format!(
                "task {task} counts inside effect {effect} {{{id}}} on the {on}"
            ));
        }
    }
    // A counted NPC joins to the log through its string id: the log prints
    // string_id << 32 for it.
    let mut counts = conn
        .prepare(
            "SELECT c.task_index, c.kind, c.ref_fqn, c.event_name, c.event_code, o.string_id \
             FROM achievement_task_counts c \
             LEFT JOIN objects o ON o.fqn = c.ref_fqn AND o.is_canonical = 1 \
             WHERE c.achievement_fqn = ?1 ORDER BY c.task_index",
        )
        .expect("counts query");
    let rows = counts
        .query_map([&fqn], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .expect("count rows");
    for row in rows {
        let (task, kind, ref_fqn, name, code, string_id) = row.expect("count row");
        let kind = kind.unwrap_or_else(|| "unknown".into());
        let what = ref_fqn.or(name).unwrap_or_default();
        let code = code.unwrap_or_default();
        answer.push(match (kind.as_str(), string_id) {
            ("npc", Some(sid)) => format!(
                "task {task} counts npc {what} (log id {{{}}}) {code}",
                sid << 32
            ),
            _ => format!("task {task} counts {kind} {what} {code}"),
        });
    }
    drop(rules);
    drop(counts);
    println!("{fqn}: {answer:#?}");
    world.answer = answer;
}

#[then("each task is answered with the count it needs and the NPCs it counts")]
fn tasks_count_npcs(world: &World) {
    let tasks: Vec<&String> = world
        .answer
        .iter()
        .filter(|a| a.contains(" log "))
        .collect();
    assert!(
        !tasks.is_empty(),
        "{} has no tasks: {:?}",
        world.subject,
        world.answer
    );
    for t in tasks {
        let task = t.split(' ').nth(1).unwrap_or_default();
        let has = |what: &str| {
            world
                .answer
                .iter()
                .any(|a| a.starts_with(&format!("task {task} {what}")))
        };
        assert!(
            has("needs ") && has("counts npc "),
            "{} task {task} lacks its count or NPCs: {:?}",
            world.subject,
            world.answer
        );
    }
}

#[then("the answer says a combat log cannot show its progress and names the script event")]
fn log_cannot_show(world: &World) {
    let a = &world.answer;
    assert!(
        a.iter()
            .any(|l| l.ends_with("log cannot show: script event"))
            && !a.iter().any(|l| l.ends_with("log can show")),
        "{} is not answered as invisible to the log: {a:?}",
        world.subject
    );
    assert!(
        a.iter().any(|l| l.contains(" counts script_event ")),
        "{} names no script event: {a:?}",
        world.subject
    );
}

#[then("the answer names the buff's effect")]
fn names_buff(world: &World) {
    assert!(
        world
            .answer
            .iter()
            .any(|a| a.contains(" counts inside effect abl.")),
        "{} names no effect: {:?}",
        world.subject,
        world.answer
    );
}

// FR-KESSEL-025 c7: hidden earnables are listed.

#[given("a build where an encounter script raises an achievement event no quest step names")]
fn encounter_event(world: &mut World) {
    // A Dynamic Encounter script raising an event from a root handler (not an
    // On Custom handler a quest step could send), counted by an achievement.
    let (script, handler, name): (String, String, String) = spice(world)
        .query_row(
            "SELECT a.script_fqn, a.handler, a.value FROM script_actions a \
             WHERE a.action = 'Achievement Event' AND a.script_fqn LIKE 'hyd.dynamic_events.%' \
               AND a.handler NOT LIKE 'On Custom%' \
               AND EXISTS (SELECT 1 FROM achievement_task_counts c WHERE c.event_name = a.value) \
             ORDER BY a.script_fqn, a.handler, a.value LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("a Dynamic Encounter script raising a counted event from a root handler");
    world.subject = format!("{script}|{handler}|{name}");
}

#[when("kessel's agent asks for hidden earnables")]
fn ask_hidden(world: &mut World) {
    let subject = world.subject.clone();
    let mut stmt = spice(world)
        .prepare(
            "SELECT script_fqn, handler, event_name, trigger, credits, achievement_fqn \
             FROM hidden_earnables ORDER BY script_fqn, handler, event_name",
        )
        .expect("hidden query");
    let rows: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{} trigger {} credits {} achievement {}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            ))
        })
        .expect("hidden rows")
        .map(|r| r.expect("hidden row"))
        .collect();
    drop(stmt);
    println!(
        "c7 {} hidden earnables; {subject}: {:#?}",
        rows.len(),
        rows.iter()
            .filter(|r| r.starts_with(&subject))
            .collect::<Vec<_>>()
    );
    world.answer = rows;
}

#[then("that event is listed with its script, its trigger and its achievement")]
fn hidden_listed(world: &World) {
    let prefix = format!("{} trigger On ", world.subject);
    assert!(
        world
            .answer
            .iter()
            .any(|a| a.starts_with(&prefix) && a.contains(" achievement ach.")),
        "{} is not listed with a trigger and an achievement",
        world.subject
    );
}

/// FR-KESSEL-025 c9 on real Tython and Hoth conversation starts: what each
/// is and what it grants. 06:19:57 and 06:20:50 are the flesh-raider corpse
/// (by position, then by its 'Opening' cast), 06:28:53 a holocall after
/// planting a surveillance device (by the quest join), 06:31:35 the datacron,
/// and Hoth 11:02:53 follows accessing a security panel (by the quest join).
/// Local only, like the `@local` scenarios: skipped unless `KESSEL_SPICE`
/// and `KESSEL_COMBAT_LOG` are set.
#[test]
fn c9_quest_task_conversations_resolve() {
    const TYTHON: &str = "combat_2026-11-09_05_45_53_516397.txt";
    let (Ok(path), Ok(_)) = (
        std::env::var("KESSEL_SPICE"),
        std::env::var("KESSEL_COMBAT_LOG"),
    ) else {
        return;
    };
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|e| panic!("open {path}: {e}"));
    let corpse = [
        "conversation cnv.location.tython.bronze.flesh_eating_baby.fleshraider_parent_corpse",
        "grants quest qst.location.tython.bronze.the_littlest_flesh_raider node 15 starts it",
        "grants quest qst.location.tython.bronze.the_littlest_flesh_raider node 22 completes task (0,2,0) 'Return to the Flesh Raider Baby'",
    ];
    let cases: [(&str, &str, &[&str]); 5] = [
        (TYTHON, "[06:19:57.859]", &corpse),
        (TYTHON, "[06:20:50.198]", &corpse),
        (
            TYTHON,
            "[06:28:53.886]",
            &[
                "conversation cnv.location.tython.class.jedi_knight_new.master_orgus_din",
                "grants quest qst.location.tython.class.jedi_knight.enemy_force node 209 completes task (0,6,0) 'Use Your Personal Holocom'",
            ],
        ),
        (
            TYTHON,
            "[06:31:35.619]",
            &[
                "conversation cnv.location.tython.data_holocron.datacron_b_tython",
                "grants quest qst.location.tython.data_holocron.tyt_willpower_i node 16 starts it",
            ],
        ),
        (
            "combat_2026-10-31_00_33_45_643179.txt",
            "[11:02:53.428]",
            &[
                "conversation cnv.location.hoth.world.imperial.breaking_the_maw.fixer_308_holo",
                "grants quest qst.location.hoth.world.imperial.breaking_the_maw node 68 completes task (1,2,0) 'Find information on White Maw holdings'",
            ],
        ),
    ];
    let all = conversations();
    for (file, at, expected) in cases {
        let (_, n, area, p, target, casts) = all
            .iter()
            .find(|c| {
                c.0.ends_with(file)
                    && std::fs::read(&c.0)
                        .map(|b| {
                            String::from_utf8_lossy(&b)
                                .lines()
                                .nth(c.1 - 1)
                                .is_some_and(|l| l.starts_with(at))
                        })
                        .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("no conversation start {at} in {file}"));
        let answer = resolve_conversation(&conn, *area, *p, *target, casts);
        println!("c9 {file} line {n} {at}: {answer:?}");
        for e in expected {
            assert!(
                answer.iter().any(|a| a.contains(e)),
                "{file} {at}: no {e:?} in {answer:?}"
            );
        }
    }
}

/// FR-KESSEL-025 c9 on use casts whose placeable names no codex entry and no
/// conversation: what each is and what using it advances, from
/// `placeable_triggers`. Local only, like the `@local` scenarios.
#[test]
fn c9_use_casts_answer_what_the_placeable_triggers() {
    let (Ok(path), Ok(_)) = (
        std::env::var("KESSEL_SPICE"),
        std::env::var("KESSEL_COMBAT_LOG"),
    ) else {
        return;
    };
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|e| panic!("open {path}: {e}"));
    const GRANT_KILL: &str =
        " (Grant Codex (hyd.location.flashpoint.raid.eternity_vault.phase.raid_events On Custom:Grant_Kill))";
    let ev = |codex: &str| {
        format!("grants codex cdx.achievements.flashpoint.the_eternity_vault.{codex}{GRANT_KILL}")
    };
    let cases: [(&str, &str, &str, Vec<String>); 4] = [
        (
            "combat_2026-11-09_05_45_53_516397.txt",
            "[06:28:45.782]",
            "placeable plc.location.tython.class.jedi_knight.enemy_force.surviellance_device_2 ",
            vec!["triggers quest_task qst.location.tython.class.jedi_knight.enemy_force (task (0,5,1) 'Plant a Device in Sector 02'".into()],
        ),
        (
            "combat_2026-10-31_20_43_20_205597.txt",
            "[21:12:55.889]",
            "placeable plc.location.hutta.class.spy.the_new_truth.security_camera ",
            vec!["triggers quest_task qst.location.hutta.bonus.storm_the_grounds_of_fa_athra_s_palace (task (0,1,0) 'Disable Fa'athra Security Cameras'".into()],
        ),
        // A task that names only the core's spawner.
        (
            "combat_2026-05-02_01_10_39_857937.txt",
            "[02:14:37.690]",
            "placeable plc.location.flashpoint.shared.directive_7.event.zadd_core.mentor_processing_core_2 ",
            vec!["triggers quest_task qst.location.flashpoint.shared.directive_7.directive_7 (task (0,11,0) 'Defeat Mentor'".into()],
        ),
        // Codex entries reached through Execute Hydra Script into another
        // handler (Grant_Kill) of a script already on the call path.
        (
            "combat_2026-06-02_15_47_03_587192.txt",
            "[18:58:24.878]",
            "placeable plc.location.flashpoint.raid.eternity_vault.puzzle.rakata_obelisk.confirm_button_1 ",
            vec![ev("puzzle"), ev("hardmode.puzzle"), ev("nightmare.puzzle")],
        ),
    ];
    let lines = log_lines();
    for (file, at, placeable, triggers) in cases {
        let (_, n, area, _, _, line) = lines
            .iter()
            .find(|l| {
                l.0.ends_with(file)
                    && matches!(l.5, LogLine::Cast(..))
                    && std::fs::read(&l.0)
                        .map(|b| {
                            String::from_utf8_lossy(&b)
                                .lines()
                                .nth(l.1 - 1)
                                .is_some_and(|x| x.starts_with(at))
                        })
                        .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("no cast {at} in {file}"));
        let LogLine::Cast(cast, p) = line else {
            unreachable!("matched a cast")
        };
        let answer = resolve_use(&conn, *area, *cast, *p);
        println!("c9 {file} line {n} {at}: {answer:?}");
        assert!(
            answer.iter().any(|a| a.starts_with(placeable)),
            "{file} {at}: {answer:?}"
        );
        for t in &triggers {
            assert!(
                answer.iter().any(|a| a.starts_with(t.as_str())),
                "{file} {at}: no {t:?} in {answer:?}"
            );
        }
    }
}

scenarios!(
    "tests",
    tags = "@smoke or @criterion-FR-KESSEL-025-c1 or @criterion-FR-KESSEL-025-c2 \
            or @criterion-FR-KESSEL-025-c3 or @criterion-FR-KESSEL-025-c4 \
            or @criterion-FR-KESSEL-025-c5 or @criterion-FR-KESSEL-025-c6 \
            or @criterion-FR-KESSEL-025-c7 or @criterion-FR-KESSEL-025-c8 \
            or @criterion-FR-KESSEL-025-c9 \
            or @criterion-FR-KESSEL-025-c10",
    fixtures = [world: World]
);
