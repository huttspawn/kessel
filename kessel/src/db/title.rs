//! Title domain: how each player and legacy title is granted (FR-KESSEL-025 c6).
//!
//! A title id is its row position + 1 in the player title table
//! (chrPlayerTitlesTablePrototype, text in pc/title.stb) or its key in the
//! legacy title table (lgcLegacyTitlesTablePrototype, text in
//! pc/legacytitle.stb). Grants name titles by that id: a hydra 'Grant Title'
//! action (`script_actions`), a teaching item, an achievement's reward row, a
//! reputation rank, a collection, an engine code; a player title also names
//! the cdx.titles entry whose sources (`codex_sources`) grant it.

use super::world::{decode_framed, decoded_objects, load_singleton};
use super::*;
use crate::gom_reader::GomValue;
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, HashSet};

// chrPlayerTitlesTablePrototype: the title list, each row's text and codex
// entry, and engine code -> title ids.
const PLAYER_TITLES: u32 = 0xae42_5253;
const PLAYER_TEXT: u32 = 0xae42_5254;
const PLAYER_CODEX: u32 = 0x6f8d_0150;
const ENGINE_CODES: u32 = 0xae42_5258;
// lgcLegacyTitlesTablePrototype: id -> (id, text).
const LEGACY_TITLES: u32 = 0xa875_fc60;
const LEGACY_ID: u32 = 0xa875_fc5f;
const LEGACY_TEXT: u32 = 0xa875_fc5a;
// Item: what it teaches (5 player title, 9 legacy title) and the title id.
const ITEM_TEACHES: u32 = 0xdc16_d5e1;
const ITEM_TEACHES_VALUE: u32 = 0xdc16_d5e3;
// Achievement reward row, and achRewardsTable_Prototype's legacy title.
const ACH_REWARD: u32 = 0xdeaa_a365;
const REWARD_TABLE: u32 = 0x0f72_cd27;
const REWARD_LEGACY_TITLE: u32 = 0x352b_c8a0;
// repGroupInfosPrototype: per group, each side's fqn and legacy title per rank.
const REP_GROUPS: u32 = 0xf83f_8fd0;
const REP_SIDES: [(u32, u32); 2] = [(0x060d_40fd, 0xf639_5f99), (0x060d_40fe, 0xf639_5f9a)];
// colCollectionItemsPrototype: collection entries and the player title each names.
const COLLECTIONS: u32 = 0x0344_aaf8;
const COLLECTION_TITLE: u32 = 0xa21d_88fb;

/// One grant: (title kind, title id, source kind, source fqn, via).
type Grant = (&'static str, i64, &'static str, String, String);

fn num(v: &GomValue) -> Option<i64> {
    match v {
        GomValue::Str(s) => s.parse().ok(),
        other => other.as_i64(),
    }
}

fn field_num(v: &GomValue, id: u32) -> Option<i64> {
    v.embedded_field(id).and_then(num)
}

fn nums(v: Option<&GomValue>) -> Vec<i64> {
    v.and_then(GomValue::as_list)
        .unwrap_or_default()
        .iter()
        .filter_map(num)
        .collect()
}

