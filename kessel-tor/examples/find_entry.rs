//! Find a path in a game's archives and print its size: a live check that the
//! hash convention matches the real files.
//!
//! ```text
//! cargo run -p kessel-tor --example find_entry -- <assets_dir> <path>
//! ```

#![allow(clippy::print_stdout, reason = "an example reports to the terminal")]

use kessel_tor::{Archive, filename_hash};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result {
    let args: Vec<String> = std::env::args().collect();
    let [_, assets, path] = args.as_slice() else {
        return Err("usage: find_entry <assets_dir> <path>".into());
    };
    let hash = filename_hash(path);
    for item in std::fs::read_dir(assets)? {
        let tor = item?.path();
        if tor.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let mut ar = Archive::open(&tor)?;
        if let Some(e) = ar.find(hash).cloned() {
            let bytes = ar.read(&e)?;
            println!(
                "{path} in {}: {} bytes (stored {})",
                tor.display(),
                bytes.len(),
                e.stored_len
            );
            return Ok(());
        }
    }
    Err(format!("{path} not found").into())
}
