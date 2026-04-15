Feature: Repos-MCP Plugin Validation
  As a Skylet plugin
  I want to meet all Skylet v2 ABI requirements
  So that I can be safely loaded and executed

  @validation @abi
  Scenario: Plugin has required V2 ABI exports
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    When I request the plugin info
    Then the action should succeed
    And the response should have field "name"
    And the response should have field "version"
    And the response should have field "abi_version"

  @validation @abi
  Scenario: Plugin uses correct ABI version
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    When I request the plugin info
    Then the response field "abi_version" should equal "2.0"

  @validation @abi
  Scenario: Plugin exposes capabilities
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I request the plugin info
    Then the response should have field "capabilities"
    And the response should be valid JSON

  @validation @error
  Scenario: Plugin handles unknown actions gracefully
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I execute the action "definitely_not_a_real_action_xyz123"
    Then the action should fail
    And the error should contain "unknown"

  @validation @error
  Scenario: Plugin handles invalid JSON arguments
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I execute the action "repo.create" with arguments:
      """
      {invalid json here}
      """
    Then the action should fail
