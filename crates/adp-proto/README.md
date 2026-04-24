# adp-proto

Shared protocol types for ADP service integration — SSE events, service registration, RBAC, and metrics constants.

## Types

| Type | Purpose |
|------|---------|
| `SseEvent` | 7 SSE event variants (heartbeat, registration, dispatch, etc.) |
| `ServiceType` | Service classification (AgenticLoop, CiRunner, CiRouter, etc.) |
| `KeyPermissions` | SSH key permissions with namespace restrictions |
| `PathRestrictions` | Glob-based path allow/deny rules |
| `RegistrationInfo` | Service registration payload with TTL |
| `ServiceMetadata` | Service capabilities and labels |

## Metrics Constants

16 Prometheus metric names + label helpers for KEDA auto-scaling, used by both adp server and integration crates.

## Tests

13 unit tests.
