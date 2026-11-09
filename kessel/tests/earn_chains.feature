# Generated from legion requirement FR-KESSEL-025, revision 2026-11-09T20:47:42.586253023+00:00.
# Do not edit: regenerate with scripts/features.mjs.

Feature: Every earnable thing is answered with how it is earned, where, and whether the log can see it

  @criterion-FR-KESSEL-025-c1 @local
  Scenario: An achievement answers what it counts
    Given a build holding a kill-count achievement
    When kessel's agent asks for that achievement
    Then each task is answered with the count it needs and the NPCs it counts

  @criterion-FR-KESSEL-025-c2 @local
  Scenario: An achievement says whether the log can show it
    Given a build holding an achievement counted only by a script event
    When kessel's agent asks for that achievement
    Then the answer says a combat log cannot show its progress and names the script event

  @criterion-FR-KESSEL-025-c3 @local
  Scenario: A buff-conditioned achievement names its buff
    Given a build holding an achievement that counts kills while a toy buff is active
    When kessel's agent asks for that achievement
    Then the answer names the buff's effect

  @criterion-FR-KESSEL-025-c4 @local
  Scenario: A codex entry answers its sources
    Given a build holding a lore codex entry granted by using a placeable
    When kessel's agent asks for that codex entry
    Then the answer names the placeable that grants it

  @criterion-FR-KESSEL-025-c5 @local
  Scenario: A granting placeable answers what and where
    Given a build holding a datacron placeable
    When kessel's agent asks for that placeable
    Then the answer names what it grants
    And gives its area and world position

  @criterion-FR-KESSEL-025-c6 @local
  Scenario: A title answers its grant path
    Given a build holding a title granted by a script
    When kessel's agent asks for that title
    Then the answer names the script that grants it

  @criterion-FR-KESSEL-025-c7 @local
  Scenario: Hidden earnables are listed
    Given a build where an encounter script raises an achievement event no quest step names
    When kessel's agent asks for hidden earnables
    Then that event is listed with its script, its trigger and its achievement

  @criterion-FR-KESSEL-025-c8 @local
  Scenario: An item answers its sources
    Given a build holding an armor piece dropped only by a world event
    When kessel's agent asks for that item
    Then the answer names the world event that drops it

  @criterion-FR-KESSEL-025-c9 @local
  Scenario: A logged object use names the codex it granted
    Given a build holding a lore placeable that grants a codex entry, placed in an area
    And a combat-log line where the player casts 'Examining...' at a position beside that placeable in that area
    When kessel's agent asks what that use granted
    Then the answer names the placeable and its codex entry
  
  @criterion-FR-KESSEL-025-c9 @local
  Scenario: A logged conversation names the codex it granted
    Given a build holding an NPC whose conversation grants a codex entry, placed in an area
    And a combat-log line where the player enters a conversation at a position beside that NPC in that area
    When kessel's agent asks what that conversation granted
    Then the answer names the NPC, its conversation and the codex entry

  @criterion-FR-KESSEL-025-c10 @local
  Scenario: Entering a place names the codex it granted
    Given a build holding a trigger volume that grants a codex entry, placed in an area
    And combat-log lines whose player positions in that area move from outside to inside the volume
    When kessel's agent asks what those positions granted
    Then the answer names the volume and its codex entry
