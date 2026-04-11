Feature: Repository Operations
  As a Git operations plugin
  I want to manage Git repositories
  So that users can create, list, and delete repositories

  Background:
    Given the plugin "./target/release/librepos_mcp.so" is loaded
    And the plugin is initialized

  @repo @api @smoke
  Scenario: Create a new repository
    When I execute the action "repo.create" with arguments:
      """
      {
        "id": "test-123",
        "method": "repo.create",
        "params": {
          "name": "test-repo",
          "tenant_id": "tenant-1"
        }
      }
      """
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "id"

  @repo @api
  Scenario: List repositories
    When I execute the action "repo.list" with arguments:
      """
      {
        "id": "test-456",
        "method": "repo.list",
        "params": {
          "tenant_id": "tenant-1"
        }
      }
      """
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "result"

  @repo @api
  Scenario: Get repository info
    When I execute the action "repo.info" with arguments:
      """
      {
        "id": "test-789",
        "method": "repo.info",
        "params": {
          "name": "test-repo",
          "tenant_id": "tenant-1"
        }
      }
      """
    Then the action should succeed
    And the response should be valid JSON

  @repo @api @error
  Scenario: Handle missing repository name
    When I execute the action "repo.create" with arguments:
      """
      {
        "id": "test-error-1",
        "method": "repo.create",
        "params": {
          "tenant_id": "tenant-1"
        }
      }
      """
    Then the action should fail

  @repo @api @error
  Scenario: Handle missing tenant ID
    When I execute the action "repo.create" with arguments:
      """
      {
        "id": "test-error-2",
        "method": "repo.create",
        "params": {
          "name": "test-repo"
        }
      }
      """
    Then the action should fail
