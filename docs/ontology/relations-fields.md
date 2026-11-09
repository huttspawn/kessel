# Field relation catalog

`relations-fields.jsonl` lists every link one game object makes to another through a field: one line per (source class, field id, target class). It covers 726,576 objects and singletons from the live 7.x install, unfiltered, and every one of them was read (0 failures). This is catalog 2 of the three ontology catalogs (memory 01a1c255); positional, script-flow, string-id and log links belong to `relations-derived.jsonl`.

## The finding that settles resolution

**A GOM GUID is the fold of its own FQN.** For all 616,931 canonical objects, `guid == 0xE000<<48 | fold48(fnv1a64(upper(fqn)))`. This is the same fold RESEARCH 01a1c0ce-447e proved for achievement event codes. The only exceptions are the 108,717 non-canonical ability effect objects, which have their own GUIDs. So:

- An achievement "event code" that names an NPC, quest or codex is just that object's GUID. Event-code links resolve like any other GUID, and the catalog files them under `value_form: GUID`.
- An unresolved GUID that equals the fold of a string seen anywhere in the data names something the extract does not load. 42,254 unresolved values do this. They have rows of their own with `target_loaded: false`: `cbtWeaponSpec` names `wpn.*` weapon specs (24,149), spawners and `spnMedCenters` name `med.*` medical centres, quests name `veh.*` vehicles and lettered encounter variants (`a:enc.*`), and conversations name NPCs and placeables that are no longer loaded.
- A GUID is stable for as long as the FQN is, which bears on the identity rules (memory 019deac3: "GUIDs shift on patch"). A GUID changes only when the object is renamed.

## Method

1. `kessel-discovery/examples/relations_fields.rs` loads every object from `objects` (GUID, `kind`, FQN, header and payload) and every row of `singletons` (GUID = header bytes 0..8, little-endian) in the unfiltered extract. It decodes each one with kessel's own reader: `gom_reader::read_framed_object`, or for conversations the graph root after the 33-byte preamble, as `field_sweep` does. It walks every field, nested object, list element, map key and map value.
2. A value is a reference candidate when it is:
   - a u64 or i64 whose top 16 bits are `0xE000`, looked up by GUID;
   - a class reference, looked up by GUID;
   - a string shaped like an FQN whose first segment is a prefix some loaded FQN uses (538 prefixes), looked up first by exact FQN, then by stripping trailing segments (`qst.x.is_on_quest` -> `qst.x`, form `FQN string plus member suffix`).
3. A candidate's **target class is the `kind` of the object it resolves to** in the extract, not the registry's `ref_kinds`. GSF component kinds (`*_scs`, 200+ of them, which make up the "GSF stat names" in `ref_kinds`) collapse to `scs`. Each singleton is its own class, `singleton:<name>`.
4. The link is filed under the field that holds the value; `position` says whether it sat in the field itself, a list, a map key or a map value. `nested_path` is the most common chain of ancestor field ids, and is null when the field sits at the top level.
5. `kessel-discovery/examples/relations_fields.py` joins field names (`docs/fields/names.json`, proved by hash) and meanings (`docs/fields/registry.jsonl`), then writes the catalog. 34 identity fields, where 90%+ of a field's links point back at the object holding it, are left out of the catalog and listed below.

Rerun:

```sh
cargo build --release -p kessel-discovery --example relations_fields
./target/release/examples/relations_fields <unfiltered.sqlite> raw.json
python3 -I kessel-discovery/examples/relations_fields.py raw.json docs/fields/registry.jsonl docs/ontology/relations-fields.jsonl stats.json
```

### Columns

`field_name` is the name proved by hash, or null. `field_meaning` is the registry's decoded meaning, where one exists. `meaning` is a verb phrase: 166 rows are written by hand (`meaning_by: hand`, kept in `relations_fields_verbs.json`), and the rest are phrased from the field name. `per_source` is `one` when no source holds more than one link of this relation. `sources_per_target_max` and `sources_per_target_mean` show the fan-in. `resolve_share` is resolved over candidate values for the (source class, field) across all forms, so it is the share of that field's reference-shaped values that land on a loaded object.

### Status

- **proved**: 90% or more of the field's reference-shaped values resolve, by exact GUID or full FQN. An exact 64-bit or full-name match cannot happen by chance, so the count does not matter.
- **likely**: the share is 50-90%; or the link is a member-suffix match (a prefix match, such as a script reading a quest variable); or it is an icon or image field whose asset name happens to equal an FQN (13 rows, marked "asset named like the target"); or the target is not loaded.
- **unsettled**: under 50% of the field's values resolve, so the field may carry something else too.

## Totals

