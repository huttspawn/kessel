//! The report of what an extraction did not read (FR-KESSEL-026): `field_facts`, the type tag
//! client.gom declares for each field id and the count of each wire tag read, and `unknowns`,
//! one row per finding of each check. A check with nothing to report writes one `clean` row; a
//! check that cannot run writes one `failed` row saying why, and the extraction goes on.

use super::Database;
use crate::unknowns::{path_hash, BuildFacts};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags, Transaction};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// One finding: what it is about, what was found, and a count where one applies.
struct Finding {
    subject: String,
    detail: String,
    count: Option<u64>,
}

impl Finding {
    fn new(subject: impl Into<String>, detail: impl Into<String>, count: Option<u64>) -> Self {
        Self {
            subject: subject.into(),
            detail: detail.into(),
            count,
        }
    }
}

/// Field id (hex) to its declared tag and the count of each wire tag read.
type Facts = BTreeMap<String, (Option<u8>, BTreeMap<u8, u64>)>;

pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS field_facts (
            field_id     TEXT NOT NULL,
            declared_tag INTEGER,
            read_tag     INTEGER NOT NULL,
            count        INTEGER NOT NULL,
            PRIMARY KEY (field_id, read_tag)
        );
        CREATE TABLE IF NOT EXISTS unknowns (
            check_name TEXT NOT NULL,
            status     TEXT NOT NULL CHECK (status IN ('found', 'clean', 'failed')),
            subject    TEXT,
            detail     TEXT NOT NULL,
            count      INTEGER
        );",
    )?;
    Ok(())
}

impl Database {
    /// Write the run's report of what it did not read, from what the sweep saw and the
    /// output's own objects. `previous` names an earlier extraction to compare field facts
    /// against; `hashes` names the paths it can.
    pub fn write_unknowns_report(
        &self,
        facts: &BuildFacts,
        hashes: &crate::hash::HashDictionary,
        previous: Option<&Path>,
    ) -> Result<()> {
        self.with_tx(|tx| {
            tx.execute_batch("DELETE FROM field_facts; DELETE FROM unknowns;")?;
            let fields = field_facts(tx, facts)?;
            let mut insert = tx.prepare(
                "INSERT INTO field_facts (field_id, declared_tag, read_tag, count) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (id, (declared, tags)) in &fields {
                for (tag, n) in tags {
                    insert.execute(params![id, declared, tag, n])?;
                }
            }
            drop(insert);
            let declared = match &facts.declared {
                Some(Ok(_)) => Ok(fields.len()),
                Some(Err(e)) => Err(format!(
                    "{} not readable: {e}",
                    crate::gom_schema::CLIENT_GOM_PATH
                )),
                None => Err(format!("{} not found", crate::gom_schema::CLIENT_GOM_PATH)),
            };
            let checks: [(&str, Result<Vec<Finding>>); 6] = [
                ("field_facts", recorded(declared.clone())),
                ("field_changes", field_changes(&fields, previous)),
                ("declared_tag", declared_tag(&fields, declared.is_ok())),
                ("string_tables", string_tables(tx, facts, hashes)),
                ("left_out", Ok(left_out(facts))),
                ("area_files", area_files(tx, facts, hashes)),
            ];
            let mut row = tx.prepare(
                "INSERT INTO unknowns (check_name, status, subject, detail, count) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (check, result) in checks {
                match result {
                    Ok(found) if found.is_empty() => {
                        row.execute(params![
                            check,
                            "clean",
                            None::<String>,
                            "found nothing",
                            None::<u64>
                        ])?;
                    }
                    Ok(found) => {
                        for f in found {
                            row.execute(params![check, "found", f.subject, f.detail, f.count])?;
                        }
                    }
                    Err(e) => {
                        row.execute(params![
                            check,
                            "failed",
                            None::<String>,
                            format!("{e:#}"),
                            None::<u64>
                        ])?;
                    }
                }
            }
            Ok(())
        })
    }
}

/// The field facts check reports how many fields it recorded; it cannot run without the
/// declared tags.
fn recorded(declared: Result<usize, String>) -> Result<Vec<Finding>> {
    match declared {
        Ok(n) => Ok(vec![Finding::new(
            "field_facts",
            "fields recorded",
            Some(n as u64),
        )]),
        Err(e) => bail!("declared tags not read: {e}; field_facts holds the tags read only"),
    }
}

/// Every field value the output's objects and singletons carry, plus those of the objects
/// left out, with the tag client.gom declares for the field.
fn field_facts(tx: &Transaction, facts: &BuildFacts) -> Result<Facts> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    let mut counts = facts.field_tags.clone();
    let mut stmt = tx.prepare(
        "SELECT kind, json_extract(json, '$.header_hex'), json_extract(json, '$.payload_b64') \
         FROM objects UNION ALL SELECT '', header_hex, payload_b64 FROM singletons",
    )?;
    let mut rows = stmt.query([])?;
    let mut scratch = BuildFacts::default();
    while let Some(r) = rows.next()? {
        let (kind, header, payload): (String, Option<String>, Option<String>) =
            (r.get(0)?, r.get(1)?, r.get(2)?);
        let (Some(header), Some(payload)) = (header, payload) else {
            continue;
        };
        let (Ok(header), Ok(payload)) = (hex::decode(header), BASE64.decode(payload)) else {
            continue;
        };
        scratch.note_fields(&kind, &header, &payload);
    }
    for (key, n) in scratch.field_tags {
        *counts.entry(key).or_default() += n;
    }
    let declared = facts.declared.as_ref().and_then(|d| d.as_ref().ok());
    let mut fields = Facts::new();
    for ((id, tag), n) in counts {
        let entry = fields.entry(format!("{id:016x}")).or_insert_with(|| {
            (
                declared.and_then(|d| d.get(&(id as u32)).copied()),
                BTreeMap::new(),
            )
        });
        entry.1.insert(tag, n);
    }
    Ok(fields)
}

