# ADP Schema & Access REST API

## Overview

The ADP (Agentic Development Platform) REST API provides endpoints for managing ref-namespace schemas and access control configurations stored in git refs. All data lives in the `refs/adp/*` namespace of each repository.

## Base URLs

- **Repo-level**: `/repos/{owner}/{repo}/adp`
- **System-level**: `/system/adp`

## Authentication

Most read endpoints are accessible to all authenticated users. Endpoints that modify access configuration require the `X-ADP-User` header:

```
X-ADP-User: username
```

The caller's identity is resolved against the repository's access config (`refs/adp/access/config`) to determine owner/admin privileges.

### Authorization Matrix

| Endpoint | Required Role |
|----------|--------------|
| `GET /schemas` | Any authenticated user |
| `POST /schemas` | Any authenticated user |
| `GET/PUT/DELETE /schemas/{id}` | Any authenticated user |
| `GET /schemas/{id}/versions` | Any authenticated user |
| `GET /schemas/resolve` | Any authenticated user |
| `GET /refs` | Any authenticated user |
| `GET /refs/{ref_path}` | Any authenticated user |
| `GET /blobs/{sha}` | Any authenticated user |
| `GET /access` | Owner or Admin |
| `PUT /access` | Owner only |

---

## Schema Endpoints

### List Schemas

```
GET /repos/{owner}/{repo}/adp/schemas
```

Returns all schemas defined in the repository.

**Response** `200 OK`:
```json
[
  {
    "id": "engram-task",
    "namespace_pattern": "refs/engram/task/*",
    "title": "Engram Task",
    "description": "An engram task entity",
    "version": "1.0.0",
    "author": "engram-cli/1.0.0",
    "created_at": "2026-04-10T12:00:00Z",
    "updated_at": "2026-04-10T12:00:00Z",
    "schema": { "type": "object" },
    "ui": {
      "view": "entity-list",
      "list_fields": ["title", "status"],
      "detail_fields": ["title", "description"],
      "fields": {
        "status": {
          "widget": "badge",
          "color_map": { "done": "green" }
        }
      }
    }
  }
]
```

### Create Schema

```
POST /repos/{owner}/{repo}/adp/schemas
```

Creates a new schema. Returns `409 Conflict` if a schema with the same ID already exists.

**Request Body**:
```json
{
  "id": "my-schema",
  "namespace_pattern": "refs/my-schema/*",
  "title": "My Schema",
  "description": "Description",
  "version": "1.0.0",
  "schema": { "type": "object", "properties": {} }
}
```

**Response** `201 Created`: Returns the created schema.

**Validation Rules**:
- `id` must not be empty, contain `/`, or spaces
- `namespace_pattern` must not be empty
- `version` must not be empty

### Get Schema

```
GET /repos/{owner}/{repo}/adp/schemas/{id}
```

**Response** `200 OK`: Returns the schema.
**Response** `404 Not Found`: Schema does not exist.

### Update Schema

```
PUT /repos/{owner}/{repo}/adp/schemas/{id}
```

Updates an existing schema. The `id` in the URL must match the `id` in the body. `updated_at` is automatically set to the current time.

**Response** `200 OK`: Returns the updated schema.

### Delete Schema

```
DELETE /repos/{owner}/{repo}/adp/schemas/{id}
```

**Response** `204 No Content`: Schema deleted.
**Response** `404 Not Found`: Schema does not exist.

### List Schema Versions

```
GET /repos/{owner}/{repo}/adp/schemas/{id}/versions
```

Returns the immutable version history (sidecar refs) for a schema. Each write to a schema creates a new version sidecar at `refs/adp/schema/v{N}/{id}`.

**Response** `200 OK`:
```json
[
  { "version": 1, "ref_name": "refs/adp/schema/v1/my-schema" },
  { "version": 2, "ref_name": "refs/adp/schema/v2/my-schema" }
]
```

### Resolve Schema

```
GET /repos/{owner}/{repo}/adp/schemas/resolve?ref=refs/engram/task/abc123
```

Finds the schema whose `namespace_pattern` matches the given ref name. If multiple schemas match, the most specific one (fewer wildcards) is returned.

**Response** `200 OK`: Returns the matching schema.
**Response** `404 Not Found`: No schema matches.

---

## Ref Browsing Endpoints

### List ADP Refs

```
GET /repos/{owner}/{repo}/adp/refs[?prefix=refs/adp/schema/]
```

Lists all ADP refs (skipping version sidecars) under the given prefix.

**Query Parameters**:
- `prefix` (optional): Ref prefix to filter by. Defaults to `refs/adp/`.

