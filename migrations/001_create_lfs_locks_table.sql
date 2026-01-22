CREATE TABLE IF NOT EXISTS lfs_locks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant VARCHAR(255) NOT NULL,
    repo VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    ref_name VARCHAR(255),
    owner_name VARCHAR(255) NOT NULL,
    owner_id UUID,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    CONSTRAINT unique_path_per_repo UNIQUE (tenant, repo, path)
);

CREATE INDEX idx_lfs_locks_tenant_repo ON lfs_locks(tenant, repo);
CREATE INDEX idx_lfs_locks_owner ON lfs_locks(owner_name);
CREATE INDEX idx_lfs_locks_path ON lfs_locks(path);
CREATE INDEX idx_lfs_locks_ref ON lfs_locks(ref_name) WHERE ref_name IS NOT NULL;
CREATE INDEX idx_lfs_locks_expires ON lfs_locks(expires_at) WHERE expires_at IS NOT NULL;

COMMENT ON TABLE lfs_locks IS 'Git LFS file locks per https://github.com/git-lfs/git-lfs/blob/main/docs/api/locking.md';
COMMENT ON COLUMN lfs_locks.expires_at IS 'Server extension for automatic cleanup (not in official Git LFS spec)';
