//! Runs every `.feature` file under this crate's `tests/` directory.
//!
//! A scenario with no matching step code fails `cargo test`, and the failure
//! names the scenario.

use kessel::hash::compute_icon_id;
use rstest_bdd_macros::{given, scenarios, then, when};

#[derive(Default)]
struct World {
    name: String,
    icon_id: String,
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

scenarios!("tests", fixtures = [world: World]);
