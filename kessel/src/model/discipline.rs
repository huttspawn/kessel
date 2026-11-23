//! Discipline trees: where an ability or talent sits in a combat style's
//! discipline tree, and the `dis.*` mod tree. Pure: FQNs and payload bytes
//! in, typed placements out.

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

use crate::schema::discipline::decode_dis_payload;

/// Where an `abl.*` ability sits in the discipline trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbilityPlacement {
    /// `abl.<origin>.skill.<discipline>.*`: belongs to one combat discipline.
    Discipline {
        origin: String,
        discipline_name: String,
        /// `abl.<origin>.skill.<discipline>`, the `disciplines` key.
        fqn_prefix: String,
        combat_style: &'static str,
        /// Unlock level for `mods.tierN` choices; `None` otherwise.
        tier_level: Option<u8>,
        /// `core`, `passive`, `special`, `choice` or `mod`.
        slot_type: &'static str,
    },
    /// A per-origin pool shared by both combat styles of the origin.
    Shared {
        origin: String,
        /// `class_shared`, `utility` or `shared_mod`.
        slot_type: &'static str,
    },
}

/// Classify an `abl.*` FQN into its discipline-tree placement.
///
///   abl.{class}.skill.{discipline}.{name}              -> core
///   abl.{class}.skill.{discipline}.mods.passive.{name} -> passive
///   abl.{class}.skill.{discipline}.mods.tierN.{name}   -> choice
///   abl.{class}.skill.{discipline}.mods.special.{name} -> special
///   abl.{class}.skill.utility.{name}                   -> shared utility
///   abl.{class}.skill.mods.tier1.{name}                -> shared mod
///   abl.{class}.{name}                                 -> class-shared
///
/// Returns `None` for non-player origins and for disciplines missing from
/// `DISCIPLINE_COMBAT_STYLE_MAP` (a new patch's discipline then shows up as
/// a missing extraction and forces an explicit map update).
#[must_use]
pub fn place_ability(fqn: &str) -> Option<AbilityPlacement> {
    let parts: Vec<&str> = fqn.split('.').collect();

    if parts.len() == 3 {
        let origin = parts[1];
        return PLAYER_ORIGINS
            .contains(&origin)
            .then(|| AbilityPlacement::Shared {
                origin: origin.to_string(),
                slot_type: "class_shared",
            });
    }

    if parts.len() < 5 || parts[2] != "skill" {
        return None;
    }
    let origin = parts[1];
    if !PLAYER_ORIGINS.contains(&origin) {
        return None;
    }
    let shared = |slot_type| {
        Some(AbilityPlacement::Shared {
            origin: origin.to_string(),
            slot_type,
        })
    };
    if parts[3] == "utility" {
        return shared("utility");
    }
    if parts[3] == "mods" {
        return shared("shared_mod");
    }

    let discipline_name = parts[3];
    let combat_style = combat_style_for(origin, discipline_name)?;
    let (slot_type, tier_level) = if parts.len() >= 7 && parts[4] == "mods" {
        match parts[5] {
            "passive" => ("passive", None),
            "special" => ("special", None),
            s if s.starts_with("tier") => ("choice", tier_from_segment(Some(s))),
            _ => ("mod", None),
        }
    } else {
        ("core", None)
    };
    Some(AbilityPlacement::Discipline {
        origin: origin.to_string(),
        discipline_name: discipline_name.to_string(),
        fqn_prefix: format!("abl.{origin}.skill.{discipline_name}"),
        combat_style,
        tier_level,
        slot_type,
    })
}

/// Where a `tal.*` talent sits in the discipline trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TalentPlacement {
    /// `tal.<origin>.skill.<discipline>.*`: literal membership of the
    /// discipline keyed `abl.<origin>.skill.<discipline>`.
    Discipline { fqn_prefix: String },
    /// `tal.<origin>.skill.utility.*`: offered to both combat styles.
    Utility { origin: String },
}

