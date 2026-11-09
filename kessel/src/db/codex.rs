//! Codex domain: every source that grants a codex entry (FR-KESSEL-025 c4).
//!
//! A placeable grants one directly (plcCodexSpec, in `placeable_grants`).
//! Every other grant is a Hydra 'Grant Codex' action (`script_actions`), and
//! its source is whatever runs that handler: a trigger volume holding the
//! script, a conversation node's script call, a quest hook or an achievement's
//! completion hook (a Hydra reference plus a custom event), or, when nothing
//! found calls it, the script itself.

use super::world::{decoded_objects, gom_str};
use super::*;

// A quest or achievement hook's script call: a Hydra reference and the custom
// event it sends, which fires that script's 'On Custom:<event>' handler.
const HOOK_SCRIPT: u32 = 0x4fa1_4a1f;
const HOOK_EVENT: u32 = 0xcdeb_4375;

/// Every (script guid, event) a hook anywhere in `obj` calls.
fn hook_calls(obj: &crate::gom_reader::GomValue) -> Vec<(u64, String)> {
    use crate::gom_reader::GomValue;
    fn walk(v: &GomValue, out: &mut Vec<(u64, String)>) {
        match v {
            GomValue::Embedded(fields) => {
                let script = v.embedded_field(HOOK_SCRIPT).and_then(GomValue::as_i64);
                let event = v.embedded_field(HOOK_EVENT).and_then(gom_str);
                if let (Some(script), Some(event)) = (script, event) {
                    out.push((script as u64, event));
                }
                fields.iter().for_each(|(_, x)| walk(x, out));
            }
            GomValue::List(items) => items.iter().for_each(|x| walk(x, out)),
            GomValue::Map(entries) => entries.iter().for_each(|(_, x)| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(obj, &mut out);
    out
}

/// A hook that runs a script handler: (source kind, owner fqn, script fqn, handler).
type HookCall = (&'static str, String, String, String);

impl Database {
    /// Populate `codex_sources`. Run after `populate_world_areas` and
    /// `populate_conversation_grants`, whose tables this reads. Returns rows written.
    pub fn populate_codex_sources(&self) -> Result<u64> {
        self.with_tx(|tx| {
            let mut scripts = tx.prepare(
                "SELECT fqn FROM objects WHERE guid = ?1 AND kind = 'Hydra' AND is_canonical = 1",
            )?;
            let mut hooks: Vec<HookCall> = Vec::new();
            for (kind, source_kind) in [("Quest", "quest"), ("Achievement", "achievement_reward")] {
                for (fqn, obj) in decoded_objects(tx, kind)? {
                    for (guid, event) in hook_calls(&obj) {
                        let script: Option<String> = scripts
                            .query_row([format!("{guid:016X}")], |r| r.get(0))
                            .ok();
                        if let Some(script) = script {
                            hooks.push((
                                source_kind,
                                fqn.clone(),
                                script,
                                format!("On Custom:{event}"),
                            ));
                        }
                    }
                }
            }
            drop(scripts);
            insert_codex_sources(tx, &hooks)
        })
    }
}

fn insert_codex_sources(tx: &Transaction, hooks: &[HookCall]) -> Result<u64> {
    tx.execute("DELETE FROM codex_sources", [])?;
    tx.execute(
        "INSERT OR IGNORE INTO codex_sources (codex_fqn, source_kind, source_fqn, via) \
         SELECT codex_fqn, 'placeable', placeable_fqn, '' FROM placeable_grants \
         WHERE codex_fqn IS NOT NULL",
        [],
    )?;
    let grants: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT script_fqn, handler, target_fqn FROM script_actions \
             WHERE action = 'Grant Codex' AND target_fqn IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut volumes = tx.prepare(
        "SELECT area_dir_id || '/' || instance_id FROM trigger_volumes \
         WHERE script_fqn = ?1 AND ?2 NOT LIKE 'On Custom:%'",
    )?;
    let mut conversations = tx.prepare(
        "SELECT DISTINCT conversation_fqn FROM conversation_node_actions \
         WHERE kind = 'script' AND script_fqn = ?1 AND handler = ?2",
    )?;
    let mut insert = tx.prepare(
        "INSERT OR IGNORE INTO codex_sources (codex_fqn, source_kind, source_fqn, via) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (script, handler, codex) in &grants {
        let via = format!("{script} {handler}");
        let mut sources: Vec<(&str, String)> = Vec::new();
        for (kind, stmt) in [
            ("volume", &mut volumes),
            ("conversation", &mut conversations),
        ] {
            let rows = stmt.query_map(params![script, handler], |r| r.get::<_, String>(0))?;
            for row in rows {
                sources.push((kind, row?));
            }
        }
        sources.extend(
            hooks
                .iter()
                .filter(|(_, _, s, h)| s == script && h == handler)
                .map(|(kind, owner, _, _)| (*kind, owner.clone())),
        );
        if sources.is_empty() {
            sources.push(("script", script.clone()));
        }
        for (kind, source) in sources {
            insert.execute(params![codex, kind, source, via])?;
        }
    }
    drop(insert);
    tx.execute(
        "INSERT OR IGNORE INTO codex_sources (codex_fqn, source_kind, source_fqn, via) \
         SELECT fqn, 'none', '', '' FROM objects o WHERE kind = 'Codex' AND is_canonical = 1 \
         AND NOT EXISTS (SELECT 1 FROM codex_sources s WHERE s.codex_fqn = o.fqn)",
        [],
    )?;
    Ok(tx.query_row("SELECT COUNT(*) FROM codex_sources", [], |r| r.get(0))?)
}

/// Create the codex-domain tables (idempotent).
pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        r#"
            -- One row per way a codex entry is granted (FR-KESSEL-025 c4).
            -- source_kind: placeable (plcCodexSpec), volume (a trigger
            -- volume's script; source_fqn is area_dir_id/instance_id),
            -- conversation (a node's script call), quest (a quest hook's
            -- script), achievement_reward (an achievement's completion hook
            -- script), script (a handler nothing found calls), or none (no
            -- source found). via is '<script_fqn> <handler>', empty for a
            -- direct field.
            CREATE TABLE IF NOT EXISTS codex_sources (
                codex_fqn   TEXT NOT NULL,
                source_kind TEXT NOT NULL,
                source_fqn  TEXT NOT NULL,
                via         TEXT NOT NULL,
                PRIMARY KEY (codex_fqn, source_kind, source_fqn, via)
            );
            CREATE INDEX IF NOT EXISTS idx_codex_sources_source ON codex_sources(source_fqn);
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::temp_db_path;
    use crate::gom_reader::GomValue as G;

    fn obj(fields: Vec<(u32, G)>) -> G {
        G::Embedded(
            fields
                .into_iter()
                .map(|(id, v)| (0x4000_0000_0000_0000 | u64::from(id), v))
                .collect(),
        )
    }

    #[test]
    fn hook_calls_find_every_script_call_at_any_depth() {
        let call = |guid: u64, event: &str| {
            obj(vec![
                (HOOK_SCRIPT, G::U64(guid)),
                (HOOK_EVENT, G::Str(event.into())),
            ])
        };
        let quest = obj(vec![(
            0x05a2_a6d3,
            G::List(vec![obj(vec![(
                0x05a2_a6d1,
                G::List(vec![obj(vec![(0xcdf0_591a, call(7, "Found Willpower"))])]),
            )])]),
        )]);
        assert_eq!(hook_calls(&quest), [(7, "Found Willpower".to_string())]);
        // A Hydra reference with no event is no call.
        assert!(hook_calls(&obj(vec![(HOOK_SCRIPT, G::U64(7))])).is_empty());
    }

    #[test]
    fn codex_sources_answer_every_way_an_entry_is_granted() -> Result<()> {
        let path = temp_db_path("codex_sources");
        let db = Database::with_grammar(&path, None)?;
        db.init_schema()?;
        let qm = "hyd.location.data_holocron.quest_manager.qm_tyt_datacron_codex";
        db.with_tx(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO placeable_grants (placeable_fqn, codex_fqn) \
                   VALUES ('plc.lore.a', 'cdx.lore.a');
                 INSERT INTO script_actions (script_fqn, handler, ordinal, action, target_fqn) VALUES
                   ('{qm}', 'On Custom:Found Willpower', 0, 'Grant Codex', 'cdx.datacron.tython.datacron_2'),
                   ('hyd.trig', 'On Enter', 0, 'Grant Codex', 'cdx.place.b'),
                   ('hyd.talk', 'On Custom:Tell', 0, 'Grant Codex', 'cdx.persons.c'),
                   ('hyd.lone', 'On Custom:Nobody', 0, 'Grant Codex', 'cdx.lone.d'),
                   ('hyd.lone', 'On Custom:Nobody', 1, 'Grant Title', '671');
                 INSERT INTO trigger_volumes (area_dir_id, room, instance_id, script_fqn) \
                   VALUES (9, 'r', 5, 'hyd.trig');
                 INSERT INTO conversation_node_actions \
                   (conversation_fqn, node_id, kind, ordinal, script_fqn, handler) \
                   VALUES ('cnv.c', 3, 'script', 0, 'hyd.talk', 'On Custom:Tell');"
            ))?;
            for (fqn, guid) in [("cdx.lore.a", "01"), ("cdx.never.e", "02")] {
                tx.execute(
                    "INSERT INTO objects (game_id, stable_id, payload_hash, guid, template_guid, \
                       fqn, kind, version, revision, json, is_canonical) \
                     VALUES (?1, ?1, '', ?1, '', ?2, 'Codex', 0, 0, '{}', 1)",
                    params![guid, fqn],
                )?;
            }
            let hooks = [(
                "quest",
                "qst.location.tython.data_holocron.tyt_willpower_i".to_string(),
                qm.to_string(),
                "On Custom:Found Willpower".to_string(),
            )];
            insert_codex_sources(tx, &hooks)
        })?;
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT codex_fqn, source_kind, source_fqn, via FROM codex_sources ORDER BY codex_fqn",
        )?;
        let rows: Vec<[String; 4]> = stmt
            .query_map([], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))?
            .collect::<rusqlite::Result<_>>()?;
        let row = |a: &str, b: &str, c: &str, d: &str| [a, b, c, d].map(String::from);
        assert_eq!(
            rows,
            [
                row(
                    "cdx.datacron.tython.datacron_2",
                    "quest",
                    "qst.location.tython.data_holocron.tyt_willpower_i",
                    &format!("{qm} On Custom:Found Willpower")
                ),
                row(
                    "cdx.lone.d",
                    "script",
                    "hyd.lone",
                    "hyd.lone On Custom:Nobody"
                ),
                row("cdx.lore.a", "placeable", "plc.lore.a", ""),
                row("cdx.never.e", "none", "", ""),
                row(
                    "cdx.persons.c",
                    "conversation",
                    "cnv.c",
                    "hyd.talk On Custom:Tell"
                ),
                row("cdx.place.b", "volume", "9/5", "hyd.trig On Enter"),
            ]
        );
        Ok(())
    }
}
