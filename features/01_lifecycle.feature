Feature: Repos-MCP Plugin Lifecycle
  As a Skylet plugin
  I want to ensure proper lifecycle management
  So that I can be safely loaded and used in production

  @smoke @lifecycle
  Scenario: Load and initialize agentic-repos plugin
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I request the plugin info
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "name"
    And the response field "name" should equal "git-operations"

  @smoke @lifecycle
  Scenario: Plugin metadata validation
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I request the plugin info
    Then the response should have field "version"
    And the response should have field "author"
    And the response should have field "description"
    And the response should have field "abi_version"

  @smoke @lifecycle
  Scenario: Plugin health check
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I execute the action "health.check"
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "status"

  @lifecycle @wip
  Scenario: Plugin graceful shutdown
    Given the plugin "./target/release/libagentic_repos.so" is loaded
    And the plugin is initialized
    When I shutdown the plugin
    Then all tests should pass
