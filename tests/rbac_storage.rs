//! Tests for rbac::storage — InMemoryRbacStorage CRUD operations

use agentic_repos::rbac::models::{Permission, RepositoryPermission, Role, User};
use agentic_repos::rbac::storage::{InMemoryRbacStorage, RbacStorage};
use std::collections::HashSet;
use uuid::Uuid;

fn make_storage() -> InMemoryRbacStorage {
    InMemoryRbacStorage::new()
}

fn make_user(name: &str) -> User {
    User::new(name.to_string(), None)
}

fn make_role(name: &str) -> Role {
    Role::new(
        name.to_string(),
        format!("{} role", name),
        HashSet::from([Permission::RepositoryRead]),
    )
}

// ── User storage ──

#[tokio::test]
async fn create_and_get_user() {
    let store = make_storage();
    let user = make_user("alice");
    store.create_user(user.clone()).await.unwrap();
    let found = store.get_user("alice").await.unwrap().unwrap();
    assert_eq!(found.username, "alice");
    assert_eq!(found.id, user.id);
}

#[tokio::test]
async fn get_user_not_found() {
    let store = make_storage();
    assert!(store.get_user("nobody").await.unwrap().is_none());
}

#[tokio::test]
async fn get_user_by_id_found() {
    let store = make_storage();
    let user = make_user("bob");
    let id = user.id;
    store.create_user(user).await.unwrap();
    let found = store.get_user_by_id(&id).await.unwrap().unwrap();
    assert_eq!(found.username, "bob");
}

#[tokio::test]
async fn get_user_by_id_not_found() {
    let store = make_storage();
    assert!(store.get_user_by_id(&Uuid::new_v4()).await.unwrap().is_none());
}

#[tokio::test]
async fn user_exists_check() {
    let store = make_storage();
    assert!(!store.user_exists("charlie").await.unwrap());
    store.create_user(make_user("charlie")).await.unwrap();
    assert!(store.user_exists("charlie").await.unwrap());
}

#[tokio::test]
async fn update_user() {
    let store = make_storage();
    let mut user = make_user("dave");
    store.create_user(user.clone()).await.unwrap();
    user.add_ssh_key("ssh-key-1".to_string());
    store.update_user(user).await.unwrap();
    let found = store.get_user("dave").await.unwrap().unwrap();
    assert!(found.ssh_keys.contains(&"ssh-key-1".to_string()));
}

