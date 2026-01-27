-- Repository-level permissions for RBAC
CREATE TABLE IF NOT EXISTS repository_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id VARCHAR(255) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    granted_by UUID REFERENCES users(id),
    CONSTRAINT unique_user_repo_per_permission UNIQUE (repository_id, user_id, permission_id)
);

CREATE INDEX idx_repo_permissions_repository ON repository_permissions(repository_id);
CREATE INDEX idx_repo_permissions_user ON repository_permissions(user_id);
CREATE INDEX idx_repo_permissions_permission ON repository_permissions(permission_id);

COMMENT ON TABLE repository_permissions IS 'Repository-level access permissions for users';
