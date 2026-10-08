//! Rebuild when anything under `tests/` changes. The acceptance harness
//! discovers `.feature` files at macro-expansion time; rstest-bdd tracks edits
//! to files it found but not files added later, so without this a new
//! scenario would not run until something else forced a rebuild.

fn main() {
    println!("cargo:rerun-if-changed=tests");
}
