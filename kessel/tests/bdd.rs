//! Runs the `.feature` files under this crate's `tests/` directory.
//!
//! A scenario with no matching step code fails `cargo test`, and the failure
//! names the scenario. The tag filter below runs the scenarios that have step
//! code: the `@smoke` checks and FR-KESSEL-025 c5 and c10. The `@local`
//! scenarios read a full local extract named by `KESSEL_SPICE` and are skipped
//! when it is unset (CI never has one).

use kessel::hash::compute_icon_id;
use rstest_bdd_macros::{given, scenarios, then, when};
use rusqlite::{Connection, OpenFlags};

#[derive(Default)]
struct World {
    name: String,
    icon_id: String,
    /// The local extract, opened read-only.
    spice: Option<Connection>,
    /// The placeable or volume the scenario holds.
    subject: String,
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

// FR-KESSEL-025 c5: a granting placeable answers what and where.

#[given("a build holding a datacron placeable")]
fn datacron_placeable(world: &mut World) {
    let fqn: String = spice(world)
        .query_row(
            "SELECT g.placeable_fqn FROM placeable_grants g \
             JOIN placements p ON p.entity_fqn = g.placeable_fqn \
             WHERE g.placeable_fqn LIKE 'plc.%datacron%' \
             ORDER BY g.placeable_fqn LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("a placed datacron placeable in placeable_grants");
    world.subject = fqn;
}

#[when("kessel's agent asks for that placeable")]
fn ask_placeable(world: &mut World) {
    let fqn = world.subject.clone();
    let conn = spice(world);
    let grants: [Option<String>; 4] = conn
        .query_row(
            "SELECT use_ability_fqn, codex_fqn, conversation_fqn, script_fqn \
             FROM placeable_grants WHERE placeable_fqn = ?1",
            [&fqn],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )
        .expect("placeable_grants row");
    let mut answer: Vec<String> = ["use ability", "codex", "conversation", "script"]
        .iter()
        .zip(grants)
        .filter_map(|(what, v)| Some(format!("grants {what} {}", v?)))
        .collect();
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
    world.answer = answer;
}

#[then("the answer names what it grants")]
fn names_grant(world: &World) {
    assert!(
        world.answer.iter().any(|a| a.starts_with("grants ")),
        "{} grants nothing: {:?}",
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
    let (volume, log_area_id, lo, hi): (String, i64, [f64; 3], [f64; 3]) = spice(world)
        .query_row(
            "SELECT v.area_dir_id || '/' || v.instance_id, l.log_area_id, \
                    v.log_min_x, v.log_min_y, v.log_min_z, v.log_max_x, v.log_max_y, v.log_max_z \
             FROM trigger_volumes v \
             JOIN script_actions a ON a.script_fqn = v.script_fqn \
             JOIN area_log_ids l ON l.area_dir_id = v.area_dir_id \
             WHERE a.action = 'Grant Codex' AND a.handler = 'On Enter' \
               AND v.log_min_x IS NOT NULL \
             ORDER BY v.area_dir_id, v.instance_id LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    [r.get(2)?, r.get(3)?, r.get(4)?],
                    [r.get(5)?, r.get(6)?, r.get(7)?],
                ))
            },
        )
        .expect("a codex-granting trigger volume in an area the log can name");
    world.subject = volume;
    world.log_area_id = log_area_id;
    world.positions = vec![lo, hi];
}

#[given(
    "combat-log lines whose player positions in that area move from outside to inside the volume"
)]
fn positions_enter(world: &mut World) {
    let [lo, hi] = [world.positions[0], world.positions[1]];
    let centre = [0, 1, 2].map(|i| (lo[i] + hi[i]) / 2.0);
    let outside = [lo[0] - 20.0, centre[1], centre[2]];
    world.positions = vec![outside, centre];
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

scenarios!(
    "tests",
    tags = "@smoke or @criterion-FR-KESSEL-025-c5 or @criterion-FR-KESSEL-025-c10",
    fixtures = [world: World]
);
