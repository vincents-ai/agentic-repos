use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;

/// Attempt to connect to the test database.
///
/// Returns `None` when PostgreSQL is not reachable (e.g. in a sandboxed CI
/// environment), allowing callers to skip the test gracefully rather than
/// panicking.
pub async fn setup_test_db() -> Option<Arc<PgPool>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost/repos_mcp_test".to_string());

    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect(&database_url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Skipping test: PostgreSQL not available ({e})");
            return None;
        }
    };

    // Ensure pgcrypto is available and create table and indexes required by tests.
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto;")
        .execute(&pool)
        .await
        .expect("Failed to create pgcrypto extension");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS lfs_locks (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant TEXT NOT NULL,
            repo TEXT NOT NULL,
            path TEXT NOT NULL,
            ref_name TEXT,
            owner_name TEXT NOT NULL,
            owner_id TEXT,
            locked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ,
            UNIQUE(tenant, repo, path)
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create test table");

    // Ensure the id column has a default generator (in case the table existed
    // from a previous run without the default).
    sqlx::query("ALTER TABLE lfs_locks ALTER COLUMN id SET DEFAULT gen_random_uuid();")
        .execute(&pool)
        .await
        .expect("Failed to set default on id column");

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_lfs_locks_tenant_repo ON lfs_locks(tenant, repo);")
        .execute(&pool)
        .await
        .expect("Failed to create index idx_lfs_locks_tenant_repo");

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_lfs_locks_path ON lfs_locks(tenant, repo, path);")
        .execute(&pool)
        .await
        .expect("Failed to create index idx_lfs_locks_path");

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lfs_locks_owner ON lfs_locks(tenant, repo, owner_name);",
    )
    .execute(&pool)
    .await
    .expect("Failed to create index idx_lfs_locks_owner");

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_lfs_locks_ref ON lfs_locks(tenant, repo, ref_name);",
    )
    .execute(&pool)
    .await
    .expect("Failed to create index idx_lfs_locks_ref");

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_lfs_locks_expires ON lfs_locks(expires_at);")
        .execute(&pool)
        .await
        .expect("Failed to create index idx_lfs_locks_expires");

    Some(Arc::new(pool))
}

pub async fn cleanup_test_db(pool: &PgPool) {
    sqlx::query("DELETE FROM lfs_locks")
        .execute(pool)
        .await
        .expect("Failed to cleanup test data");
}
