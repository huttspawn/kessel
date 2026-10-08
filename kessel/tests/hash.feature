# Generated from the legion requirement. Do not hand-edit: change the
# requirement and regenerate.
Feature: Object identity hashes

  @smoke
  Scenario: A stable id is the SHA-256 of the FQN truncated to 16 hex characters
    Given an object FQN
    When its stable id is computed
    Then the stable id is the first 16 hex characters of the SHA-256 digest of the FQN
