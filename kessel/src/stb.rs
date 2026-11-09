//! STB (String Table Binary) Parser
//!
//! STB files contain localized strings for SWTOR game objects.
//! They are found at paths like `/resources/en-us/str/abl.stb`.
//!
//! Format (discovered by analysis):
//! Header (7 bytes):
//!   byte 0:      Version (0x01)
//!   bytes 1-2:   Reserved (0x0000)
//!   bytes 3-6:   String count (u32 LE)
//!
//! Entry (26 bytes each):
//!   bytes 0-3:   ID1 (u32 LE)
//!   bytes 4-7:   ID2 (u32 LE)
//!   bytes 8-9:   Flags (u16 LE)
//!   bytes 10-13: Version (u32 LE)
//!   bytes 14-17: Text length (u32 LE)
//!   bytes 18-21: Text offset (u32 LE)
//!   bytes 22-25: Additional length (u32 LE)
//!
//! Text data follows entries at specified offsets.

use crate::hash::{combine_hash, hashlittle2};
use anyhow::{bail, Result};
use std::collections::HashMap;

const HEADER_SIZE: usize = 7;
const ENTRY_SIZE: usize = 26;

/// A single string entry from an STB file
#[derive(Debug, Clone)]
pub struct StbEntry {
    /// Primary ID
    pub id1: u32,
    /// Secondary ID
    pub id2: u32,
    /// Entry flags. Two entries of one (id1, id2) are told apart by these;
    /// the low byte names the kind (see [`variant_kind`]).
    pub flags: u16,
    /// Version number
    pub version: u32,
    /// The localized text
    pub text: String,
}

/// Parsed STB file with metadata
#[derive(Debug, Clone)]
pub struct StbFile {
    /// All string entries
    pub entries: Vec<StbEntry>,
    /// Locale extracted from path (e.g., "en-us")
    pub locale: String,
    /// FQN prefix derived from path (e.g., "str.abl")
    pub fqn_prefix: String,
}

/// Parse an STB file from raw bytes
///
/// # Arguments
/// * `data` - Raw STB file bytes
/// * `path` - File path (used to extract locale and FQN prefix)
///
/// # Returns
/// Parsed StbFile with entries, locale, and FQN prefix
pub fn parse(data: &[u8], path: &str) -> Result<StbFile> {
    if data.len() < HEADER_SIZE {
        bail!("STB file too small: {} bytes", data.len());
    }

    // Verify version byte
    if data[0] != 0x01 {
        bail!("Invalid STB version: {:02X} (expected 0x01)", data[0]);
    }

    // Read string count
    let count = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as usize;

    // Calculate expected minimum size
    let entries_end = HEADER_SIZE + (count * ENTRY_SIZE);
    if data.len() < entries_end {
        bail!(
            "STB file truncated: {} bytes, need {} for {} entries",
            data.len(),
            entries_end,
            count
        );
    }

    let mut entries = Vec::with_capacity(count);

    for i in 0..count {
        let entry_offset = HEADER_SIZE + (i * ENTRY_SIZE);
        let entry_data = &data[entry_offset..entry_offset + ENTRY_SIZE];

        let id1 = u32::from_le_bytes([entry_data[0], entry_data[1], entry_data[2], entry_data[3]]);
        let id2 = u32::from_le_bytes([entry_data[4], entry_data[5], entry_data[6], entry_data[7]]);
        let flags = u16::from_le_bytes([entry_data[8], entry_data[9]]);
        let version = u32::from_le_bytes([
            entry_data[10],
            entry_data[11],
            entry_data[12],
            entry_data[13],
        ]);
        let text_len = u32::from_le_bytes([
            entry_data[14],
            entry_data[15],
            entry_data[16],
            entry_data[17],
        ]) as usize;
        let text_offset = u32::from_le_bytes([
            entry_data[18],
            entry_data[19],
            entry_data[20],
            entry_data[21],
        ]) as usize;
        // bytes 22-25 are additional_length, not needed for basic parsing

        // Read text at offset
        let text = if text_len > 0 && text_offset + text_len <= data.len() {
            let text_bytes = &data[text_offset..text_offset + text_len];
            // STB text is UTF-8, but may have null terminator
            let text_str = String::from_utf8_lossy(text_bytes);
            text_str.trim_end_matches('\0').to_string()
        } else {
            String::new()
        };

        entries.push(StbEntry {
            id1,
            id2,
            flags,
            version,
            text,
        });
    }

    let locale = extract_locale_from_path(path);
    let fqn_prefix = extract_fqn_from_path(path);

    Ok(StbFile {
        entries,
        locale,
        fqn_prefix,
    })
}

