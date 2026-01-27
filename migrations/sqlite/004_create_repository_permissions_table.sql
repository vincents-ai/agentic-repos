-- Repository-level permissions for RBAC (SQLite)
CREATE TABLE IF NOT EXISTS repository_permissions (
    id TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    permission_id TEXT NOT NULL,
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    granted_by TEXT,
    UNIQUE (repository_id, user_id, permission_id)
);

CREATE INDEX idx_repo_permissions_repository ON repository_permissions(repository_id);
CREATE INDEX idx_repo_permissions_user ON repository_permissions(user_id);
CREATE INDEX idx_repo_permissions_permission ON repository_permissions(permission_id);
