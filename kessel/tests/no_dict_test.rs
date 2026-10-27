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

#[test]
fn rerun_into_a_pre_295_database_rebuilds_fx_specs() {
    let input = scratch_dir("old-db-input");
    let out_dir = scratch_dir("old-db-output");
    let db_path = out_dir.join("spice.sqlite");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE fx_specs (fqn TEXT PRIMARY KEY, node_classes_json TEXT NOT NULL, \
         raw_xml TEXT NOT NULL);",
    )
    .unwrap();
    drop(conn);

    let output = Command::new(env!("CARGO_BIN_EXE_kessel"))
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&db_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "kessel failed on a pre-#295 database: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let has_file_hash: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM pragma_table_info('fx_specs') WHERE name = 'file_hash'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(has_file_hash, "fx_specs kept its pre-#295 schema");

    let _ = std::fs::remove_dir_all(&input);
    let _ = std::fs::remove_dir_all(&out_dir);
}