/// The kind of a second entry of an (id1, id2) key, from its flags' low
/// byte: 0x50 is a dialogue option's wheel text, 0x46 a female variant.
pub fn variant_kind(flags: u16) -> &'static str {
    match flags & 0xff {
        0x50 => "wheel",
        0x46 => "female",
        _ => "variant",
    }
}

/// For each entry, whether it is the one `strings` keeps for its (id1, id2):
/// the highest `version`, the first on a tie. The rest are variants.
pub fn primary_mask(entries: &[StbEntry]) -> Vec<bool> {
    let mut winner: HashMap<(u32, u32), usize> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        winner
            .entry((e.id1, e.id2))
            .and_modify(|w| {
                if e.version > entries[*w].version {
                    *w = i;
                }
            })
            .or_insert(i);
    }
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| winner[&(e.id1, e.id2)] == i)
        .collect()
}

/// Extract locale from STB file path
///
/// Path format: `/resources/en-us/str/abl.stb`
/// Returns: "en-us"
fn extract_locale_from_path(path: &str) -> String {
    // Look for pattern /resources/{locale}/str/
    let normalized = path.to_lowercase().replace('\\', "/");

    if let Some(resources_pos) = normalized.find("/resources/") {
        let after_resources = &normalized[resources_pos + 11..]; // skip "/resources/"
        if let Some(slash_pos) = after_resources.find('/') {
            return after_resources[..slash_pos].to_string();
        }
    }

    // Default to en-us if we can't parse
    "en-us".to_string()
}

/// Extract FQN prefix from STB file path
///
/// Path format: `/resources/en-us/str/abl.stb`
/// Returns: "str.abl"
///
/// Path format: `/resources/en-us/str/abl/agent/skill.stb`
/// Returns: "str.abl.agent.skill"
fn extract_fqn_from_path(path: &str) -> String {
    let normalized = path.to_lowercase().replace('\\', "/");

    // Find /str/ and extract everything after it
    if let Some(str_pos) = normalized.find("/str/") {
        let after_str = &normalized[str_pos + 5..]; // skip "/str/"

        // Remove .stb extension and convert slashes to dots
        let without_ext = after_str.trim_end_matches(".stb");
        let fqn_suffix = without_ext.replace('/', ".");

        return format!("str.{}", fqn_suffix);
    }

    // Fallback: just use filename without extension
    if let Some(filename) = normalized.rsplit('/').next() {
        let without_ext = filename.trim_end_matches(".stb");
        return format!("str.{}", without_ext);
    }

    "str.unknown".to_string()
}

