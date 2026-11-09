# Generated from the legion requirement. Do not hand-edit: change the requirement and regenerate.
Feature: icon ids

  @smoke @criterion-smoke
  Scenario: an icon id is the first 16 hex characters of the name's SHA-256
    Given the icon name "abc"
    When its icon id is computed
    Then the icon id is "ba7816bf8f01cfea"
