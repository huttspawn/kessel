# Generated from legion requirement FR-KESSEL-026, revision 2026-11-10T00:49:18.890151946+00:00.
# Do not edit: regenerate with scripts/features.mjs.

Feature: Each extraction reports what in the build it did not read

  @criterion-FR-KESSEL-026-c1
  Scenario: Field facts are recorded
    Given a build whose objects carry field values
    When kessel extracts the build
    Then the output database holds, for each field id, the declared type tag or that none is declared, and the count of each value shape read

  @criterion-FR-KESSEL-026-c2
  Scenario: Field changes come from two builds' facts
    Given a previous extraction whose recorded field facts differ from the build's
    When kessel extracts the build naming that previous extraction
    Then each new or gone field id, each changed declared type tag and each appeared or disappeared value shape is reported

  @criterion-FR-KESSEL-026-c3
  Scenario: An unread string table is reported
    Given a build holding an en-us string table kessel does not read
    When kessel extracts the build
    Then the table is reported with its path or hash

  @criterion-FR-KESSEL-026-c4
  Scenario: Left-out objects are counted
    Given a build holding objects the extract leaves out
    When kessel extracts the build
    Then each kind and prefix left out is reported with the number of objects left out

  @criterion-FR-KESSEL-026-c5
  Scenario: An area without its file is reported
    Given a build whose map data names an area with no area file, and holding an area file no map data names
    When kessel extracts the build
    Then both areas are reported

  @criterion-FR-KESSEL-026-c6
  Scenario: The report lives with the data
    Given any build
    When kessel extracts the build
    Then the output database holds the run's report of unknowns

  @criterion-FR-KESSEL-026-c7
  Scenario: A clean check says so
    Given a build in which a check finds nothing to report
    When kessel extracts the build
    Then the report states that the check found nothing

  @criterion-FR-KESSEL-026-c8
  Scenario: A failed check never fails the extract
    Given a build in which one check cannot run
    When kessel extracts the build
    Then the report states that the check could not run and why
    And the extraction completes and writes its data

  @criterion-FR-KESSEL-026-c9
  Scenario: A value that contradicts its declared type is reported
    Given a build in which a field's read values do not fit the type tag its client dictionary declares
    When kessel extracts the build
    Then the field is reported with the declared tag and the shapes read
