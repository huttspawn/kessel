//! Runs the `.feature` files under this crate's `tests/` directory.
//!
//! A scenario with no matching step code fails `cargo test`, and the failure
//! names the scenario. The tag filter below runs the scenarios that have step
//! code: the `@smoke` checks and FR-KESSEL-025 c4, c5, c9 and c10. The `@local`
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

/// The combat-log files `KESSEL_COMBAT_LOG` names (a file or a directory of
/// `.txt` logs), or skip the scenario when it is unset.
fn combat_logs() -> Vec<std::path::PathBuf> {
    let Ok(path) = std::env::var("KESSEL_COMBAT_LOG") else {
        rstest_bdd::skip!("@local: set KESSEL_COMBAT_LOG to a combat log or a directory of them");
    };
    let path = std::path::PathBuf::from(path);
    if !path.is_dir() {
        return vec![path];
    }
    let mut files: Vec<_> = std::fs::read_dir(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
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

/// The codex entries a placeable or conversation grants: its own codex
/// entry, its On Use script's Grant Codex actions and its conversation's
/// codex sources.
fn codex_grants(
    conn: &Connection,
    codex: Option<String>,
    convo: Option<&str>,
    script: Option<&str>,
) -> Vec<String> {
    let mut answer: Vec<String> = codex
        .into_iter()
        .map(|c| format!("grants codex {c}"))
        .collect();
    let mut via = |sql: &str, key: &str| {
        let mut stmt = conn.prepare(sql).expect("grant query");
        let rows = stmt
            .query_map([key], |r| {
                Ok(format!(
                    "grants codex {} via {key} {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?
                ))
            })
            .expect("grant rows");
        answer.extend(rows.map(|r| r.expect("grant row")));
    };
    if let Some(s) = script {
        via(
            "SELECT target_fqn, handler FROM script_actions \
             WHERE script_fqn = ?1 AND action = 'Grant Codex' AND target_fqn IS NOT NULL",
            s,
        );
    }
    if let Some(c) = convo {
        via(
            "SELECT codex_fqn, via FROM codex_sources \
             WHERE source_kind = 'conversation' AND source_fqn = ?1",
            c,
        );
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
    let conn = spice(world);
    world.answer = match use_target(conn, area, cast, p) {
        Some((fqn, codex, convo, script, d)) => {
            let mut answer = vec![format!("placeable {fqn} at {d:.2}")];
            answer.extend(codex_grants(
                conn,
                codex,
                convo.as_deref(),
                script.as_deref(),
            ));
            answer
        }
        None => vec!["nothing that grants stands within reach".to_string()],
    };
    println!("c9 use answer: {:?}", world.answer);
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
    type Owner = (Option<i64>, String, String, String, [f64; 3]);
    let (area, p, target) = (world.log_area_id, world.positions[0], world.target_set);
    let casts = world.casts.clone();
    let conn = spice(world);
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
    world.answer = match found {
        Some(((_, kind, owner, convo, _), d)) => {
            let mut answer = vec![format!("{kind} {owner} conversation {convo} at {d:.2}")];
            answer.extend(codex_grants(conn, None, Some(convo), None));
            answer
        }
        None => vec!["nothing that grants stands within reach".to_string()],
    };
    println!("c9 conversation answer: {:?}", world.answer);
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

scenarios!(
    "tests",
    tags = "@smoke or @criterion-FR-KESSEL-025-c4 or @criterion-FR-KESSEL-025-c5 \
            or @criterion-FR-KESSEL-025-c9 or @criterion-FR-KESSEL-025-c10",
    fixtures = [world: World]
);
