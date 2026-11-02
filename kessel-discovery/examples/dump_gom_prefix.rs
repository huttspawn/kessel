//! Decode every GOM object whose FQN starts with a prefix, straight from the
//! archives, including prefixes the extraction allowlist drops.
//!
//! Usage: dump_gom_prefix <assets_dir> <hashes_filename.txt> <fqn_prefix> <out.json>
//!
//! Writes a JSON object keyed by FQN; each value is the decoded field tree, or
//! `{"error": ...}` when the walk stops on an unsupported tag.

use anyhow::Result;
use kessel::gom_reader::{read_object_fields, GomValue};
use kessel::hash::HashDictionary;
use kessel::myp::Archive;
use kessel::pbuk;
use serde_json::{json, Map, Value};

fn render(v: &GomValue) -> Value {
    match v {
        GomValue::Null => Value::Null,
        GomValue::U64(n) => json!(n),
        GomValue::I64(n) => json!(n),
        GomValue::Bool(b) => json!(b),
        GomValue::F32(f) => json!(f),
        GomValue::Enum(i) => json!({ "enum": i }),
        GomValue::Str(s) => json!(s),
        GomValue::List(l) => Value::Array(l.iter().map(render).collect()),
        GomValue::Map(m) => Value::Array(
            m.iter()
                .map(|(k, v)| json!({ "key": render(k), "value": render(v) }))
                .collect(),
        ),
        GomValue::Embedded(fields) => Value::Object(
            fields
                .iter()
                .map(|(id, v)| (format!("{id:#018x}"), render(v)))
                .collect(),
        ),
        GomValue::ClassRef(id) => json!({ "class": format!("{id:#018x}") }),
        GomValue::Vector3(v) => json!(v),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, assets, hash_file, prefix, out] = args.as_slice() else {
        anyhow::bail!("usage: dump_gom_prefix <assets_dir> <hashes> <fqn_prefix> <out.json>");
    };
    let mut hashes = HashDictionary::new();
    hashes.load(hash_file)?;
    let mut found = Map::new();
    for entry in std::fs::read_dir(assets)? {
        let tor = entry?.path();
        if tor.extension().is_none_or(|x| x != "tor") {
            continue;
        }
        let Ok(mut archive) = Archive::open(&tor) else {
            continue;
        };
        let entries: Vec<_> = archive.entries()?.cloned().collect();
        for e in &entries {
            // DUMP_ALL_ENTRIES=1 sniffs every entry for PBUK magic instead of
            // trusting the dictionary's /buckets/ paths (which can be stale).
            let sniff_all = std::env::var_os("DUMP_ALL_ENTRIES").is_some();
            if e.compressed_size == 0
                || (!sniff_all
                    && !hashes
                        .get(e.filename_hash)
                        .is_some_and(|p| p.contains("/buckets/")))
            {
                continue;
            }
            let data = archive.read_entry(e)?;
            if !pbuk::is_pbuk(&data) {
                continue;
            }
            for obj in pbuk::parse(&data)? {
                if !obj.fqn.starts_with(prefix.as_str()) {
                    continue;
                }
                let value = match read_object_fields(&obj.payload) {
                    Ok(v) => render(&v),
                    Err(err) => json!({
                        "error": format!("{err:#}"),
                        "bytes": obj.payload.len(),
                        "head": obj.payload.iter().take(512).map(|b| format!("{b:02x}")).collect::<String>(),
                    }),
                };
                found.insert(obj.fqn.clone(), value);
            }
        }
    }
    eprintln!("{} objects match {prefix}", found.len());
    std::fs::write(out, serde_json::to_string_pretty(&Value::Object(found))?)?;
    Ok(())
}