| | Relations | Links |
|---|---:|---:|
| To loaded objects | 1,407 | 5,229,825 |
| proved | 1,315 | 4,906,539 |
| likely | 74 | 322,589 |
| unsettled | 18 | 697 |
| To names the extract does not load (likely) | 92 | 42,254 |
| **All rows** | **1,499** | **5,272,079** |

- The rows cover 629 field ids, 878 (source class, field) pairs, 290 source classes and 538 target classes. Counting each singleton as its own class gives 1,054 class pairs.
- Leaving singletons out, 48 classes link through 208 class pairs. The rannoch-shape toy found 190, but it read only u64 GUIDs and keyed classes by FQN prefix. The registry's baseline of 1,606 pairs counts each GSF stat kind and each FQN prefix separately.
- 6,529,113 values look like references, and 95.3% of them resolve. By form: GUID 4.31M (98.3% resolve), FQN string 1.92M (87.7%), member suffix 213k, class reference 86.7k (99.99%).
- 712 relations have many links per source and 695 have one. 376 have fan-in, where several sources point at one target.
- 728 relations sit inside nested structs, and 504 sit in lists or maps.

## Top relations by count

| Links | Source | Field | Target | Meaning | Form; position | Per source | Sources per target (max) | Status |
|---:|---|---|---|---|---|---|---:|---|
|---:|---|---|---|---|---|---|---:|---|
| 312,452 | Conversation | `4000000bada8873c` cnvSpeaker | Npc | conversation node is spoken by npc | GUID; scalar | many | 193 | proved |
| 307,010 | Conversation | `4000000bada8873f` cnvListener | Npc | conversation node is addressed to npc | GUID; scalar | many | 205 | proved |
| 253,601 | Item | `4000001036f1a231` itmAppearanceSpecByPlayerClass | class | item appearance varies by player class | GUID; map key | many | 31,713 | proved |
| 253,585 | Item | `4000001036f1a231` itmAppearanceSpecByPlayerClass | ItemPaintPattern | item looks like paint pattern for a class | GUID; map value | many | 34 | proved |
| 189,322 | Item | `4000004efc1318f8` cbtActiveDefaultItem | Discipline | item is the default gear for discipline | GUID; map key | many | 4,322 | proved |
| 180,867 | Conversation | `4000000bb7fe66fb` cnvMarkTemplate | Stage | conversation node stages its actors with stage template | FQN string; scalar | many | 718 | proved |
| 158,676 | Quest | `4000000a731cd8be` (unnamed) | Spawn | quest step toggles spawner | GUID; map key | many | 37 | proved |
| 131,359 | Stage | `40000001bc16dc71` stgCameraAutoTemplateFQN | cam | stage uses camera template | FQN string; scalar | many | 3,393 | proved |
| 108,717 | Ability | `4000000a1d6b4918` ablEffectIDs | Ability | ability lists effect | GUID; list element | many | 1 | proved |
| 108,717 | Ability | `4000000a1d6b491c` effAbilitySpec | Ability | effect belongs to ability | GUID; scalar | one | 35 | proved |
| 103,535 | Conversation | `4000000bb7fe66f9` cnvStagePresetMarksNPCs | Npc | conversation stage preset places npc on a mark | GUID; map key | many | 1,034 | proved |
| 68,079 | Stage | `4000000a4e63e824` stgMarkPreviewNPC | Npc | stage mark previews npc | GUID; scalar | many | 3,364 | proved |
| 65,914 | singleton:qstRewardsInfoPrototype | `40000010d9f35ee9` qstRewardsInfoItemClassesRequired | class | quest reward row is limited to class | GUID; map key | many | 1 | proved |
| 63,542 | Hydra | `40000002080ce2d2` hydFQN | Quest | script action names its object (hydFQN) | FQN string plus member suffix, FQN string; scalar | many | 1,467 | likely |
| 63,542 | Hydra | `40000043d5fb02d0` hydTag | Quest | script action tags its object (hydTag) | FQN string plus member suffix, FQN string; scalar | many | 1,467 | likely |
| 59,983 | Item | `4000002f55809161` itmEnhancementDefaults | Item | item comes with default modification item | GUID; list element | many | 1,232 | proved |
| 59,674 | Spawn | `4000000a731cd8ba` (unnamed) | Npc | spawner places npc | GUID; scalar | many | 41 | proved |
| 59,674 | Spawn | `40000049b78d6eec` (unnamed) | Npc | spawner places npc (by FQN) | FQN string; scalar | many | 41 | proved |
| 57,079 | Conversation | `400000426c54c421` cnvReactionNpcId | NpcCompanion | conversation node draws a reaction from companion | GUID; scalar | many | 565 | proved |
| 55,677 | Npc | `4000000a731cd8bf` npcTemplateVisualDataAppearance | NpcPackage | npc wears appearance package | GUID; scalar | many | 6,197 | proved |
| 54,464 | Npc | `4000000a731cd8c0` npcClassPackage | class | npc is of class package | GUID; scalar | one | 18,647 | proved |
| 53,088 | schem | `4000000b48c70717` prfSchematicMaterialOrder | Item | schematic consumes material item (order) | GUID; list element | many | 1,249 | proved |
| 53,077 | schem | `400000099cef75d5` prfSchematicMaterials | Item | schematic consumes material item | GUID; map key | many | 1,249 | proved |
| 53,000 | Npc | `40000009f5129c77` npcTemplateVisualDataMeleeWeapon | Item | npc wields melee weapon item | GUID; scalar | many | 31,351 | proved |
| 52,518 | Npc | `4000000f3bba9abc` npcParentSpecId | Npc | npc inherits from parent npc | GUID; scalar | one | 16,635 | proved |
| 49,273 | Encounter | `40000003d9a52ab9` spnEncounterSpawnerIdsToFqns | Spawn | encounter holds spawner (by FQN) | FQN string; map value | many | 49 | proved |
| 49,273 | Encounter | `400000065f8874e4` spnEncounterSpawnerFqnToProto | Spawn | encounter holds spawner (by class reference) | class reference (GUID); map value | many | 49 | proved |
| 49,273 | Encounter | `40000042d5378130` spnEncounterSpawnerIdsToFqnIds | Spawn | encounter holds spawner (by id) | GUID; map value | many | 49 | proved |
| 47,589 | Item | `4000001036f1a232` itmAppearanceSpec | ItemPaintPattern | item looks like paint pattern | GUID; scalar | one | 131 | proved |
| 47,420 | Hydra | `40000002080ce2d2` hydFQN | Conversation | script action names its object (hydFQN) | FQN string plus member suffix, FQN string; scalar | many | 98 | likely |

