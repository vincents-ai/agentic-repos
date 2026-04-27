//! Authentication — open-core SSH key auth and session management.
//!
//! Enterprise auth providers (LDAP, SAML, OAuth2) have moved to adp.
//! This module keeps only:
//! - `session` — provider-agnostic `SessionUser`, `Session`, `SessionManager`
//! - `middleware` — `OAuthMiddleware`, `get_current_session`, `get_current_user`

pub mod middleware;
pub mod session;

pub use middleware::OAuthMiddleware;
pub use session::{
    InMemorySessionStore, Session, SessionManager, SessionStore, SessionUser,
};
