//! Runs the `.feature` files under this crate's `tests/` directory.
//!
//! A scenario with no matching step code fails `cargo test`, and the failure
//! names the scenario. The tag filter below runs the scenarios that have step
//! code: the `@smoke` checks and FR-KESSEL-025 c1 to c7 and c10. The `@local`
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

scenarios!(
    "tests",
    tags = "@smoke or @criterion-FR-KESSEL-025-c1 or @criterion-FR-KESSEL-025-c2 \
            or @criterion-FR-KESSEL-025-c3 or @criterion-FR-KESSEL-025-c4 \
            or @criterion-FR-KESSEL-025-c5 or @criterion-FR-KESSEL-025-c6 \
            or @criterion-FR-KESSEL-025-c7 \
            or @criterion-FR-KESSEL-025-c10",
    fixtures = [world: World]
);
