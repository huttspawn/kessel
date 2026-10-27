//! Extraction runs without a hash dictionary (#295).

use std::path::PathBuf;
use std::process::Command;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kessel-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn extraction_without_hashes_needs_no_dictionary_file() {
    let input = scratch_dir("no-dict-input");
    let out_dir = scratch_dir("no-dict-output");

    let output = Command::new(env!("CARGO_BIN_EXE_kessel"))
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(out_dir.join("out.sqlite"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "kessel failed without a dictionary: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !out_dir.join("hashes_filename.txt").exists(),
        "kessel fetched a dictionary it was not given"
    );

    let _ = std::fs::remove_dir_all(&input);
    let _ = std::fs::remove_dir_all(&out_dir);
}
