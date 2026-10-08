//! Acceptance harness: runs every `.feature` file under `tests/` as one test
//! per scenario. Feature files mirror `src/` (`src/a/b.rs` ->
//! `tests/a/b.feature`) and their step code sits beside them in
//! `tests/a/b.rs`, declared below as a module so its steps register here.
//! A step with no matching code fails the scenario's test by name.

#[path = "hash.rs"]
mod hash;

use hash::{hash_world, HashWorld};

rstest_bdd_macros::scenarios!("tests", fixtures = [hash_world: HashWorld]);