By source class, the most links come from Conversation (1.39M), Item (897k), the singletons together (506k), Hydra (459k), Ability (383k), Quest (348k), Npc (306k), Stage (199k) and Spawn (190k). By target class, the most land on Npc (1.29M), Item (481k), Ability (435k), class (408k), ItemPaintPattern (349k), Spawn (339k) and Quest (299k).

## Fields that look like references but don't resolve

309,559 reference-shaped values in 225 (source class, field) pairs resolve to no loaded object. 42,254 of them are folds of names seen elsewhere in the data; those have `unloaded:*` rows. These are the largest:

| Unresolved | Of | Named, not loaded | Source | Field | What they are |
|---:|---:|---:|---|---|---|
| 54,313 | 54,313 | 0 | Npc | `40000007ee70d683` (movement package) | `pkg.movement.*` FQN strings |
| 54,313 | 54,313 | 0 | Npc | `40000007ee70d684` (cover package) | `pkg.cover.*` |
| 52,585 | 52,585 | 0 | Npc | `40000007f6b3b43d` (aggro package) | `pkg.aggro.*` |
| 52,152 | 52,152 | 0 | Npc | `40000007f6b3b43c` (wander package) | `pkg.wander.*` |
| 24,149 | 24,149 | 24,149 | Item | `4000002786306130` cbtWeaponSpec | GUIDs of `wpn.*` weapon specs, not loaded |
| 11,654 | 11,654 | 0 | Quest | `40000044372ac59e` (category tags) | E000 values matching no seen name; the registry reads them as 64-bit tag ids |
| 3,777 | 40,162 | 3,434 | Quest | `400000423de42f90` qstTaskInteractionObjects | `a:enc.*`/`b:enc.*` encounter variants, `veh.*`, cut NPCs |
| 3,272 | 26,480 | 2,182 | Achievement | `4000004c8ce6ac37` achEvents | script event names (`friendly_frond`), not objects |
| 2,764 | 5,581 | 0 | Quest | `40000029e45961b0` qstMissionIcon | `cdx.*` icon names that are not codex FQNs |
| 2,652 | 2,652 | 0 | Npc | `400000037c4de713` (vendor package) | `pkg.vendor.*` |
| 2,641 | 8,156 | 7 | Quest | `4000000a2d7b8798` qstItemSpecId | unknown E000 values (quest-item ids?) |
| 2,279 | 16,201 | 11 | Ability | `40000009ae2f555d` ablCooldownTimerSpecs | timer names (`stealth`, `shoot_leg`) |
| 2,104 | 2,104 | 0 | class | `400000427551a394` (unnamed) | unknown E000 values |
| 2,090 x 2 | 2,090 | 0 | class | `40000007ee70d680/681` | `pkg.movement.*`, `pkg.cover.*` |
| 1,836 / 1,705 | 316,924 / 322,590 | 1,765 / 1,639 | Conversation | cnvListener / cnvSpeaker | NPCs and placeables no longer loaded |
| 1,549 / 434 | same | 0 | Spawn | `40000009b2f107c2/c1` | `pkg.wander.*`, `pkg.aggro.*` |
| 1,250 | 1,629 | 6 | Phase | `4000003067688080` mpnMetadataFullFQN | mostly unmatched; 379 hit `tax`, unsettled |
| 1,217 / 1,192 | 38,834 / 38,648 | 1,058 / 1,042 | singleton:lgcAchievementEventsPrototype | `4000004055f64de0`, `40000045597a88f3` | script event names |
| 1,170 | 85,487 | 1,170 | Spawn | `4000000a731cd8ba` | `med.*` medical centres |
| 1,146 | 1,146 | 1,108 | singleton:spnMedCenterPrototype | spnMedCenters | `med.*` |
| 1,128 | 1,128 | 0 | scs | `4000004cdbcfe3eb` scFFComponentId | GSF component ids with no object |
| 875 | 876 | 260 | singleton:scEncountersProtoData | scEncountersData | `sce.*` GSF encounters |
| 832 | 3,530 | 0 | singleton:cdxCategoryTotalsPrototype | cdx...LookupList | category ids |
| 507 / 506 / 280 | same | 0 | singleton:scFFComponent*Cost / AppearanceData | GSF component ids | GSF component ids with no object |
| 342 | 342 | 0 | scs | scFFComponentIcon | `spvp.*` icon names |
| 326 x 2 | 326 | 0 | singleton:npcTitlesPrototype | npcTitlesLookupList | `npc.title.*` keys; title records, not Npc objects |
| 252 x 3 | 252 | 0 | singleton:lgcUnlockPrototypeMap | lgcUnlockDef* | legacy unlock ids |
| 241 | 241 | 9 | singleton:timerDatatable | tmrDurations | timer names |

