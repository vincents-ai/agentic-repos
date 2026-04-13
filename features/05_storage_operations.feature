Feature: Storage Operations
  As a Git operations plugin
  I want to manage storage backends
  So that repositories can be stored in various backends

  Background:
    Given the plugin "./target/release/librepos_mcp.so" is loaded
    And the plugin is initialized

  @storage @api
  Scenario: List available storage backends
    When I execute the action "storage.list_backends" with arguments:
      """
      {
        "id": "storage-1",
        "method": "storage.list_backends",
        "params": {}
      }
      """
    Then the action should succeed
    And the response should be valid JSON
    And the response should have field "result"

  @storage @api @smoke
  Scenario: List storage buckets
    When I execute the action "storage.list_buckets" with arguments:
      """
      {
        "id": "storage-2",
        "method": "storage.list_buckets",
        "params": {
          "backend_name": "local"
        }
      }
      """
    Then the action should succeed
    And the response should be valid JSON

  @storage @api @error
  Scenario: Handle invalid storage backend
    When I execute the action "storage.list_buckets" with arguments:
      """
      {
        "id": "storage-error-1",
        "method": "storage.list_buckets",
        "params": {
          "backend_name": "nonexistent-backend"
        }
      }
      """
    Then the action should succeed
    And the response field "result" should contain "error"

  @storage @api @wip
  Scenario: Create and delete storage bucket
    When I execute the action "storage.create_bucket" with arguments:
      """
      {
        "id": "storage-3",
        "method": "storage.create_bucket",
        "params": {
          "backend_name": "local",
          "bucket_name": "test-bucket-123"
        }
      }
      """
    Then the action should succeed
    And I store the response field "id" as "bucket_op_id"
