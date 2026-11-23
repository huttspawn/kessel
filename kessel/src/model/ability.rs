//! Abilities: ground ability stats from the sentinel-anchored prop block,
//! and GSF ability stats.

use std::collections::BTreeMap;

use crate::gsf_stat_dictionary::StatDictionary;
use crate::model::{rank_by_label, LabeledStat};
use crate::schema::gsf_ability::decode_gsf_ability_stats;

/// `01 04 00 00 80 BF`: prop 0x0401 with the -1.0 uninit value, the start
/// of the dominant ability template's prop block.
const ABILITY_PROP_SENTINEL: [u8; 6] = [0x01, 0x04, 0x00, 0x00, 0x80, 0xBF];

/// Decoded ability properties from a single sentinel-anchored prop block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AbilityProps {
    pub cooldown: Option<f32>,
    pub cast_time: Option<f32>,
    pub channel_duration: Option<f32>,
    pub hard_cast_time: Option<f32>,
    pub force_cost: Option<i32>,
    pub resource_cost: Option<i32>,
    /// Every finite in-block record, keyed by its dictionary label
    /// (`unknown_0x<id>` when unlabelled). Values are widened to `f64`.
    pub raw_props: BTreeMap<String, f64>,
}

impl AbilityProps {
    #[must_use]
    pub fn any_hit(&self) -> bool {
        self.cooldown.is_some()
            || self.cast_time.is_some()
            || self.channel_duration.is_some()
            || self.hard_cast_time.is_some()
            || self.force_cost.is_some()
            || self.resource_cost.is_some()
            || !self.raw_props.is_empty()
    }
}

/// An ability's stats: its FQN-derived resource pool and its prop block.
#[derive(Debug, Clone, PartialEq)]
pub struct AbilityStats {
    pub resource_pool: Option<&'static str>,
    pub props: AbilityProps,
}

/// Decode an `abl.*` ability's stats. `None` when the ability has neither
/// a prop block hit nor a resource pool (the secondary template with an
/// unclassified FQN), so there is nothing to record.
#[must_use]
pub fn decode_ability_stats(
    fqn: &str,
    payload: &[u8],
    dict: &StatDictionary,
) -> Option<AbilityStats> {
    let props = scan_ability_props(payload, dict);
    let resource_pool = resource_pool_from_fqn(fqn);
    (props.any_hit() || resource_pool.is_some()).then_some(AbilityStats {
        resource_pool,
        props,
    })
}

/// Decode an `abl.spvp.*` ability's stat records, labelled through the
/// FQN-aware dictionary and ranked per label.
#[must_use]
pub fn decode_gsf_ability(fqn: &str, payload: &[u8], dict: &StatDictionary) -> Vec<LabeledStat> {
    rank_by_label(decode_gsf_ability_stats(payload).into_iter().map(|rec| {
        (
            rec.prop_id,
            rec.ordinal,
            rec.value,
            dict.ability_label_for(rec.prop_id, fqn),
        )
    }))
}