**Response** `200 OK`:
```json
[
  { "name": "refs/adp/schema/engram-task", "schema_id": "engram-task" },
  { "name": "refs/adp/schema/engram-session", "schema_id": "engram-session" }
]
```

### Read Ref Blob

```
GET /repos/{owner}/{repo}/adp/refs/{ref_path}
```

Reads the JSON blob pointed to by the given ref. The ref may point to a commit (containing a `data` tree entry) or directly to a blob.

**Response** `200 OK`: Returns the JSON content.
**Response** `404 Not Found`: Ref does not exist.

### Read Blob by SHA

```
GET /repos/{owner}/{repo}/adp/blobs/{sha}
```

Reads a blob by its SHA hash. Returns parsed JSON if valid, or raw content otherwise.

**Response** `200 OK`:
```json
{
  "sha": "abc123...",
  "content": { "key": "value" }
}
```

---

## Access Config Endpoints

### Get Access Config

```
GET /repos/{owner}/{repo}/adp/access
```

**Requires**: Owner or Admin role.

**Response** `200 OK`:
```json
{
  "version": "1",
  "owners": ["alice"],
  "admins": ["bob"],
  "rules": [
    {
      "user": "*",
      "paths": ["refs/heads/*"],
      "permissions": ["pull", "push"]
    }
  ]
}
```

### Update Access Config

```
PUT /repos/{owner}/{repo}/adp/access
```

**Requires**: Owner role (not just admin).

**Request Body**: Same format as the response above.

**Validation Rules**:
- `version` must not be empty
- Owner/admin usernames must not be empty
- Each rule must have a non-empty `user` and at least one `path`

---

## System-Level Endpoints

All schema and access config endpoints are mirrored under `/system/adp` for managing the system repository (`_adp/system`):

| Repo Endpoint | System Endpoint |
|---|---|
| `GET /repos/{owner}/{repo}/adp/schemas` | `GET /system/adp/schemas` |
| `POST /repos/{owner}/{repo}/adp/schemas` | `POST /system/adp/schemas` |
| `GET /repos/{owner}/{repo}/adp/schemas/{id}` | `GET /system/adp/schemas/{id}` |
| `PUT /repos/{owner}/{repo}/adp/schemas/{id}` | `PUT /system/adp/schemas/{id}` |
| `DELETE /repos/{owner}/{repo}/adp/schemas/{id}` | `DELETE /system/adp/schemas/{id}` |
| `GET /repos/{owner}/{repo}/adp/schemas/{id}/versions` | `GET /system/adp/schemas/{id}/versions` |
| `GET /repos/{owner}/{repo}/adp/schemas/resolve` | `GET /system/adp/schemas/resolve` |
| `GET /repos/{owner}/{repo}/adp/access` | `GET /system/adp/access` |
| `PUT /repos/{owner}/{repo}/adp/access` | `PUT /system/adp/access` |

System-level access config endpoints require system-level owner/admin status resolved against the system repo's access config.

---

## Error Responses

All errors follow a consistent format:

```json
{
  "error": "error_type",
  "message": "Human-readable description",
  "code": 400
}
```

| Code | Error Type | Description |
|------|-----------|-------------|
| 400 | `bad_request` | Invalid input (validation failure) |
| 403 | `forbidden` | Caller lacks required role |
| 404 | `not_found` | Resource does not exist |
| 409 | `conflict` | Resource already exists (schema creation) |
| 500 | `internal_server_error` | Server-side error |

---

## Data Types

### RefSchema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Unique slug identifier (no `/` or spaces) |
| `namespace_pattern` | string | yes | Glob pattern for matching refs |
| `title` | string | yes | Human-readable name |
| `description` | string | no | Description |
| `version` | string | yes | Semver version string |
| `author` | string | no | Author identifier |
| `created_at` | datetime | yes | Creation timestamp |
| `updated_at` | datetime | yes | Last update timestamp |
| `schema` | object | yes | JSON Schema (draft 2020-12) |
| `ui` | UiHints | no | Web UI rendering hints |

### AccessConfig

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | string | yes | Schema version |
| `owners` | string[] | no | Full control users |
| `admins` | string[] | no | Read/write + private ref access |
| `rules` | PathRule[] | no | Ordered permission rules |

### PathRule

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `user` | string | yes | Username or `*` for all |
| `paths` | string[] | yes | Glob patterns for ref names |
| `permissions` | Permission[] | yes | Granted permissions |

### Permission

| Value | Description |
|-------|-------------|
| `pull` | Read/clone |
| `push` | Write/push |
| `admin` | Full admin (implies pull+push) |
