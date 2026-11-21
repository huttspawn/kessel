# Generated from issue #430 (a defect; it traces to no legion requirement).
# Do not hand-edit: change the issue and regenerate.

Feature: Each dialogue line is spoken by the speaker its node names

  @criterion-1 @local
  Scenario: A player choice is spoken by the player
    Given a build holding a conversation with a player choice
    When kessel's agent reads that conversation's script
    Then the choice is spoken by the player
    And it names the NPC the player is talking to

  @criterion-2 @local
  Scenario: An NPC line names the NPC the node names
    Given a build holding a conversation where two NPCs speak in turn
    When kessel's agent reads that conversation's script
    Then each NPC line names the NPC its own node names as speaker

  @criterion-3 @local
  Scenario: A placeable can speak
    Given a build holding a conversation spoken by a terminal
    When kessel's agent reads that conversation's script
    Then the terminal's line names the terminal as its speaker

  @criterion-4 @local
  Scenario: A speaker the build does not hold is kept, not guessed
    Given a build holding a conversation line whose speaker is not an extracted object
    When kessel's agent reads that conversation's script
    Then the line keeps its speaker's GUID
    And it names no speaker