/// The STB tables extracted from the archive, by path. Each is found by the
/// hash of its path, so no dictionary is needed. Root tables hold the primary
/// object kinds; the gui tables (every `/str/gui/` table in the archives, 178
/// on the live game) hold the interface text the root tables lack, such as
/// conquest themes, achievement categories, guild and mail text; the pc tables
/// hold player and legacy title text. The list names every en-us table outside
/// `/str/cnv/` in the live archives (288). Conversation tables (`/str/cnv/`) are not listed: they are
/// derived from the `cnv.*` objects in `populate_conversation_strings`.
const KNOWN_STB_PATHS: &[&str] = &[
    "/resources/en-us/str/abl.stb",
    "/resources/en-us/str/tal.stb",
    "/resources/en-us/str/itm.stb",
    "/resources/en-us/str/npc.stb",
    "/resources/en-us/str/qst.stb",
    "/resources/en-us/str/cdx.stb",
    "/resources/en-us/str/ach.stb",
    "/resources/en-us/str/gui/abilities.stb",
    "/resources/en-us/str/gui/abilitypanel.stb",
    "/resources/en-us/str/gui/abl/player/skill_trees.stb",
    "/resources/en-us/str/gui/achievementcategories.stb",
    "/resources/en-us/str/gui/achievementwindow.stb",
    "/resources/en-us/str/gui/activities.stb",
    "/resources/en-us/str/gui/advanceddescriptions.stb",
    "/resources/en-us/str/gui/advancednames.stb",
    "/resources/en-us/str/gui/alignment.stb",
    "/resources/en-us/str/gui/alliance.stb",
    "/resources/en-us/str/gui/amplifiers.stb",
    "/resources/en-us/str/gui/arenascorecard.stb",
    "/resources/en-us/str/gui/atf/primary_trait/name.stb",
    "/resources/en-us/str/gui/atf/primary_trait/tooltip.stb",
    "/resources/en-us/str/gui/atf/secondary_trait/desc.stb",
    "/resources/en-us/str/gui/atf/secondary_trait/effect_desc.stb",
    "/resources/en-us/str/gui/atf/secondary_trait/name.stb",
    "/resources/en-us/str/gui/auctionhouse.stb",
    "/resources/en-us/str/gui/backgrounddescriptions.stb",
    "/resources/en-us/str/gui/backgroundnames.stb",
    "/resources/en-us/str/gui/badges.stb",
    "/resources/en-us/str/gui/bank.stb",
    "/resources/en-us/str/gui/bug.stb",
    "/resources/en-us/str/gui/charactercreate.stb",
    "/resources/en-us/str/gui/characterselect.stb",
    "/resources/en-us/str/gui/charactersheet.stb",
    "/resources/en-us/str/gui/charcreateabilities.stb",
    "/resources/en-us/str/gui/charcreateabilitiestitle.stb",
    "/resources/en-us/str/gui/charcreateadvancedbullets.stb",
    "/resources/en-us/str/gui/charcreateadvance.stb",
    "/resources/en-us/str/gui/charcreatecombatrole.stb",
    "/resources/en-us/str/gui/charcreatecombatroletitle.stb",
    "/resources/en-us/str/gui/charcreateequipment.stb",
    "/resources/en-us/str/gui/charcreateequipmenttitle.stb",
    "/resources/en-us/str/gui/charcreatestory.stb",
    "/resources/en-us/str/gui/charcreatestorytitle.stb",
    "/resources/en-us/str/gui/chat.stb",
    "/resources/en-us/str/gui/classdescriptions.stb",
    "/resources/en-us/str/gui/classnames.stb",
    "/resources/en-us/str/gui/codexwindow.stb",
    "/resources/en-us/str/gui/collections.stb",
    "/resources/en-us/str/gui/collection.stb",
    "/resources/en-us/str/gui/colornames.stb",
    "/resources/en-us/str/gui/combat.stb",
    "/resources/en-us/str/gui/companiongallery.stb",
    "/resources/en-us/str/gui/companiongifttypes.stb",
    "/resources/en-us/str/gui/companionmissions.stb",
    "/resources/en-us/str/gui/companion.stb",
    "/resources/en-us/str/gui/container.stb",
    "/resources/en-us/str/gui/contextmenu.stb",
    "/resources/en-us/str/gui/conversationforceescapedialog.stb",
    "/resources/en-us/str/gui/conversationgui.stb",
    "/resources/en-us/str/gui/conversationreactions.stb",
    "/resources/en-us/str/gui/conversation.stb",
    "/resources/en-us/str/gui/conversionscreen.stb",
    "/resources/en-us/str/gui/crafting.stb",
    "/resources/en-us/str/gui/craftingtrainer.stb",
    "/resources/en-us/str/gui/credits.stb",
    "/resources/en-us/str/gui/crewskills.stb",
    "/resources/en-us/str/gui/customersupport.stb",
    "/resources/en-us/str/gui/death.stb",
    "/resources/en-us/str/gui/deconstruction.stb",
    "/resources/en-us/str/gui/dialog.stb",
    "/resources/en-us/str/gui/disciplines.stb",
    "/resources/en-us/str/gui/disciplinewindow.stb",
    "/resources/en-us/str/gui/dressingroom.stb",
    "/resources/en-us/str/gui/dueling.stb",
    "/resources/en-us/str/gui/effectresults.stb",
    "/resources/en-us/str/gui/effect.stb",
    "/resources/en-us/str/gui/elevator.stb",
    "/resources/en-us/str/gui/elev_dest_descriptions.stb",
    "/resources/en-us/str/gui/elev_dest_names.stb",
    "/resources/en-us/str/gui/elev_package_display_names.stb",
    "/resources/en-us/str/gui/emotegrouping.stb",
    "/resources/en-us/str/gui/emotes.stb",
    "/resources/en-us/str/gui/energybar.stb",
    "/resources/en-us/str/gui/entitlements.stb",
    "/resources/en-us/str/gui/equipslot.stb",
    "/resources/en-us/str/gui/experiencebar.stb",
    "/resources/en-us/str/gui/facialreconstruction.stb",
    "/resources/en-us/str/gui/ffhangar.stb",
    "/resources/en-us/str/gui/flashpoint.stb",
    "/resources/en-us/str/gui/flytext.stb",
    "/resources/en-us/str/gui/galacticcommand.stb",
    "/resources/en-us/str/gui/galaxymap.stb",
    "/resources/en-us/str/gui/gameoptions.stb",
    "/resources/en-us/str/gui/gcpackexplorer.stb",
    "/resources/en-us/str/gui/gift.stb",
    "/resources/en-us/str/gui/groupfinder.stb",
    "/resources/en-us/str/gui/grouplootrules.stb",
    "/resources/en-us/str/gui/group.stb",
    "/resources/en-us/str/gui/guildbank.stb",
    "/resources/en-us/str/gui/guildlog.stb",
    "/resources/en-us/str/gui/guild.stb",
    "/resources/en-us/str/gui/guildwindow.stb",
    "/resources/en-us/str/gui/helptips.stb",
    "/resources/en-us/str/gui/hyperlinks.stb",
    "/resources/en-us/str/gui/iconbrowser.stb",
    "/resources/en-us/str/gui/instance.stb",
    "/resources/en-us/str/gui/inventory.stb",
    "/resources/en-us/str/gui/itemenhancement.stb",
    "/resources/en-us/str/gui/item_source.stb",
    "/resources/en-us/str/gui/items.stb",
    "/resources/en-us/str/gui/itm/enhancement/categories.stb",
    "/resources/en-us/str/gui/itm/enhancement/messages.stb",
    "/resources/en-us/str/gui/itm/enhancement/stations.stb",
    "/resources/en-us/str/gui/itm/enhancement/types.stb",
    "/resources/en-us/str/gui/itm/loot/quality.stb",
    "/resources/en-us/str/gui/itm/modifiers.stb",
    "/resources/en-us/str/gui/itm/setbonuses.stb",
    "/resources/en-us/str/gui/kbarticlelinks.stb",
    "/resources/en-us/str/gui/keysabbreviated.stb",
    "/resources/en-us/str/gui/keys.stb",
    "/resources/en-us/str/gui/legacy.stb",
    "/resources/en-us/str/gui/lockoutwindow.stb",
    "/resources/en-us/str/gui/loginrewards.stb",
    "/resources/en-us/str/gui/loot.stb",
    "/resources/en-us/str/gui/macrobinoculars.stb",
    "/resources/en-us/str/gui/mail.stb",
    "/resources/en-us/str/gui/map.stb",
    "/resources/en-us/str/gui/misc.stb",
    "/resources/en-us/str/gui/missionboard.stb",
    "/resources/en-us/str/gui/missionlog.stb",
    "/resources/en-us/str/gui/missionreward.stb",
    "/resources/en-us/str/gui/mtxdiscounts.stb",
    "/resources/en-us/str/gui/mtxpackexplorer.stb",
    "/resources/en-us/str/gui/mtxstorefrontitems.stb",
    "/resources/en-us/str/gui/mtxstorefront.stb",
    "/resources/en-us/str/gui/options.stb",
    "/resources/en-us/str/gui/partybar.stb",
    "/resources/en-us/str/gui/planetaryconquest.stb",
    "/resources/en-us/str/gui/plc/generic/medcenters.stb",
    "/resources/en-us/str/gui/portraitbar.stb",
    "/resources/en-us/str/gui/preferences.stb",
    "/resources/en-us/str/gui/profession.stb",
    "/resources/en-us/str/gui/pvp.stb",
    "/resources/en-us/str/gui/qa/gameplay_persistence/loc_refactor/pnewman_dupe_01.stb",
    "/resources/en-us/str/gui/qa/gameplay_persistence/loc_refactor/pnewman_unique5_01.stb",
    "/resources/en-us/str/gui/qa/gameplay_persistence/loc_service_test/pnewman05_0dev_gui_frozen.stb",
    "/resources/en-us/str/gui/qa/gameplay_persistence/loc_service_test/pnewman06_0tech_gui_frozen.stb",
    "/resources/en-us/str/gui/questcategories.stb",
    "/resources/en-us/str/gui/quest.stb",
    "/resources/en-us/str/gui/queue.stb",
    "/resources/en-us/str/gui/quickslots.stb",
    "/resources/en-us/str/gui/quicktravel.stb",
    "/resources/en-us/str/gui/raid.stb",
    "/resources/en-us/str/gui/rollpriority.stb",
    "/resources/en-us/str/gui/scaleform.stb",
    "/resources/en-us/str/gui/scoreboardbadges.stb",
    "/resources/en-us/str/gui/scoreboard.stb",
    "/resources/en-us/str/gui/scorecard.stb",
    "/resources/en-us/str/gui/sc_ships.stb",
    "/resources/en-us/str/gui/securetrade.stb",
    "/resources/en-us/str/gui/selinescorecard.stb",
    "/resources/en-us/str/gui/shardselect.stb",
    "/resources/en-us/str/gui/sharemission.stb",
    "/resources/en-us/str/gui/shipscorecard.stb",
    "/resources/en-us/str/gui/skilltree.stb",
    "/resources/en-us/str/gui/spacecombat_items.stb",
    "/resources/en-us/str/gui/spacecombat.stb",
    "/resources/en-us/str/gui/splitstack.stb",
    "/resources/en-us/str/gui/stats.stb",
    "/resources/en-us/str/gui/stronghold.stb",
    "/resources/en-us/str/gui/system.stb",
    "/resources/en-us/str/gui/targetbar.stb",
    "/resources/en-us/str/gui/taxi.stb",
    "/resources/en-us/str/gui/test/qa/bvt/loc_db_test.stb",
    "/resources/en-us/str/gui/test/qa/jgibbs/54515.stb",
    "/resources/en-us/str/gui/test/qa/jgibbs/test.stb",
    "/resources/en-us/str/gui/tooltips.stb",
    "/resources/en-us/str/gui/trade.stb",
    "/resources/en-us/str/gui/trainer.stb",
    "/resources/en-us/str/gui/tutorials.stb",
    "/resources/en-us/str/gui/updateinfo.stb",
    "/resources/en-us/str/gui/vendor.stb",
    "/resources/en-us/str/gui/wardrobe.stb",
    "/resources/en-us/str/gui/warzonejoin.stb",
    "/resources/en-us/str/gui/wholist.stb",
    "/resources/en-us/str/pc/title.stb",
    "/resources/en-us/str/pc/legacytitle.stb",
    "/resources/en-us/str/abl/player/grant_codex_xp.stb",
    "/resources/en-us/str/abl/player/grant_exploration_xp.stb",
    "/resources/en-us/str/abl/player/grant_quest_xp.stb",
    "/resources/en-us/str/abl/player/harvest.stb",
    "/resources/en-us/str/abl/player/loot.stb",
    "/resources/en-us/str/abl/player/mount.stb",
    "/resources/en-us/str/abl/player/proficiency.stb",
    "/resources/en-us/str/abl/player/recently_died.stb",
    "/resources/en-us/str/abl/player/summon_companion.stb",
    "/resources/en-us/str/abl/pvp/guard_stancereq.stb",
    "/resources/en-us/str/abl/pvp/warzone.stb",
    "/resources/en-us/str/ahd.stb",
    "/resources/en-us/str/apt.stb",
    "/resources/en-us/str/cnd.stb",
    "/resources/en-us/str/dec.stb",
    "/resources/en-us/str/lgc/dailylogin.stb",
    "/resources/en-us/str/lgc/reputation.stb",
    "/resources/en-us/str/lgc/unlocks.stb",
    "/resources/en-us/str/lky.stb",
    "/resources/en-us/str/misc/mail/alliance.stb",
    "/resources/en-us/str/misc/mail/bounty_hunter.stb",
    "/resources/en-us/str/misc/mail/chapter_1_imp.stb",
    "/resources/en-us/str/misc/mail/chapter_1_rep.stb",
    "/resources/en-us/str/misc/mail/chapter_2_imp.stb",
    "/resources/en-us/str/misc/mail/chapter_2_rep.stb",
    "/resources/en-us/str/misc/mail/chapter_3_imp.stb",
    "/resources/en-us/str/misc/mail/chapter_3_rep.stb",
    "/resources/en-us/str/misc/mail/chapter_4_imp.stb",
    "/resources/en-us/str/misc/mail/chapter_4_rep.stb",
    "/resources/en-us/str/misc/mail/chapter_5_imp.stb",
    "/resources/en-us/str/misc/mail/chapter_5_rep.stb",
    "/resources/en-us/str/misc/mail/chapter_5_shared.stb",
    "/resources/en-us/str/misc/mail/companion.stb",
    "/resources/en-us/str/misc/mail/daily_area.stb",
    "/resources/en-us/str/misc/mail/date_night.stb",
    "/resources/en-us/str/misc/mail/endgame.stb",
    "/resources/en-us/str/misc/mail/event.stb",
    "/resources/en-us/str/misc/mail/exp_03.stb",
    "/resources/en-us/str/misc/mail/exp_04.stb",
    "/resources/en-us/str/misc/mail/flashpoint.stb",
    "/resources/en-us/str/misc/mail/imperial_agent.stb",
    "/resources/en-us/str/misc/mail/jedi_consular.stb",
    "/resources/en-us/str/misc/mail/jedi_knight.stb",
    "/resources/en-us/str/misc/mail/kotet.stb",
    "/resources/en-us/str/misc/mail/kotfe_season_1.stb",
    "/resources/en-us/str/misc/mail/sith_inquisitor.stb",
    "/resources/en-us/str/misc/mail/sith_warrior.stb",
    "/resources/en-us/str/misc/mail/smuggler.stb",
    "/resources/en-us/str/misc/mail/star_fortress.stb",
    "/resources/en-us/str/misc/mail/strongholds.stb",
    "/resources/en-us/str/misc/mail/support_content.stb",
    "/resources/en-us/str/misc/mail/system_messages.stb",
    "/resources/en-us/str/misc/mail/trooper.stb",
    "/resources/en-us/str/misc/mail/ventures.stb",
    "/resources/en-us/str/mnt/mounts.stb",
    "/resources/en-us/str/mpn.stb",
    "/resources/en-us/str/mrp.stb",
    "/resources/en-us/str/nco.stb",
    "/resources/en-us/str/pc/flair.stb",
    "/resources/en-us/str/pc/titles/prefix.stb",
    "/resources/en-us/str/pc/titles/suffix.stb",
    "/resources/en-us/str/pcs.stb",
    "/resources/en-us/str/plc.stb",
    "/resources/en-us/str/plc/generic/containers.stb",
    "/resources/en-us/str/plc/generic/environment.stb",
    "/resources/en-us/str/prf/items.stb",
    "/resources/en-us/str/prf/missions.stb",
    "/resources/en-us/str/prf/professions.stb",
    "/resources/en-us/str/prf/schematics.stb",
    "/resources/en-us/str/prf/subtypes.stb",
    "/resources/en-us/str/prf/system.stb",
    "/resources/en-us/str/prf/workstations.stb",
    "/resources/en-us/str/rdd.stb",
    "/resources/en-us/str/spvp/component_types.stb",
    "/resources/en-us/str/spvp/components.stb",
    "/resources/en-us/str/spvp/crew.stb",
    "/resources/en-us/str/spvp/drones.stb",
    "/resources/en-us/str/spvp/misc.stb",
    "/resources/en-us/str/spvp/roles.stb",
    "/resources/en-us/str/spvp/ships.stb",
    "/resources/en-us/str/spvp/stats.stb",
    "/resources/en-us/str/svy.stb",
    "/resources/en-us/str/sys/cnvconfirmationtext.stb",
    "/resources/en-us/str/sys/codexcategories.stb",
    "/resources/en-us/str/sys/contactcategories.stb",
    "/resources/en-us/str/sys/currencycategory.stb",
    "/resources/en-us/str/sys/factions.stb",
    "/resources/en-us/str/sys/flashpoint.stb",
    "/resources/en-us/str/sys/groupfinder.stb",
    "/resources/en-us/str/sys/localizedslashcommands.stb",
    "/resources/en-us/str/sys/pvp.stb",
    "/resources/en-us/str/sys/searchtags.stb",
    "/resources/en-us/str/sys/shardnames.stb",
    "/resources/en-us/str/sys/signs.stb",
    "/resources/en-us/str/sys/tips.stb",
    "/resources/en-us/str/sys/titles.stb",
    "/resources/en-us/str/sys/worldmap.stb",
    "/resources/en-us/str/tax/terminals.stb",
    "/resources/en-us/str/test/abl.stb",
    "/resources/en-us/str/test/dec.stb",
    "/resources/en-us/str/test/itm.stb",
];

