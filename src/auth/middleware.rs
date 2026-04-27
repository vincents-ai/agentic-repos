use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorUnauthorized,
    Error, HttpMessage,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::Arc;
use uuid::Uuid;

use super::session::{InMemorySessionStore, Session, SessionManager, SessionUser};

pub struct OAuthMiddleware {
    session_manager: Arc<SessionManager<InMemorySessionStore>>,
    public_paths: Vec<String>,
}

impl OAuthMiddleware {
    pub fn new(session_manager: Arc<SessionManager<InMemorySessionStore>>) -> Self {
        Self {
            session_manager,
            public_paths: vec![
                "/health".to_string(),
                "/info".to_string(),
                "/metrics".to_string(),
                "/api/v1/auth/login".to_string(),
                "/api/v1/auth/callback".to_string(),
                "/api/v1/auth/refresh".to_string(),
            ],
        }
    }

    pub fn with_public_paths(mut self, paths: Vec<String>) -> Self {
        self.public_paths = paths;
        self
    }

    fn is_public_path(&self, path: &str) -> bool {
        self.public_paths.iter().any(|p| {
            // Exact match or segment-boundary prefix match
            // prevents /api/auth matching /api/auth-backdoor
            if path == p {
                return true;
            }
            path.starts_with(p) && path.as_bytes().get(p.len()) == Some(&b'/')
        })
    }

    fn extract_session_id(&self, req: &ServiceRequest) -> Option<Uuid> {
        let auth_header = req.headers().get("Authorization")?;
        let auth_str = auth_header.to_str().ok()?;

        if auth_str.starts_with("Bearer ") {
            let token = auth_str.trim_start_matches("Bearer ");
            Uuid::parse_str(token).ok()
        } else {
            None
        }
    }
}

pub struct OAuthMiddlewareInner<S> {
    service: Rc<S>,
    middleware: OAuthMiddleware,
}

impl<S, B> Transform<S, ServiceRequest> for OAuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = OAuthMiddlewareInner<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(OAuthMiddlewareInner {
            service: Rc::new(service),
            middleware: self.clone(),
        }))
    }
}

impl<S, B> Service<ServiceRequest> for OAuthMiddlewareInner<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);
        let middleware = self.middleware.clone();

        Box::pin(async move {
            let path = req.path();

            if middleware.is_public_path(path) {
                return service.call(req).await;
            }

            let session_id = match middleware.extract_session_id(&req) {
                Some(id) => id,
                None => {
                    return Err(ErrorUnauthorized("Missing or invalid Authorization header"));
                }
            };

            let session = match middleware.session_manager.get_session(&session_id).await {
                Some(s) => s,
                None => {
                    return Err(ErrorUnauthorized("Invalid or expired session"));
                }
            };

            req.extensions_mut().insert(session);

            service.call(req).await
        })
    }
}

impl Clone for OAuthMiddleware {
    fn clone(&self) -> Self {
        Self {
            session_manager: Arc::clone(&self.session_manager),
            public_paths: self.public_paths.clone(),
        }
    }
}

pub fn get_current_session(req: &actix_web::HttpRequest) -> Option<Session> {
    req.extensions().get::<Session>().cloned()
}

pub fn get_current_user(req: &actix_web::HttpRequest) -> Option<SessionUser> {
    get_current_session(req).map(|s| s.user)
}

pub fn require_auth(req: &actix_web::HttpRequest) -> Result<Session, Error> {
    get_current_session(req).ok_or_else(|| ErrorUnauthorized("Authentication required"))
}