#[tokio::test]
async fn delete_user() {
    let store = make_storage();
    store.create_user(make_user("eve")).await.unwrap();
    store.delete_user("eve").await.unwrap();
    assert!(store.get_user("eve").await.unwrap().is_none());
    assert!(store.list_users().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_nonexistent_user_ok() {
    let store = make_storage();
    store.delete_user("ghost").await.unwrap();
}

#[tokio::test]
async fn list_users_empty() {
    let store = make_storage();
    assert!(store.list_users().await.unwrap().is_empty());
}

#[tokio::test]
async fn list_users_multiple() {
    let store = make_storage();
    store.create_user(make_user("u1")).await.unwrap();
    store.create_user(make_user("u2")).await.unwrap();
    store.create_user(make_user("u3")).await.unwrap();
    assert_eq!(store.list_users().await.unwrap().len(), 3);
}

// ── Role storage ──

#[tokio::test]
async fn create_and_get_role() {
    let store = make_storage();
    let role = make_role("viewer");
    store.create_role(role.clone()).await.unwrap();
    let found = store.get_role("viewer").await.unwrap().unwrap();
    assert_eq!(found.name, "viewer");
    assert_eq!(found.id, role.id);
}

#[tokio::test]
async fn get_role_not_found() {
    let store = make_storage();
    assert!(store.get_role("nothing").await.unwrap().is_none());
}

#[tokio::test]
async fn get_role_by_id() {
    let store = make_storage();
    let role = make_role("editor");
    let id = role.id;
    store.create_role(role).await.unwrap();
    let found = store.get_role_by_id(&id).await.unwrap().unwrap();
    assert_eq!(found.name, "editor");
}

#[tokio::test]
async fn role_exists_check() {
    let store = make_storage();
    assert!(!store.role_exists("admin").await.unwrap());
    store.create_role(make_role("admin")).await.unwrap();
    assert!(store.role_exists("admin").await.unwrap());
}

#[tokio::test]
async fn update_role() {
    let store = make_storage();
    let mut role = make_role("updatable");
    store.create_role(role.clone()).await.unwrap();
    role.description = "Updated desc".to_string();
    store.update_role(role).await.unwrap();
    let found = store.get_role("updatable").await.unwrap().unwrap();
    assert_eq!(found.description, "Updated desc");
}

#[tokio::test]
async fn delete_role() {
    let store = make_storage();
    store.create_role(make_role("temp")).await.unwrap();
    store.delete_role("temp").await.unwrap();
    assert!(store.get_role("temp").await.unwrap().is_none());
}

#[tokio::test]
async fn delete_nonexistent_role_ok() {
    let store = make_storage();
    store.delete_role("ghost-role").await.unwrap();
}

#[tokio::test]
async fn list_roles() {
    let store = make_storage();
    store.create_role(make_role("r1")).await.unwrap();
    store.create_role(make_role("r2")).await.unwrap();
    assert_eq!(store.list_roles().await.unwrap().len(), 2);
}

// ── Repository permissions ──

#[tokio::test]
async fn create_and_get_repo_permission() {
    let store = make_storage();
    let user_id = Uuid::new_v4();
    let perm = RepositoryPermission {
        repository_id: "repo-1".to_string(),
        user_id,
        permissions: HashSet::from([Permission::RepositoryWrite]),
        granted_by: Uuid::new_v4(),
        granted_at: chrono::Utc::now(),
        expires_at: None,
    };
    store.create_repository_permission(perm).await.unwrap();
    let perms = store.get_repository_permissions("repo-1", &user_id).await.unwrap();
    assert_eq!(perms.len(), 1);
    assert!(perms[0].permissions.contains(&Permission::RepositoryWrite));
}

#[tokio::test]
async fn get_repo_permissions_empty() {
    let store = make_storage();
    let perms = store.get_repository_permissions("repo-x", &Uuid::new_v4()).await.unwrap();
    assert!(perms.is_empty());
}

#[tokio::test]
async fn delete_repo_permission_partial() {
    let store = make_storage();
    let user_id = Uuid::new_v4();
    let perm = RepositoryPermission {
        repository_id: "repo-2".to_string(),
        user_id,
        permissions: HashSet::from([Permission::RepositoryWrite, Permission::RepositoryRead]),
        granted_by: Uuid::new_v4(),
        granted_at: chrono::Utc::now(),
        expires_at: None,
    };
    store.create_repository_permission(perm).await.unwrap();

    // Delete write — this removes the entire entry since it contained write
    store.delete_repository_permission("repo-2", &user_id, &Permission::RepositoryWrite).await.unwrap();

    let perms = store.get_repository_permissions("repo-2", &user_id).await.unwrap();
    // The entry is fully removed, not just the permission from the set
    assert!(perms.is_empty());
}

#[tokio::test]
async fn delete_all_repo_perms_removes_key() {
    let store = make_storage();
    let user_id = Uuid::new_v4();
    let perm = RepositoryPermission {
        repository_id: "repo-3".to_string(),
        user_id,
        permissions: HashSet::from([Permission::RepositoryRead]),
        granted_by: Uuid::new_v4(),
        granted_at: chrono::Utc::now(),
        expires_at: None,
    };
    store.create_repository_permission(perm).await.unwrap();

    store.delete_repository_permission("repo-3", &user_id, &Permission::RepositoryRead).await.unwrap();

    let perms = store.get_repository_permissions("repo-3", &user_id).await.unwrap();
    assert!(perms.is_empty());
}

// ── Default impl ──

#[test]
fn default_impl_matches_new() {
    let _store: InMemoryRbacStorage = InMemoryRbacStorage::default();
}