The `pkg.*` strings, 222,200 of them, are the largest gap: NPC behaviour packages (movement, cover, aggro, wander, vendor) that the extract does not load. Only 33 `pkg` objects are loaded, so the package class exists in name only. The full list, with samples, is in the run's `stats.json`.

## Identity fields (left out)

These 34 fields point back at the object that holds them (989,729 links), so they are keys, not relations. They are Stage `40000003d0b8ed12`, conEntitySpec (Ability, Item, scs), utlTemplateId (Spawn, ItemPaintPattern, Placeable, NpcPackage, Stage, CharacterPreset), chrNonPlayerCharacterSpec, hydScriptFQN, ablAbilityDataPackage, ablPackageSpec, talTalentDataPackage, effAppearanceName, dynFqn, cnvConversationName (on a Conversation; on an Npc it is the "npc speaks conversation" relation), qstQuestDefinitionFqnId and qstQuestDefinitionName, achSpecId, cmdEmoteCommand, achSteamId, cntAlertOwnerId, ablModalGroup, phsRaidPrototypeFQN, prfTrainerPackageSpec, ablCooldownTimerSpecs (on an Ability, 13,922 of 16,201), hydOptionIdValues (on a Hydra), and `400000442eabc8b0` (Quest, Achievement).

## Unsettled relations

These are the 18 rows where under half of the field's values resolve. The field may be a mix, or it may hold asset names:

- Phase mpnMetadataFullFQN -> tax (379)
- Quest qstFailObjects -> Npc, Placeable, Encounter, Item (85 in all; 24% resolve, and most of the rest are script names such as `PLAYER` and `timer`)
- Quest qstMapNoteAsset -> Phase (8)
- Quest `40000001cef51340` -> Conversation (6)
- singleton:lgcUnlockPrototypeMap `4000003de3844769` -> Quest (160)
- singleton:scShipProtoData scDumbMissileAbility -> Ability (19)
- colItemImage, itmAppearances, mtxStorefrontItemImage -> Item
- aptIconLarge and repGroupInfo*Icon -> Codex
- pkgTrainingCostTable `40000043742cdd31` and prfProfessionAbilitySpec -> Ability
- scEncountersData -> epp (1)

## Limits

- The catalog holds field-to-object links only. Links by string-table id, by position (rooms, spawners in area files), by script flow and by log id belong to the derived catalog.
- `unloaded:*` rows have no per-source or fan-in counts, because the sweep keeps only value counts for unresolved GUIDs.
- A FQN string that names nothing loaded and is not a GUID fold (such as `pkg.*`) is counted as unresolved, not given a row.
- Status is a structural judgement: it says the link is real. It does not say what the link means. `meaning` for the 1,333 rows phrased from the field name is only as good as that name.