/// Map an `abl.*` or `tal.*` FQN to a normalized resource pool / category tag.
///
/// Player class FQNs resolve to their resource pool (rage/focus/force/heat/
/// ammo/energy). Galactic Starfighter (`*.spvp.*`) resolves to `gsf` — GSF
/// uses a 3-pool blaster/engine/shield system, the tag identifies the game
/// mode. On-rails Space Combat (`*.space_combat.*`) and companion / racial /
/// legacy / spvp-buff entries resolve to None.
#[must_use]
pub fn resource_pool_from_fqn(fqn: &str) -> Option<&'static str> {
    let segments: Vec<&str> = fqn.split('.').collect();
    if segments.len() < 2 {
        return None;
    }
    if segments[0] != "abl" && segments[0] != "tal" {
        return None;
    }
    match segments[1] {
        "sith_warrior" => Some("rage"),
        "jedi_knight" => Some("focus"),
        "sith_inquisitor" | "jedi_consular" => Some("force"),
        "bounty_hunter" => Some("heat"),
        "trooper" => Some("ammo"),
        "agent" | "smuggler" => Some("energy"),
        "spvp" => Some("gsf"),
        _ => None,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Walk the sentinel-anchored ability property block. Finds the first
/// occurrence of `01 04 00 00 80 BF` (= 0x0401 with -1.0 uninit value), then
/// reads contiguous 6-byte `[u16 LE prop_id][f32 LE value]` records until the
/// next 2 bytes are not in the 0x04xx range. Returns an empty result for
/// abilities on the secondary template that have no such block.
///
/// Verified field-ID semantics (template 400000000283F4D2):
///   0x0401 = cooldown seconds
///   0x041b = cast / channel time seconds
///   0x0406 = channel duration seconds
///   0x041a = alternate hard cast time
///   0x0403 = Force-pool cost (sorcerer/sage; tech writes a low scaling
///            coefficient here, so values < 5 are dropped as non-cost)
///   0x041e = heat / energy / ammo cost (tech)
///
/// Other in-block fields (0x041d, 0x041f, 0x0420, 0x0421, 0x0402, 0x0404)
/// have class-context-dependent semantics that do not map cleanly to flat
/// columns; they land in `raw_props`, re-keyed from prop id to the ground
/// dictionary label (`melee_range_meters`).
#[must_use]
pub fn scan_ability_props(payload: &[u8], dict: &StatDictionary) -> AbilityProps {
    let mut stats = AbilityProps::default();
    let Some(start) = find_subslice(payload, &ABILITY_PROP_SENTINEL) else {
        return stats;
    };

    let mut raw: BTreeMap<u16, f32> = BTreeMap::new();
    let mut i = start;
    while i + 6 <= payload.len() {
        let prop_id = u16::from_le_bytes([payload[i], payload[i + 1]]);
        if (prop_id >> 8) != 0x04 {
            break;
        }
        let value = f32::from_le_bytes([
            payload[i + 2],
            payload[i + 3],
            payload[i + 4],
            payload[i + 5],
        ]);
        if value.is_finite() {
            // The sentinel itself (0x0401 = -1.0) should not displace later
            // 0x0401 records, which carry the actual cooldown.
            let is_sentinel = prop_id == 0x0401 && value == -1.0;
            if !is_sentinel {
                raw.entry(prop_id).or_insert(value);
                match prop_id {
                    0x0401 if stats.cooldown.is_none() => stats.cooldown = Some(value),
                    0x041b if stats.cast_time.is_none() => stats.cast_time = Some(value),
                    0x0406 if stats.channel_duration.is_none() => {
                        stats.channel_duration = Some(value);
                    }
                    0x041a if stats.hard_cast_time.is_none() => stats.hard_cast_time = Some(value),
                    0x0403 if stats.force_cost.is_none() && value >= 5.0 => {
                        stats.force_cost = Some(value as i32);
                    }
                    0x041e if stats.resource_cost.is_none() && value >= 1.0 => {
                        stats.resource_cost = Some(value as i32);
                    }
                    _ => {}
                }
            }
        }
        i += 6;
    }

    // Insert in prop-id order so a label shared by two ids keeps the later
    // id's value, as the JSON map built from the same walk always did.
    for (id, val) in raw {
        stats
            .raw_props
            .insert(dict.ground_ability_label(id).label, f64::from(val));
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> StatDictionary {
        StatDictionary::from_embedded().unwrap()
    }
    fn scan(payload: &[u8]) -> AbilityProps {
        scan_ability_props(payload, &dict())
    }
    fn rec(prop_id: u16, value: f32) -> Vec<u8> {
        let mut v = Vec::with_capacity(6);
        v.extend_from_slice(&prop_id.to_le_bytes());
        v.extend_from_slice(&value.to_le_bytes());
        v
    }
    fn block(records: &[(u16, f32)]) -> Vec<u8> {
        // 4 bytes of pre-sentinel padding so the sentinel is not at byte 0
        // (matches the real payload shape where header bytes precede it).
        let mut v = vec![0xAAu8; 4];
        v.extend(rec(0x0401, -1.0));
        for (id, val) in records {
            v.extend(rec(*id, *val));
        }
        // Trailing non-0x04xx byte pair to terminate the block.
        v.extend_from_slice(&[0x80, 0x05, 0x00, 0x00, 0x00, 0x00]);
        v
    }
    #[test]
    fn ability_props_decode_cooldown() {
        let stats = scan(&block(&[(0x0401, 15.0)]));
        assert_eq!(stats.cooldown, Some(15.0));
    }
    #[test]
    fn ability_props_decode_cast_and_force_cost() {
        let stats = scan(&block(&[(0x041b, 1.5), (0x0403, 40.0)]));
        assert_eq!(stats.cast_time, Some(1.5));
        assert_eq!(stats.force_cost, Some(40));
    }
    #[test]
    fn ability_props_drop_low_force_cost_as_scaling_coefficient() {
        // 0x0403 = 1.0 on tech abilities is a coefficient, not a Force cost.
        let stats = scan(&block(&[(0x0403, 1.0)]));
        assert_eq!(stats.force_cost, None);
    }
    #[test]
    fn ability_props_decode_resource_cost() {
        let stats = scan(&block(&[(0x041e, 15.0)]));
        assert_eq!(stats.resource_cost, Some(15));
    }
    #[test]
    fn ability_props_no_sentinel_no_hits() {
        // Secondary template: no sentinel anywhere.
        let stats = scan(&[0x00; 64]);
        assert!(!stats.any_hit());
    }
    #[test]
    fn ability_props_block_terminates_at_non_04xx() {
        // Sentinel + cooldown + non-0x04xx terminator + would-be-cast-time
        // outside the block. cast_time must NOT be picked up.
        let mut buf = vec![0xAAu8; 4];
        buf.extend(rec(0x0401, -1.0));
        buf.extend(rec(0x0401, 15.0));
        buf.extend_from_slice(&[0x80, 0x05, 0x00, 0x00, 0x00, 0x00]); // terminator
        buf.extend(rec(0x041b, 1.5)); // outside the block
        let stats = scan(&buf);
        assert_eq!(stats.cooldown, Some(15.0));
        assert_eq!(stats.cast_time, None);
    }
    #[test]
    fn ability_props_sentinel_does_not_displace_real_cooldown() {
        // 0x0401 = -1.0 is the sentinel; the next 0x0401 carries cd.
        let stats = scan(&block(&[(0x0401, 15.0)]));
        assert_eq!(stats.cooldown, Some(15.0));
    }
    #[test]
    fn ability_props_raw_props_keyed_by_label() {
        let stats = scan(&block(&[(0x0401, 15.0), (0x04ee, 2.5)]));
        assert_eq!(stats.raw_props.get("unknown_0x04ee"), Some(&2.5));
        assert_eq!(stats.raw_props.len(), 2);
    }
    #[test]
    fn decode_ability_stats_skips_unclassified_without_hits() {
        let d = dict();
        assert_eq!(decode_ability_stats("abl.companion.x", &[0; 32], &d), None);
        let classified = decode_ability_stats("abl.trooper.x", &[0; 32], &d).unwrap();
        assert_eq!(classified.resource_pool, Some("ammo"));
        assert!(!classified.props.any_hit());
    }
    #[test]
    fn resource_pool_warrior_is_rage() {
        assert_eq!(
            resource_pool_from_fqn("abl.sith_warrior.force_charge"),
            Some("rage")
        );
    }
    #[test]
    fn resource_pool_inquisitor_is_force() {
        assert_eq!(
            resource_pool_from_fqn("abl.sith_inquisitor.crushing_darkness"),
            Some("force")
        );
    }
    #[test]
    fn resource_pool_bounty_hunter_is_heat() {
        assert_eq!(
            resource_pool_from_fqn("abl.bounty_hunter.rocket_punch"),
            Some("heat")
        );
    }
    #[test]
    fn resource_pool_companion_is_none() {
        assert_eq!(
            resource_pool_from_fqn("abl.companion.weapon_set.blaster.tank.taunt"),
            None
        );
    }
    #[test]
    fn resource_pool_spvp_is_gsf() {
        assert_eq!(
            resource_pool_from_fqn("abl.spvp.missile.rocket_pod.damage"),
            Some("gsf")
        );
    }
    #[test]
    fn resource_pool_works_for_tal_prefix() {
        assert_eq!(
            resource_pool_from_fqn("tal.bounty_hunter.skill.bodyguard.empowered_scans"),
            Some("heat")
        );
        assert_eq!(
            resource_pool_from_fqn("tal.spvp.engine.barrel_roll.tier2"),
            Some("gsf")
        );
    }
    #[test]
    fn resource_pool_unknown_prefix_is_none() {
        assert_eq!(resource_pool_from_fqn("itm.something"), None);
        assert_eq!(resource_pool_from_fqn("qst.foo"), None);
    }
}
