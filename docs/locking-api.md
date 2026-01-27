# Document Locking API

This document describes the recommended Document Locking API and usage patterns.

Purpose
- Prevent conflicting concurrent edits and clarify ownership of edits.
- Primary strategy: optimistic locking with version tokens (ETag / If-Match).
- Secondary strategy: short-lived server-side leases for long-running editor sessions.

Design Summary
- Optimistic locking: client reads document with an `ETag` (version token) and includes `If-Match` on update. Reject updates when version mismatch (409 Conflict).
- Server-side leases: used when clients need to hold a lock for multi-step edits (e.g. large uploads). Leases have a short TTL (30–120s) and a `leaseId` to release early.

HTTP Endpoints

- POST /api/documents/{documentId}/locks
  - Description: Acquire a lease for a document.
  - Request body: { "owner": "user-id", "ttl": 30 }
  - Response 201: { "leaseId": "abc123", "expiresAt": "2026-01-27T12:34:56Z", "owner": "user-id" }

- GET /api/documents/{documentId}/locks
  - Description: Inspect current lock state.
  - Response 200: { "locked": true, "owner": "user-id", "leaseId": "abc123", "expiresAt": "..." }

- DELETE /api/documents/{documentId}/locks/{leaseId}
  - Description: Release a lease owned by the caller (or by admin/service account).
  - Response 204: No content

- PUT /api/documents/{documentId}
  - Description: Update document content using optimistic locking.
  - Headers: `If-Match: "<etag>"`
  - Request body: { "content": { ... } }
  - Response 200 / 409 Conflict when ETag mismatch

Examples

Acquire a lease (curl):

```bash
curl -X POST "https://example.com/api/documents/123/locks" \
  -H "Content-Type: application/json" \
  -d '{"owner":"user:alice","ttl":60}'
```

Update with optimistic locking (curl):

```bash
# client previously read ETag: "W/\"42\""
curl -X PUT "https://example.com/api/documents/123" \
  -H "If-Match: \"W/\"42\"\"" \
  -H "Content-Type: application/json" \
  -d '{"content":{"title":"Updated"}}'
```

OpenAPI (partial) example

```yaml
paths:
  /api/documents/{documentId}/locks:
    post:
      summary: Acquire a lease
      parameters:
        - in: path
          name: documentId
          required: true
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                owner:
                  type: string
                ttl:
                  type: integer
      responses:
        '201':
          description: Lease created
          content:
            application/json:
              schema:
                type: object
                properties:
                  leaseId:
                    type: string
                  expiresAt:
                    type: string
                    format: date-time

  /api/documents/{documentId}:
    put:
      summary: Update document using optimistic locking
      parameters:
        - in: path
          name: documentId
          required: true
      requestBody:
        required: true
        content:
          application/json: {}
      responses:
        '200': { description: OK }
        '409': { description: Conflict - ETag mismatch }
      security: []
```

Notes & Best Practices
- Prefer optimistic locking for routine edits — it's simple and scales better.
- Use server-side leases only when necessary; keep TTLs short and allow auto-renewal if the client proves activity.
- Always check ownership before releasing a lease; support admin override for moderation or recovery.
- Ensure audit logs record lease acquisition/release and ETag-based updates.

See also: `engram task [c2154023-007f-4ae4-97d3-71588010d268]` for task tracking and decisions.