fn tags(tags: impl IntoIterator<Item = u8>) -> String {
    let names: Vec<String> = tags.into_iter().map(|t| format!("0x{t:02x}")).collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// New and gone field ids, changed declared tags and appeared or disappeared wire tags,
/// against the field facts of an earlier extraction.
fn field_changes(fields: &Facts, previous: Option<&Path>) -> Result<Vec<Finding>> {
    let Some(path) = previous else {
        bail!("no previous extraction named (--previous)");
    };
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open {}", path.display()))?;
    let mut before = Facts::new();
    let mut stmt = conn
        .prepare("SELECT field_id, declared_tag, read_tag, count FROM field_facts")
        .with_context(|| format!("{} holds no field facts", path.display()))?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let entry = before
            .entry(r.get(0)?)
            .or_insert((r.get(1)?, BTreeMap::new()));
        entry.1.insert(r.get(2)?, r.get(3)?);
    }
    let mut found = Vec::new();
    for (id, (declared, read)) in fields {
        let Some((was, read_before)) = before.get(id) else {
            found.push(Finding::new(
                id,
                format!(
                    "new field: declared {}, read {}",
                    tags(*declared),
                    tags(read.keys().copied())
                ),
                None,
            ));
            continue;
        };
        if was != declared {
            found.push(Finding::new(
                id,
                format!("declared tag {} became {}", tags(*was), tags(*declared)),
                None,
            ));
        }
        for tag in read.keys().filter(|t| !read_before.contains_key(t)) {
            found.push(Finding::new(
                id,
                format!("read tag {} appeared", tags([*tag])),
                read.get(tag).copied(),
            ));
        }
        for tag in read_before.keys().filter(|t| !read.contains_key(t)) {
            found.push(Finding::new(
                id,
                format!("read tag {} disappeared", tags([*tag])),
                read_before.get(tag).copied(),
            ));
        }
    }
    for (id, (was, read_before)) in &before {
        if !fields.contains_key(id) {
            found.push(Finding::new(
                id,
                format!(
                    "gone field: declared {}, read {}",
                    tags(*was),
                    tags(read_before.keys().copied())
                ),
                None,
            ));
        }
    }
    Ok(found)
}

/// Every declared field with a value read under another tag.
fn declared_tag(fields: &Facts, declared_read: bool) -> Result<Vec<Finding>> {
    if !declared_read {
        bail!("the declared tags were not read (see field_facts)");
    }
    Ok(fields
        .iter()
        .filter_map(|(id, (declared, read))| {
            let declared = (*declared)?;
            read.keys().any(|t| *t != declared).then(|| {
                let shapes: Vec<String> = read
                    .iter()
                    .map(|(t, n)| format!("0x{t:02x} x{n}"))
                    .collect();
                Finding::new(
                    id,
                    format!("declared 0x{declared:02x}, read {}", shapes.join(", ")),
                    Some(read.values().sum()),
                )
            })
        })
        .collect())
}

/// Every en-us string table in the archives that the extraction did not read: neither a
/// known table nor the table of a conversation in the output.
fn string_tables(
    tx: &Transaction,
    facts: &BuildFacts,
    hashes: &crate::hash::HashDictionary,
) -> Result<Vec<Finding>> {
    let mut read = facts.tables_read.clone();
    let mut stmt = tx.prepare("SELECT DISTINCT fqn FROM objects WHERE kind = 'Conversation'")?;
    for fqn in stmt.query_map([], |r| r.get::<_, String>(0))? {
        read.insert(path_hash(&super::cnv::conversation_stb_path(&fqn?)));
    }
    Ok(facts
        .string_tables
        .iter()
        .filter(|(hash, _)| !read.contains(hash))
        .map(|(hash, archive)| {
            let subject = hashes
                .get(*hash)
                .cloned()
                .unwrap_or_else(|| format!("{hash:016x}"));
            Finding::new(subject, format!("not read; in {archive}"), None)
        })
        .collect())
}

/// Every kind and FQN prefix the extract left out, with the number of objects.
fn left_out(facts: &BuildFacts) -> Vec<Finding> {
    facts
        .left_out
        .iter()
        .map(|((kind, prefix), n)| Finding::new(prefix.as_str(), format!("kind {kind}"), Some(*n)))
        .collect()
}

/// Areas the map data names with no area file, and area files no map data names.
fn area_files(
    tx: &Transaction,
    facts: &BuildFacts,
    hashes: &crate::hash::HashDictionary,
) -> Result<Vec<Finding>> {
    let named: HashMap<u64, u64> = super::world::map_area_dirs(tx)?
        .into_iter()
        .map(|dir| {
            (
                path_hash(&format!("/resources/world/areas/{dir}/area.dat")),
                dir,
            )
        })
        .collect();
    let mut found: Vec<Finding> = named
        .iter()
        .filter(|(hash, _)| !facts.entries.contains(hash))
        .map(|(_, dir)| {
            (
                *dir,
                Finding::new(dir.to_string(), "named by the map data; no area file", None),
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect();
    let unnamed: BTreeSet<&u64> = facts
        .area_files
        .iter()
        .filter(|h| !named.contains_key(h))
        .collect();
    for hash in unnamed {
        let subject = hashes
            .get(*hash)
            .cloned()
            .unwrap_or_else(|| format!("{hash:016x}"));
        found.push(Finding::new(
            subject,
            "area file; no map data names it",
            None,
        ));
    }
    Ok(found)
}