/// The titles and the grants the singleton tables name.
#[derive(Debug, Default, PartialEq)]
struct Titles {
    /// Every title: (kind, id) -> packed string ref of its text.
    text: BTreeMap<(&'static str, i64), Option<i64>>,
    /// Player title id -> the codex entry guid it names.
    codex: Vec<(i64, u64)>,
    grants: Vec<Grant>,
}

impl Titles {
    /// Read the player and legacy title tables; a player title id is its row
    /// position + 1, and only rows carrying text are titles.
    fn read_tables(&mut self, player: Option<&GomValue>, legacy: Option<&GomValue>) {
        if let Some(player) = player {
            let rows = player
                .embedded_field(PLAYER_TITLES)
                .and_then(GomValue::as_list);
            for (pos, row) in rows.unwrap_or_default().iter().enumerate() {
                let Some(text) = field_num(row, PLAYER_TEXT) else {
                    continue;
                };
                let id = pos as i64 + 1;
                self.text.insert(("player", id), Some(text));
                if let Some(GomValue::U64(guid)) = row.embedded_field(PLAYER_CODEX) {
                    self.codex.push((id, *guid));
                }
            }
            let codes = player
                .embedded_field(ENGINE_CODES)
                .and_then(GomValue::as_map);
            for (code, ids) in codes.unwrap_or_default() {
                for id in nums(Some(ids)) {
                    let code = num(code).map(|c| c.to_string()).unwrap_or_default();
                    self.grants
                        .push(("player", id, "engine_code", code, "unverified".into()));
                }
            }
        }
        let entries = legacy
            .and_then(|l| l.embedded_field(LEGACY_TITLES))
            .and_then(GomValue::as_map);
        for (key, entry) in entries.unwrap_or_default() {
            if let Some(id) = field_num(entry, LEGACY_ID).or_else(|| num(key)) {
                self.text
                    .insert(("legacy", id), field_num(entry, LEGACY_TEXT));
            }
        }
    }