/// Archive filename hash -> path for every known STB table. The hash is
/// `combine_hash(ph, sh)`, the key archive entries carry.
pub fn known_stb_hashes() -> HashMap<u64, &'static str> {
    KNOWN_STB_PATHS
        .iter()
        .map(|path| {
            let (ph, sh) = hashlittle2(path.as_bytes(), 0, 0);
            (combine_hash(ph, sh), *path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id1: u32, flags: u16, version: u32, text: &str) -> StbEntry {
        StbEntry {
            id1,
            id2: 7,
            flags,
            version,
            text: text.to_string(),
        }
    }

    #[test]
    fn primary_is_the_higher_version_then_the_first() {
        let entries = [
            entry(16, 0x41, 1, "Baron <<1>>"),
            entry(16, 0x46, 1, "Baroness <<1>>"),
            entry(17, 0x50, 1, "old"),
            entry(17, 0x41, 2, "new"),
            entry(18, 0x41, 1, "alone"),
        ];
        assert_eq!(primary_mask(&entries), [true, false, false, true, true]);
    }

    #[test]
    fn variant_kind_reads_the_low_flag_byte() {
        assert_eq!(variant_kind(0x0050), "wheel");
        assert_eq!(variant_kind(0x1050), "wheel");
        assert_eq!(variant_kind(0x0046), "female");
        assert_eq!(variant_kind(0x0041), "variant");
    }

    #[test]
    fn test_extract_locale() {
        assert_eq!(
            extract_locale_from_path("/resources/en-us/str/abl.stb"),
            "en-us"
        );
        assert_eq!(
            extract_locale_from_path("/resources/de-de/str/itm.stb"),
            "de-de"
        );
        assert_eq!(
            extract_locale_from_path("/resources/fr-fr/str/npc.stb"),
            "fr-fr"
        );
    }

    #[test]
    fn test_extract_fqn() {
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/abl.stb"),
            "str.abl"
        );
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/abl/agent/skill.stb"),
            "str.abl.agent.skill"
        );
        assert_eq!(
            extract_fqn_from_path("/resources/en-us/str/itm/loot/quality.stb"),
            "str.itm.loot.quality"
        );
    }

    #[test]
    fn known_stb_hashes_match_dictionary() {
        // Real PH/SH pairs from hashes_filename.txt.
        let cases: &[(&str, u32, u32)] = &[
            ("/resources/en-us/str/abl.stb", 0x8154_956D, 0x5430_5B3B),
            ("/resources/en-us/str/tal.stb", 0x93A7_AD6F, 0x5DE1_7C3F),
            ("/resources/en-us/str/itm.stb", 0x32EF_86D8, 0xFB5D_CB98),
            ("/resources/en-us/str/npc.stb", 0xB2A2_6731, 0x0DDC_0C21),
            ("/resources/en-us/str/qst.stb", 0xD097_4307, 0x8ED1_2356),
            ("/resources/en-us/str/cdx.stb", 0xE488_9EE0, 0x10A7_098C),
            ("/resources/en-us/str/ach.stb", 0x8BBE_1958, 0x45F4_B329),
            (
                "/resources/en-us/str/gui/planetaryconquest.stb",
                0xA3B4_7B7C,
                0x1203_52AA,
            ),
            (
                "/resources/en-us/str/gui/galacticcommand.stb",
                0x7552_57CA,
                0xE929_6959,
            ),
            (
                "/resources/en-us/str/gui/achievementcategories.stb",
                0x2B44_73F6,
                0x98B3_EA51,
            ),
            (
                "/resources/en-us/str/gui/guild.stb",
                0xBE3C_4C10,
                0xAB20_DDDA,
            ),
            (
                "/resources/en-us/str/gui/mail.stb",
                0x8E7A_A9CF,
                0x4A71_3705,
            ),
            (
                "/resources/en-us/str/pc/title.stb",
                0x3E55_0C96,
                0xB6B2_B127,
            ),
            (
                "/resources/en-us/str/pc/legacytitle.stb",
                0xDA28_7E12,
                0xB7A1_BE36,
            ),
            ("/resources/en-us/str/cnd.stb", 0x3956_95A3, 0xFB71_F0E0),
            ("/resources/en-us/str/ahd.stb", 0xB3C0_4BD8, 0x649A_4539),
            (
                "/resources/en-us/str/abl/player/loot.stb",
                0x11ED_D9D1,
                0x5190_E498,
            ),
        ];
        let derived = known_stb_hashes();
        assert_eq!(derived.len(), KNOWN_STB_PATHS.len());
        assert_eq!(
            derived.len(),
            288,
            "every en-us string table outside /str/cnv/ in the live archives"
        );
        let gui = derived.values().filter(|p| p.contains("/str/gui/")).count();
        assert_eq!(gui, 178, "every gui string table in the live archives");
        for (path, ph, sh) in cases {
            assert_eq!(derived.get(&combine_hash(*ph, *sh)), Some(path));
        }
    }
}
