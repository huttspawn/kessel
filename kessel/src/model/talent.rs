//! Talents: FQN-derived details, the script hook, the abilities a talent
//! references, and GSF talent stats.

use crate::gsf_stat_dictionary::StatDictionary;
use crate::model::ability::resource_pool_from_fqn;
use crate::model::{rank_by_label, LabeledStat};
use crate::schema::gsf_talent::decode_gsf_stats;

/// Per-talent metadata: resource pool, tier and script hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentDetails {
    /// Same vocabulary as ability resource pools (rage/focus/.../gsf).
    pub resource_pool: Option<&'static str>,
    /// The FQN's last segment (tier1, tier_3a, base, passive).
    pub tier: Option<String>,
    /// The length-prefixed ASCII tail of the payload naming the ability mod
    /// the talent triggers. `None` without a payload or a plausible tail.
    pub script_hook: Option<String>,
}

/// Decode a `tal.*` talent's details. Every talent gets details; fields
/// are `None` when not derivable, and a missing payload only drops the
/// script hook.
#[must_use]
pub fn decode_talent_details(fqn: &str, payload: Option<&[u8]>) -> TalentDetails {
    TalentDetails {
        resource_pool: resource_pool_from_fqn(fqn),
        tier: fqn.rsplit('.').next().map(str::to_string),
        script_hook: payload.and_then(extract_talent_script_hook),
    }
}

/// Decode a `tal.spvp.*` talent's stat records, labelled through the
/// FQN-aware dictionary (so context-overloaded stat ids such as 0x40 on
/// `minor_sensors.com_range.*` get the domain label) and ranked per label.
#[must_use]
pub fn decode_gsf_talent(fqn: &str, payload: &[u8], dict: &StatDictionary) -> Vec<LabeledStat> {
    rank_by_label(decode_gsf_stats(payload).into_iter().map(|rec| {
        (
            u16::from(rec.stat_id),
            rec.ordinal,
            rec.value,
            dict.talent_label_for(rec.stat_id, fqn),
        )
    }))
}

/// Extract ability GUIDs from a `tal.*` payload using the documented pattern:
///   CC 17 E2 84 0B  (field marker, may appear as CC 0B 84 E2 17 in some payloads)
///   D0 01           (int8 = 1)
///   CF E0 00 XX XX XX XX XX XX  (CF type tag + 8-byte GUID; E0 00 are GUID bytes 1-2)
///
/// Returns hex strings (uppercase, 16 chars) matching the objects.guid format.
#[must_use]
pub fn extract_ability_guids_from_talent(payload: &[u8]) -> Vec<String> {
    let mut guids = Vec::new();
    let len = payload.len();
    if len < 16 {
        return guids;
    }

    let mut i = 0;
    while i + 16 <= len {
        // Look for CC followed by field ID 17E2840B (either byte order)
        if payload[i] != 0xCC {
            i += 1;
            continue;
        }
        let is_field = (i + 5 <= len)
            && ((payload[i + 1] == 0x17
                && payload[i + 2] == 0xE2
                && payload[i + 3] == 0x84
                && payload[i + 4] == 0x0B)
                || (payload[i + 1] == 0x0B
                    && payload[i + 2] == 0x84
                    && payload[i + 3] == 0xE2
                    && payload[i + 4] == 0x17));
        if !is_field {
            i += 1;
            continue;
        }
        // Skip CC + 4-byte field ID + D0 01
        let after = i + 5;
        if after + 2 > len {
            i += 1;
            continue;
        }
        // D0 01 marker
        let guid_start = if payload[after] == 0xD0 && payload[after + 1] == 0x01 {
            after + 2
        } else {
            after
        };
        // CF [8-byte GUID BE]: E0 00 are the first two bytes of the GUID, not markers.
        // Matches populate_quest_chain's format: payload[i+1..i+9] byte-concat hex.
        if guid_start + 9 > len {
            i += 1;
            continue;
        }
        if payload[guid_start] == 0xCF
            && payload[guid_start + 1] == 0xE0
            && payload[guid_start + 2] == 0x00
        {
            let g = &payload[guid_start + 1..guid_start + 9];
            let hex = g.iter().map(|b| format!("{b:02X}")).collect::<String>();
            guids.push(hex);
            i = guid_start + 9;
        } else {
            i += 1;
        }
    }
    guids
}

