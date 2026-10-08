//! Step code for `tests/hash.feature`. Compiled into the acceptance harness
//! (`tests/acceptance.rs`), which runs the scenarios.

use kessel::hash::compute_stable_id;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{given, then, when, ScenarioState};
use sha2::{Digest, Sha256};

#[derive(Default, ScenarioState)]
pub struct HashWorld {
    fqn: Slot<String>,
    stable_id: Slot<String>,
}

#[fixture]
pub fn hash_world() -> HashWorld {
    HashWorld::default()
}

#[given("an object FQN")]
fn an_object_fqn(hash_world: &HashWorld) {
    hash_world
        .fqn
        .set("itm.gen.lots.blue.random_mod_box".to_string());
}

#[when("its stable id is computed")]
fn stable_id_is_computed(hash_world: &HashWorld) {
    let fqn = hash_world.fqn.get().expect("FQN set by the Given step");
    hash_world.stable_id.set(compute_stable_id(&fqn));
}

#[then("the stable id is the first 16 hex characters of the SHA-256 digest of the FQN")]
fn stable_id_is_truncated_sha256(hash_world: &HashWorld) {
    let fqn = hash_world.fqn.get().expect("FQN set by the Given step");
    let stable_id = hash_world
        .stable_id
        .get()
        .expect("stable id set by the When step");
    let full_digest = hex::encode(Sha256::digest(fqn.as_bytes()));
    assert_eq!(stable_id.len(), 16);
    assert_eq!(stable_id, full_digest[..16]);
}
