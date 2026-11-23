//! Hash utilities for SWTOR data
//!
//! - SWTOR filename hash: 64-bit hash used in MYP archives
//! - game_id: sha256(fqn:guid)[0:16] -- unique per object-instance per extraction
//! - stable_id: sha256(fqn)[0:16] -- cross-patch identity (FQN is Bioware's
//!   semantic identity; survives patches)
//! - payload_hash: sha256(payload_bytes)[0:16] -- change detector for delta joins
//! - Icon ID: sha256(name)[0:16] -- deterministic icon filenames

use sha2::{Digest, Sha256};

// Archive filename hashing and the hash dictionary live in `kessel-tor`;
// re-exported so `crate::hash::*` paths keep resolving. The binary re-declares
// this module and uses only part of the surface, hence the allow.
#[allow(unused_imports)]
pub use kessel_tor::hash::{combine_hash, hashlittle2, swtor_filename_hash, HashDictionary};

/// Compute game_id from FQN + GUID.
/// Returns 16-character hex string: sha256(fqn:guid)[0:16].
///
/// Unique per object-instance per extraction. The compound is required because
/// neither field is unique-and-stable on its own:
/// - FQN is not unique in raw extraction (canonical objects + stub references
///   share an FQN).
/// - GUID shifts on every game patch.
///
/// game_id is the join key for the current extraction. For cross-patch identity
/// tracking, use `compute_stable_id(fqn)` instead -- that hash is stable across
/// patches but only unique post-dedup.
pub fn compute_game_id(fqn: &str, guid: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fqn);
    hasher.update(b":");
    hasher.update(guid);
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Compute stable_id from FQN alone.
/// Returns 16-character hex string: sha256(fqn)[0:16].
///
/// Stable across patch versions -- FQN is Bioware's semantic identity and
/// survives patches (a rename is a real semantic change, not drift). Unique
/// only post-dedup (`mark_canonical_by_fqn`); not suitable as a PK on raw
/// extraction.
///
/// Use for cross-version delta joins: `JOIN ... USING (stable_id)` finds the
/// same logical object across two extractions even when its GUID has shifted.
pub fn compute_stable_id(fqn: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(fqn);
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Compute payload_hash from raw GOM payload bytes.
/// Returns 16-character hex string: sha256(payload)[0:16].
///
/// Not an identity. Used to detect "did this object's data change between
/// extractions" when joined to `stable_id`.
pub fn compute_payload_hash(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Compute icon ID from icon name
/// Returns 16-character hex string: sha256(name)[0:16]
///
/// Used for cache-friendly icon filenames.
pub fn compute_icon_id(name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name);
    let result = hasher.finalize();
    hex::encode(&result[..8])
}