    /// Reputation ranks (one legacy title per rank and side; 0 is none) and
    /// collections (a player title each).
    fn read_grant_tables(&mut self, reps: Option<&GomValue>, collections: Option<&GomValue>) {
        let groups = reps
            .and_then(|r| r.embedded_field(REP_GROUPS))
            .and_then(GomValue::as_map);
        for (_, group) in groups.unwrap_or_default() {
            for (side, titles) in REP_SIDES {
                let Some(fqn) = group.embedded_field(side).and_then(super::world::gom_str) else {
                    continue;
                };
                for (rank, id) in nums(group.embedded_field(titles)).into_iter().enumerate() {
                    if id != 0 {
                        let via = format!("rank {}", rank + 1);
                        self.grants
                            .push(("legacy", id, "reputation_rank", fqn.clone(), via));
                    }
                }
            }
        }
        let entries = collections
            .and_then(|c| c.embedded_field(COLLECTIONS))
            .and_then(GomValue::as_map);
        for (key, entry) in entries.unwrap_or_default() {
            let key = num(key).map(|k| k.to_string()).unwrap_or_default();
            for id in nums(entry.embedded_field(COLLECTION_TITLE)) {
                self.grants
                    .push(("player", id, "collection", key.clone(), String::new()));
            }
        }
    }
}

/// The reward row id -> legacy title id of achRewardsTable_Prototype.
fn reward_titles(rewards: Option<&GomValue>) -> std::collections::HashMap<i64, i64> {
    let rows = rewards
        .and_then(|r| r.embedded_field(REWARD_TABLE))
        .and_then(GomValue::as_map);
    rows.unwrap_or_default()
        .iter()
        .filter_map(|(k, row)| Some((num(k)?, field_num(row, REWARD_LEGACY_TITLE)?)))
        .collect()
}

/// The title an item teaches: dc16d5e1 5 is a player title, 9 a legacy title.
fn item_title(item: &GomValue) -> Option<(&'static str, i64)> {
    let kind = match field_num(item, ITEM_TEACHES)? {
        5 => "player",
        9 => "legacy",
        _ => return None,
    };
    Some((kind, field_num(item, ITEM_TEACHES_VALUE)?))
}

impl Database {
    /// Populate `title_grants`. Run after `populate_codex_sources` and
    /// `populate_world_areas`, whose tables this reads. Returns rows written.
    pub fn populate_title_grants(&self) -> Result<u64> {
        self.with_tx(|tx| {
            let mut titles = Titles::default();
            titles.read_tables(
                load_singleton(tx, "chrPlayerTitlesTablePrototype")?.as_ref(),
                load_singleton(tx, "lgcLegacyTitlesTablePrototype")?.as_ref(),
            );
            titles.read_grant_tables(
                load_singleton(tx, "repGroupInfosPrototype")?.as_ref(),
                load_singleton(tx, "colCollectionItemsPrototype")?.as_ref(),
            );
            let rewards = reward_titles(load_singleton(tx, "achRewardsTable_Prototype")?.as_ref());
            for (fqn, ach) in decoded_objects(tx, "Achievement")? {
                if let Some(id) = field_num(&ach, ACH_REWARD).and_then(|r| rewards.get(&r)) {
                    titles
                        .grants
                        .push(("legacy", *id, "achievement_reward", fqn, String::new()));
                }
            }
            // Items one at a time: there are too many to decode at once.
            let mut stmt = tx.prepare(
                "SELECT fqn, json_extract(json, '$.header_hex'), json_extract(json, '$.payload_b64') \
                 FROM objects WHERE kind = 'Item' AND is_canonical = 1",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                let (Some(header), Some(payload)) =
                    (r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)
                else {
                    continue;
                };
                if let Some((kind, id)) = decode_framed(&header, &payload).as_ref().and_then(item_title)
                {
                    titles.grants.push((kind, id, "item", r.get(0)?, String::new()));
                }
            }
            drop(rows);
            drop(stmt);
            insert_title_grants(tx, &titles)
        })
    }
}

fn insert_title_grants(tx: &Transaction, titles: &Titles) -> Result<u64> {
    tx.execute("DELETE FROM title_grants", [])?;
    let mut grants = titles.grants.clone();
    // Script grants: a hydra 'Grant Title' action's value is a player title id.
    {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT script_fqn, handler, value FROM script_actions \
             WHERE action = 'Grant Title' AND value IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (script, handler, value) = row?;
            if let Ok(id) = value.parse() {
                grants.push(("player", id, "script", script, handler));
            }
        }
    }
    // Codex grants: the cdx.titles entry a player title names, through each
    // of that entry's sources.
    {
        let mut codex = tx.prepare(
            "SELECT fqn FROM objects WHERE guid = ?1 AND kind = 'Codex' AND is_canonical = 1",
        )?;
        let mut sources = tx.prepare(
            "SELECT source_kind || ' ' || source_fqn FROM codex_sources \
             WHERE codex_fqn = ?1 AND source_kind <> 'none'",
        )?;
        for (id, guid) in &titles.codex {
            let fqn: Option<String> = codex
                .query_row([format!("{guid:016X}")], |r| r.get(0))
                .optional()?;
            let Some(fqn) = fqn else { continue };
            for via in sources.query_map([&fqn], |r| r.get::<_, String>(0))? {
                grants.push(("player", *id, "codex", fqn.clone(), via?));
            }
        }
    }
    let granted: HashSet<(&str, i64)> = grants.iter().map(|g| (g.0, g.1)).collect();
    for (kind, id) in titles.text.keys() {
        if !granted.contains(&(*kind, *id)) {
            grants.push((kind, *id, "none", String::new(), String::new()));
        }
    }
    let mut text = tx.prepare(
        "SELECT COALESCE(text_raw, text) FROM strings \
         WHERE id2 = ?1 AND id1 = ?2 AND locale = 'en-us'",
    )?;
    let mut insert = tx.prepare(
        "INSERT OR IGNORE INTO title_grants \
           (title_kind, title_id, title_text, source_kind, source_fqn, via) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (kind, id, source_kind, source, via) in &grants {
        let packed = titles.text.get(&(*kind, *id)).copied().flatten();
        let title_text: Option<String> = match packed {
            Some(p) => text
                .query_row([p >> 32, p & 0xFFFF_FFFF], |r| r.get(0))
                .optional()?,
            None => None,
        };
        insert.execute(params![kind, id, title_text, source_kind, source, via])?;
    }
    drop(insert);
    Ok(tx.query_row("SELECT COUNT(*) FROM title_grants", [], |r| r.get(0))?)
}