/// Classify a `tal.*` FQN into its discipline-tree placement. No tier or
/// column: the editorial tree layout is not in `tal.*` payloads or FQNs.
#[must_use]
pub fn place_talent(fqn: &str) -> Option<TalentPlacement> {
    let parts: Vec<&str> = fqn.split('.').collect();
    if parts.len() < 5 || parts[2] != "skill" {
        return None;
    }
    let origin = parts[1];
    let segment = parts[3];
    if !PLAYER_ORIGINS.contains(&origin) {
        return None;
    }
    if segment == "utility" {
        return Some(TalentPlacement::Utility {
            origin: origin.to_string(),
        });
    }
    Some(TalentPlacement::Discipline {
        fqn_prefix: format!("abl.{origin}.skill.{segment}"),
    })
}

/// One mod in a discipline's 8-tier x 3-choice mod tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineTreeMod {
    pub mod_index: u8,
    /// 1-based tier; 0 when the index is in no tier triplet.
    pub tier_ordinal: u8,
    /// Left/middle/right position in the UI; 0 when in no tier triplet.
    pub ui_position: u8,
    pub level_required: u8,
    pub target_guid: String,
    pub is_default: bool,
}

/// A decoded `dis.<style>.<discipline>` record: the discipline's codename,
/// its apc.* and signature-ability GUIDs, and its mod tree. GUIDs are left
/// unresolved; resolving them to game ids is the caller's lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineTree {
    /// `combat_styles.fqn_segment` (the `sage` alias is translated to
    /// `force_wizard`).
    pub combat_style: String,
    pub discipline_name: String,
    pub codename: String,
    pub icon_apc_guid: String,
    pub mod_tree_apc_guid: String,
    pub signature_ability_guid: String,
    pub mods: Vec<DisciplineTreeMod>,
}

impl DisciplineTree {
    /// The `apc.<origin>.<style>.base` FQN used as the icon when the icon
    /// GUID does not resolve (a source-data gap on 3 disciplines). `origin`
    /// is segment 1 of the discipline's `abl.<origin>.skill.<disc>` prefix.
    #[must_use]
    pub fn fallback_icon_apc_fqn(&self, fqn_prefix: &str) -> Option<String> {
        let origin = fqn_prefix.split('.').nth(1)?;
        Some(format!("apc.{origin}.{}.base", self.combat_style))
    }
}

/// Decode a `dis.<style>.<discipline>` payload into its discipline tree.
///
/// # Errors
/// When the FQN has fewer than three segments or the payload does not
/// match the `dis.*` layout (`decode_dis_payload`).
pub fn decode_discipline_tree(dis_fqn: &str, payload: &[u8]) -> Result<DisciplineTree> {
    let record = decode_dis_payload(payload)?;
    let segments: Vec<&str> = dis_fqn.split('.').collect();
    if segments.len() < 3 {
        bail!("dis fqn {dis_fqn} has fewer than 3 segments");
    }
    // dis.* uses combat_styles.fqn_segment as segment 1, except the short
    // alias `sage` for `force_wizard`.
    let combat_style = match segments[1] {
        "sage" => "force_wizard",
        other => other,
    };

    let mut tier_lookup: HashMap<u8, (u8, u8)> = HashMap::new();
    for tier in &record.tiers {
        for (pos, idx) in (0u8..).zip(tier.choice_indices.iter()) {
            tier_lookup.insert(*idx, (tier.ordinal, pos));
        }
    }
    let defaults: HashSet<u8> = record
        .defaults
        .iter()
        .map(|d| d.default_mod_index)
        .collect();

    let mods = record
        .mods
        .iter()
        .map(|entry| {
            let (tier_ordinal, ui_position) =
                tier_lookup.get(&entry.index).copied().unwrap_or((0, 0));
            DisciplineTreeMod {
                mod_index: entry.index,
                tier_ordinal,
                ui_position,
                level_required: entry.level,
                target_guid: entry.guid.clone(),
                is_default: defaults.contains(&entry.index),
            }
        })
        .collect();

    Ok(DisciplineTree {
        combat_style: combat_style.to_string(),
        discipline_name: segments[2].to_string(),
        codename: record.codename,
        icon_apc_guid: record.icon_apc_guid,
        mod_tree_apc_guid: record.mod_tree_apc_guid,
        signature_ability_guid: record.signature_ability_guid,
        mods,
    })
}

