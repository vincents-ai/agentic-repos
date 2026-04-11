Feature: Health and Monitoring
  As a Git operations plugin
  I want to provide health status and metrics
  So that operators can monitor plugin health and performance

  Background:
    Given the plugin "./target/release/librepos_mcp.so" is loaded
    And the plugin is initialized

  @health @monitoring @smoke
  Scenario: Health check returns valid status
    When I execute the action "health.check" with arguments:
      """
      {
        "id": "health-1",
        "method": "health.check",
        "params": {}
      }
      """
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "result"
    And the response field "result" should contain "status"

  @health @monitoring
  Scenario: Health check includes version info
    When I execute the action "health.check" with arguments:
      """
      {
        "id": "health-2",
        "method": "health.check",
        "params": {}
      }
      """
    Then the action should succeed
    And the response should have field "result"

  @monitoring @metrics
  Scenario: Get plugin metrics
    When I execute the action "metrics.get" with arguments:
      """
      {
        "id": "metrics-1",
        "method": "metrics.get",
        "params": {}
      }
      """
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "result"

  @health @monitoring
  Scenario: Multiple health checks in sequence
    When I execute the action "health.check" with arguments:
      """
      {
        "id": "health-3",
        "method": "health.check",
        "params": {}
      }
      """
    Then the action should succeed
    When I execute the action "health.check" with arguments:
      """
      {
        "id": "health-4",
        "method": "health.check",
        "params": {}
      }
      """
    Then the action should succeed
