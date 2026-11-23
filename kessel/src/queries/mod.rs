//! All SQL for a domain: load payloads from the `objects` table, hand them
//! to the pure `crate::model`, and write the domain's tables.
//!
//! Functions take a `&Connection` so the caller owns locking and flushing;
//! `Database`'s `populate_*` methods are thin calls into here.

pub mod ability;

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rusqlite::Connection;

/// Fetch and base64-decode the payloads of canonical objects whose `fqn`
/// matches `fqn_like` (an SQL LIKE pattern such as `"abl.%"`). Returns
/// `(game_id, fqn, payload)`; rows missing `payload_b64` or with
/// undecodable base64 are skipped.
///
/// # Errors
/// When the statement cannot be prepared or run.
pub fn object_payloads(
    conn: &Connection,
    fqn_like: &str,
) -> Result<Vec<(String, String, Vec<u8>)>> {
    Ok(object_payloads_optional(conn, fqn_like)?
        .into_iter()
        .filter_map(|(gid, fqn, p)| p.map(|p| (gid, fqn, p)))
        .collect())
}

/// `(game_id, fqn, payload)` with the payload absent or undecodable.
type OptionalPayload = (String, String, Option<Vec<u8>>);

/// Like [`object_payloads`] but keeps every matching object, with `None`
/// for a missing or undecodable payload.
///
/// # Errors
/// When the statement cannot be prepared or run.
pub fn object_payloads_optional(conn: &Connection, fqn_like: &str) -> Result<Vec<OptionalPayload>> {
    let mut stmt = conn.prepare(
        "SELECT game_id, fqn, json_extract(json, '$.payload_b64') \
         FROM objects WHERE fqn LIKE ?1 AND is_canonical = 1",
    )?;
    let rows = stmt
        .query_map([fqn_like], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .filter_map(std::result::Result::ok)
        .map(|(gid, fqn, b64)| {
            let payload = b64.and_then(|b| BASE64.decode(b).ok());
            (gid, fqn, payload)
        })
        .collect();
    Ok(rows)
}