/// Unlock level of a `mods.tierN` choice segment.
#[must_use]
pub fn tier_from_segment(seg: Option<&str>) -> Option<u8> {
    match seg? {
        "tier1" => Some(15),
        "tier2" => Some(23),
        "tier3" => Some(39),
        "tier4" => Some(43),
        "tier5" => Some(51),
        "tier6" => Some(64),
        "tier7" => Some(68),
        "tier8" => Some(73),
        _ => None,
    }
}

/// Origin codenames -- the second FQN segment in `abl.<origin>.skill.*`.
/// 8 player origins (sith_warrior, agent, etc.). Other `abl.<X>.<name>` FQNs
/// (companion, racials, legacy, location, customer_service, ...) aren't
/// player-discipline content; we ignore them.
pub const PLAYER_ORIGINS: &[&str] = &[
    "agent",
    "bounty_hunter",
    "jedi_consular",
    "jedi_knight",
    "sith_inquisitor",
    "sith_warrior",
    "smuggler",
    "trooper",
];

/// (origin, discipline_name) -> combat_style_codename.
/// 48-row map (8 origins * 6 disciplines). Stable since SWTOR 4.0; new
/// disciplines force an explicit update via populate_disciplines's
/// "unknown discipline -> skip" branch.
///
/// Names use SOURCE-data canon (firebug not pyrotech, combat not kinetic_combat).
/// huttspawn ETL renames at the editorial layer per #51.
/// 48-row table: every (origin, discipline) pair maps to exactly one combat
/// style. Stored as a flat constant so unit tests can iterate it and assert
/// invariants (combat-style values join `combat_styles.fqn_segment`, every
/// combat style appears in `origin_combat_styles`, etc).
pub const DISCIPLINE_COMBAT_STYLE_MAP: &[(&str, &str, &str)] = &[
    ("sith_warrior", "annihilation", "marauder"),
    ("sith_warrior", "carnage", "marauder"),
    ("sith_warrior", "fury", "marauder"),
    ("sith_warrior", "immortal", "juggernaut"),
    ("sith_warrior", "rage", "juggernaut"),
    ("sith_warrior", "vengeance", "juggernaut"),
    ("sith_inquisitor", "darkness", "assassin"),
    ("sith_inquisitor", "deception", "assassin"),
    ("sith_inquisitor", "hatred", "assassin"),
    ("sith_inquisitor", "corruption", "sorcerer"),
    ("sith_inquisitor", "lightning", "sorcerer"),
    ("sith_inquisitor", "madness", "sorcerer"),
    ("bounty_hunter", "advanced_prototype", "powertech"),
    ("bounty_hunter", "firebug", "powertech"),
    ("bounty_hunter", "shield_tech", "powertech"),
    ("bounty_hunter", "arsenal", "mercenary"),
    ("bounty_hunter", "bodyguard", "mercenary"),
    ("bounty_hunter", "innovative_ordnance", "mercenary"),
    ("agent", "concealment", "operative"),
    ("agent", "lethality", "operative"),
    ("agent", "medic", "operative"),
    ("agent", "engineering", "sniper"),
    ("agent", "marksmanship", "sniper"),
    ("agent", "virulence", "sniper"),
    ("jedi_knight", "defense", "guardian"),
    ("jedi_knight", "focus", "guardian"),
    ("jedi_knight", "vigilance", "guardian"),
    ("jedi_knight", "combat", "sentinel"),
    ("jedi_knight", "concentration", "sentinel"),
    ("jedi_knight", "watchman", "sentinel"),
    ("jedi_consular", "balance", "force_wizard"),
    ("jedi_consular", "seer", "force_wizard"),
    ("jedi_consular", "telekinetics", "force_wizard"),
    ("jedi_consular", "combat", "shadow"),
    ("jedi_consular", "infiltration", "shadow"),
    ("jedi_consular", "serenity", "shadow"),
    ("trooper", "assault_specialist", "commando"),
    ("trooper", "combat_medic", "commando"),
    ("trooper", "gunnery", "commando"),
    ("trooper", "plasmatech", "specialist"),
    ("trooper", "shield_specialist", "specialist"),
    ("trooper", "tactics", "specialist"),
    ("smuggler", "ruffian", "scoundrel"),
    ("smuggler", "sawbones", "scoundrel"),
    ("smuggler", "scrapper", "scoundrel"),
    ("smuggler", "dirty_fighting", "gunslinger"),
    ("smuggler", "saboteur", "gunslinger"),
    ("smuggler", "sharpshooter", "gunslinger"),
];