/// Create the title-domain tables (idempotent).
pub(crate) fn create_tables(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        r#"
            -- One row per way a title is granted (FR-KESSEL-025 c6). title_kind:
            -- player (id = row position + 1 in chrPlayerTitlesTablePrototype,
            -- text in pc/title.stb) or legacy (lgcLegacyTitlesTablePrototype,
            -- pc/legacytitle.stb). title_text is NULL when the id has no text.
            -- source_kind: script (hydra Grant Title; via is the handler), item
            -- (dc16d5e1/dc16d5e3), achievement_reward (deaaa365 -> 352bc8a0),
            -- reputation_rank (source_fqn the side's rep fqn, via 'rank N'),
            -- collection (source_fqn the collection key), codex (source_fqn the
            -- cdx.titles entry, via '<codex source_kind> <codex source_fqn>'),
            -- engine_code (ae425258; source_fqn the code, via 'unverified'), or
            -- none (no grant path found).
            CREATE TABLE IF NOT EXISTS title_grants (
                title_kind  TEXT NOT NULL,
                title_id    INTEGER NOT NULL,
                title_text  TEXT,
                source_kind TEXT NOT NULL,
                source_fqn  TEXT NOT NULL,
                via         TEXT NOT NULL,
                PRIMARY KEY (title_kind, title_id, source_kind, source_fqn, via)
            );
            CREATE INDEX IF NOT EXISTS idx_title_grants_source ON title_grants(source_fqn);
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

    fn list(ids: &[i64]) -> G {
        G::List(ids.iter().map(|n| G::I64(*n)).collect())
    }

    #[test]
    fn title_ids_are_row_position_plus_one_and_grant_tables_name_them() {
        let text = |id1: i64| G::I64((469_654 << 32) | id1);
        // Row 2 has no text, so it is no title; row 3 is title 3.
        let player = obj(vec![
            (
                PLAYER_TITLES,
                G::List(vec![
                    obj(vec![(PLAYER_TEXT, text(1)), (PLAYER_CODEX, G::U64(0xC0))]),
                    obj(vec![]),
                    obj(vec![(PLAYER_TEXT, text(3))]),
                ]),
            ),
            (ENGINE_CODES, G::Map(vec![(G::I64(3001), list(&[3]))])),
        ]);
        let legacy = obj(vec![(
            LEGACY_TITLES,
            G::Map(vec![(
                G::I64(17),
                obj(vec![
                    (LEGACY_ID, G::I64(17)),
                    (LEGACY_TEXT, G::I64((746_295 << 32) | 9)),
                ]),
            )]),
        )]);
        let reps = obj(vec![(
            REP_GROUPS,
            G::Map(vec![(
                G::I64(1),
                obj(vec![
                    (0x060d_40fd, G::Str("rep.makeb.empire".into())),
                    (0x060d_40fe, G::Str("rep.makeb.republic".into())),
                    (0xf639_5f99, list(&[0, 0, 17])),
                    (0xf639_5f9a, list(&[0, 0, 0, 18])),
                ]),
            )]),
        )]);
        let collections = obj(vec![(
            COLLECTIONS,
            G::Map(vec![
                (G::I64(274), obj(vec![(COLLECTION_TITLE, list(&[646]))])),
                (G::I64(959), obj(vec![(COLLECTION_TITLE, list(&[]))])),
            ]),
        )]);
        let mut t = Titles::default();
        t.read_tables(Some(&player), Some(&legacy));
        t.read_grant_tables(Some(&reps), Some(&collections));
        assert_eq!(
            t.text,
            BTreeMap::from([
                (("legacy", 17), Some((746_295 << 32) | 9)),
                (("player", 1), Some((469_654 << 32) | 1)),
                (("player", 3), Some((469_654 << 32) | 3)),
            ])
        );
        assert_eq!(t.codex, [(1, 0xC0)]);
        let g = |k, id, sk, s: &str, v: &str| (k, id, sk, s.to_string(), v.to_string());
        assert_eq!(
            t.grants,
            [
                g("player", 3, "engine_code", "3001", "unverified"),
                g(
                    "legacy",
                    17,
                    "reputation_rank",
                    "rep.makeb.empire",
                    "rank 3"
                ),
                g(
                    "legacy",
                    18,
                    "reputation_rank",
                    "rep.makeb.republic",
                    "rank 4"
                ),
                g("player", 646, "collection", "274", ""),
            ]
        );
    }

    #[test]
    fn items_and_reward_rows_name_titles() {
        let item = |cat, id| {
            obj(vec![
                (ITEM_TEACHES, G::Enum(cat)),
                (ITEM_TEACHES_VALUE, G::U64(id)),
            ])
        };
        assert_eq!(item_title(&item(5, 646)), Some(("player", 646)));
        assert_eq!(item_title(&item(9, 40)), Some(("legacy", 40)));
        assert_eq!(item_title(&item(2, 40)), None);
        let rewards = obj(vec![(
            REWARD_TABLE,
            G::Map(vec![
                (G::I64(-5), obj(vec![(REWARD_LEGACY_TITLE, G::I64(594))])),
                (G::I64(6), obj(vec![])),
            ]),
        )]);
        assert_eq!(reward_titles(Some(&rewards)), [(-5, 594)].into());
    }

    #[test]
    fn title_grants_answer_scripts_codex_sources_text_and_none() -> Result<()> {
        let path = temp_db_path("title_grants");
        let db = Database::with_grammar(&path, None)?;
        db.init_schema()?;
        let mut titles = Titles::default();
        titles
            .text
            .insert(("player", 114), Some((469_654 << 32) | 7));
        titles.text.insert(("player", 115), None);
        titles.text.insert(("legacy", 66), None);
        titles.codex.push((115, 0xAB));
        titles.grants.push((
            "legacy",
            40,
            "item",
            "itm.legacy_title".into(),
            String::new(),
        ));
        let master = "hyd.title.class.jedi_knight.master";
        db.with_tx(|tx| {
            tx.execute_batch(&format!(
                "INSERT INTO script_actions (script_fqn, handler, ordinal, action, value) VALUES
                   ('{master}', 'On Custom:Grant', 0, 'Grant Title', '114'),
                   ('{master}', 'On Custom:Grant', 1, 'Grant Codex', 'cdx.x');
                 INSERT INTO strings (fqn, locale, id1, id2, text, text_raw) \
                   VALUES ('str.pc.title.7', 'en-us', 7, 469654, 'Master', 'Master <<1>>');
                 INSERT INTO codex_sources (codex_fqn, source_kind, source_fqn, via) VALUES
                   ('cdx.titles.b', 'quest', 'qst.b', 'hyd.b On Custom:B'),
                   ('cdx.titles.b', 'none', '', '');
                 INSERT INTO objects (game_id, stable_id, payload_hash, guid, template_guid, \
                   fqn, kind, version, revision, json, is_canonical) \
                   VALUES ('ab', 'ab', '', '00000000000000AB', '', 'cdx.titles.b', 'Codex', \
                     0, 0, '{{}}', 1);"
            ))?;
            insert_title_grants(tx, &titles)
        })?;
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT title_kind, title_id, title_text, source_kind, source_fqn, via \
             FROM title_grants ORDER BY title_kind, title_id",
        )?;
        type Row = (String, i64, Option<String>, String, String, String);
        let rows: Vec<Row> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let row = |k: &str, id, t: Option<&str>, sk: &str, s: &str, v: &str| -> Row {
            (
                k.into(),
                id,
                t.map(String::from),
                sk.into(),
                s.into(),
                v.into(),
            )
        };
        assert_eq!(
            rows,
            [
                row("legacy", 40, None, "item", "itm.legacy_title", ""),
                row("legacy", 66, None, "none", "", ""),
                row(
                    "player",
                    114,
                    Some("Master <<1>>"),
                    "script",
                    master,
                    "On Custom:Grant"
                ),
                row("player", 115, None, "codex", "cdx.titles.b", "quest qst.b"),
            ]
        );
        Ok(())
    }
}