/// Length-prefixed ASCII tail string at the end of a talent payload (vault
/// MAPPINGS.md lines 339-365). The byte at `payload[i]` is the length, the
/// `payload[i+1..i+1+len]` bytes are the ASCII script-hook identifier. Walks
/// backward from end of payload to find the most recent valid pattern, since
/// hooks live at the tail. Returns the decoded string, or None if no
/// plausible candidate is found.
#[must_use]
pub fn extract_talent_script_hook(payload: &[u8]) -> Option<String> {
    let n = payload.len();
    // Walk back through the last 96 bytes looking for a length prefix that
    // points to a printable ASCII identifier extending to the end.
    let max_lookback = n.min(96);
    for offset in 1..max_lookback {
        let i = n.saturating_sub(offset + 1);
        let len_byte = payload[i] as usize;
        if !(4..=80).contains(&len_byte) {
            continue;
        }
        let start = i + 1;
        let end = start + len_byte;
        if end > n {
            continue;
        }
        let chunk = &payload[start..end];
        if !chunk.iter().all(|b| (32..127).contains(b)) {
            continue;
        }
        let s = std::str::from_utf8(chunk).ok()?;
        // Hook identifiers are alphanumeric + underscore, must start alpha.
        let bytes = s.as_bytes();
        if !bytes[0].is_ascii_alphabetic() {
            continue;
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            continue;
        }
        return Some(s.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_talent_with_tail(tail: &str) -> Vec<u8> {
        // Real tail format: filler bytes, then [len_u8][ascii bytes].
        let mut v = vec![0xCCu8; 64];
        v.push(u8::try_from(tail.len()).unwrap());
        v.extend_from_slice(tail.as_bytes());
        v
    }
    #[test]
    fn talent_script_hook_extracts_tail_identifier() {
        let payload = synth_talent_with_tail("abl_bh_me_kolto_shot");
        assert_eq!(
            extract_talent_script_hook(&payload),
            Some("abl_bh_me_kolto_shot".to_string())
        );
    }
    #[test]
    fn talent_script_hook_extracts_long_gsf_hook() {
        let payload = synth_talent_with_tail("spvp_increasedsystemsdamagechance");
        assert_eq!(
            extract_talent_script_hook(&payload),
            Some("spvp_increasedsystemsdamagechance".to_string())
        );
    }
    #[test]
    fn talent_script_hook_returns_none_when_no_tail() {
        let payload = vec![0xFFu8; 32];
        assert_eq!(extract_talent_script_hook(&payload), None);
    }
    #[test]
    fn talent_script_hook_rejects_non_identifier_tail() {
        // Length-prefixed but contains a space — not a valid identifier.
        let mut payload = vec![0u8; 16];
        payload.push(11);
        payload.extend_from_slice(b"hello world");
        assert_eq!(extract_talent_script_hook(&payload), None);
    }
    #[test]
    fn talent_details_without_payload_keeps_pool_and_tier() {
        let d = decode_talent_details("tal.spvp.engine.barrel_roll.tier2", None);
        assert_eq!(d.resource_pool, Some("gsf"));
        assert_eq!(d.tier.as_deref(), Some("tier2"));
        assert_eq!(d.script_hook, None);
    }
    #[test]
    fn ability_guids_follow_the_cc_field_marker() {
        let mut p = vec![0u8; 4];
        p.extend_from_slice(&[0xCC, 0x17, 0xE2, 0x84, 0x0B, 0xD0, 0x01]);
        p.extend_from_slice(&[0xCF, 0xE0, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        p.extend_from_slice(&[0u8; 8]);
        assert_eq!(
            extract_ability_guids_from_talent(&p),
            vec!["E000010203040506".to_string()]
        );
    }
    #[test]
    fn gsf_talent_ranks_repeat_labels_in_payload_order() {
        let dict = StatDictionary::from_embedded().unwrap();
        // Two bare records of stat 0x22 (+0.04, +0.08), each closed by the
        // `cb 19 d7 4b ?? 03` record-end signature.
        let mut p = vec![0u8; 8];
        for v in [0.04f32, 0.08] {
            p.extend_from_slice(&[0x22, 0x01, 0x04]);
            p.extend_from_slice(&v.to_le_bytes());
            p.extend_from_slice(&[0xCB, 0x19, 0xD7, 0x4B, 0x96, 0x03]);
        }
        let stats = decode_gsf_talent("tal.spvp.x", &p, &dict);
        let ranks: Vec<i64> = stats.iter().map(|s| s.rank).collect();
        assert_eq!(ranks, vec![1, 2]);
        assert!(stats.iter().all(|s| s.stat_id == 0x22));
    }
}