/// Origin -> 2 combat styles. Used to fan per-origin shared/utility pools
/// (abl.<origin>.<name>, abl.<origin>.skill.utility.*, abl.<origin>.skill.mods.*,
/// tal.<origin>.skill.utility.*) into combat_style_shared_abilities and
/// class_utility_talents.
pub fn origin_combat_styles(origin: &str) -> &'static [&'static str] {
    match origin {
        "sith_warrior" => &["juggernaut", "marauder"],
        "sith_inquisitor" => &["assassin", "sorcerer"],
        "bounty_hunter" => &["powertech", "mercenary"],
        "agent" => &["operative", "sniper"],
        "jedi_knight" => &["guardian", "sentinel"],
        "jedi_consular" => &["shadow", "force_wizard"],
        "trooper" => &["specialist", "commando"],
        "smuggler" => &["scoundrel", "gunslinger"],
        _ => &[],
    }
}

pub fn combat_style_for(origin: &str, discipline: &str) -> Option<&'static str> {
    DISCIPLINE_COMBAT_STYLE_MAP
        .iter()
        .find(|(o, d, _)| *o == origin && *d == discipline)
        .map(|(_, _, cs)| *cs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every combat-style value emitted by `combat_style_for` must also appear
    /// in `origin_combat_styles(origin)`. The original FK gap shipped because
    /// the two maps drifted: `combat_style_for` returned 'sage'/'vanguard' while
    /// `origin_combat_styles` returned 'shadow'/'sage' and 'vanguard'/'commando'.
    /// This test catches that class of regression at `cargo test` time.
    #[test]
    fn discipline_map_combat_styles_match_origin_fan_out() {
        for (origin, discipline, combat_style) in DISCIPLINE_COMBAT_STYLE_MAP {
            let fan_out = origin_combat_styles(origin);
            assert!(
                fan_out.contains(combat_style),
                "({origin}, {discipline}) -> {combat_style} not in origin_combat_styles({origin}) = {fan_out:?}",
            );
        }
    }

    #[test]
    fn place_ability_routes_core_choice_and_shared_pools() {
        assert_eq!(
            place_ability("abl.sith_warrior.skill.vengeance.ravage"),
            Some(AbilityPlacement::Discipline {
                origin: "sith_warrior".into(),
                discipline_name: "vengeance".into(),
                fqn_prefix: "abl.sith_warrior.skill.vengeance".into(),
                combat_style: "juggernaut",
                tier_level: None,
                slot_type: "core",
            })
        );
        let Some(AbilityPlacement::Discipline {
            slot_type,
            tier_level,
            ..
        }) = place_ability("abl.sith_warrior.skill.vengeance.mods.tier2.x")
        else {
            panic!("tier2 mod should place in its discipline");
        };
        assert_eq!((slot_type, tier_level), ("choice", Some(23)));
        assert_eq!(
            place_ability("abl.sith_warrior.force_leap"),
            Some(AbilityPlacement::Shared {
                origin: "sith_warrior".into(),
                slot_type: "class_shared",
            })
        );
        assert_eq!(
            place_ability("abl.sith_warrior.skill.utility.thwart"),
            Some(AbilityPlacement::Shared {
                origin: "sith_warrior".into(),
                slot_type: "utility",
            })
        );
        assert_eq!(place_ability("abl.companion.attack"), None);
        assert_eq!(place_ability("abl.sith_warrior.skill.nope.x"), None);
    }

    #[test]
    fn place_talent_routes_utility_and_discipline() {
        assert_eq!(
            place_talent("tal.sith_warrior.skill.utility.interloper"),
            Some(TalentPlacement::Utility {
                origin: "sith_warrior".into()
            })
        );
        assert_eq!(
            place_talent("tal.sith_warrior.skill.vengeance.unyielding"),
            Some(TalentPlacement::Discipline {
                fqn_prefix: "abl.sith_warrior.skill.vengeance".into()
            })
        );
        assert_eq!(place_talent("tal.spvp.engine.barrel_roll.tier2"), None);
    }
}
