use crate::rbac::models::{Permission, User};
use crate::rbac::service::RbacService;
use actix_web::{
    body::{BoxBody, EitherBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web, Error, HttpMessage, HttpResponse,
};
use futures_util::future::{ready, FutureExt, LocalBoxFuture, Ready};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user: User,
}

#[derive(Clone)]
pub struct RbacMiddleware {
    rbac_service: Arc<RbacService>,
}

impl RbacMiddleware {
    pub fn new(rbac_service: Arc<RbacService>) -> Self {
        Self { rbac_service }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RbacMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RbacMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RbacMiddlewareService {
            service: Rc::new(service),
            rbac_service: self.rbac_service.clone(),
        }))
    }
}

pub struct RbacMiddlewareService<S> {
    service: Rc<S>,
    rbac_service: Arc<RbacService>,
}

impl<S, B> Service<ServiceRequest> for RbacMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let rbac_service = self.rbac_service.clone();

        Box::pin(async move {
            if let Some(auth_header) = req.headers().get("Authorization") {
                if let Ok(auth_str) = auth_header.to_str() {
                    if auth_str.starts_with("Bearer ") {
                        let token = &auth_str[7..];

                        if let Ok(Some(user)) =
                            rbac_service.authenticate_user_by_ssh_key(token).await
                        {
                            req.extensions_mut().insert(AuthenticatedUser { user });
                            let resp = service.call(req).await?;
                            return Ok(resp.map_into_left_body());
                        }
                    } else if auth_str.starts_with("SSH-Key ") {
                        let ssh_key = &auth_str[8..];

                        if let Ok(Some(user)) =
                            rbac_service.authenticate_user_by_ssh_key(ssh_key).await
                        {
                            req.extensions_mut().insert(AuthenticatedUser { user });
                            let resp = service.call(req).await?;
                            return Ok(resp.map_into_left_body());
                        }
                    }
                }
            }

            if req.path().starts_with("/ui/") {
                return Ok(req
                    .into_response(
                        HttpResponse::Found()
                            .insert_header(("Location", "/ui/login"))
                            .finish(),
                    )
                    .map_into_right_body());
            }

            Ok(req
                .into_response(HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": "Authentication required",
                    "message": "Please provide valid authentication credentials"
                })))
                .map_into_right_body())
        })
    }
}

pub fn require_permission(permission: Permission) -> RequirePermission {
    RequirePermission { permission }
}

#[derive(Clone)]
pub struct RequirePermission {
    permission: Permission,
}

impl<S, B> Transform<S, ServiceRequest> for RequirePermission
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RequirePermissionService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequirePermissionService {
            service: Rc::new(service),
            permission: self.permission.clone(),
        }))
    }
}

pub struct RequirePermissionService<S> {
    service: Rc<S>,
    permission: Permission,
}

impl<S, B> Service<ServiceRequest> for RequirePermissionService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let permission = self.permission.clone();

        async move {
            let username = {
                let extensions = req.extensions();
                extensions
                    .get::<AuthenticatedUser>()
                    .map(|auth_user| auth_user.user.username.clone())
            };

            if let Some(username) = username {
                let rbac_service = req
                    .app_data::<web::Data<Arc<RbacService>>>()
                    .expect("RbacService not found in app data");

                let has_permission = rbac_service
                    .user_has_permission(&username, &permission)
                    .await
                    .unwrap_or(false);

                if has_permission {
                    let resp = service.call(req).await?;
                    return Ok(resp.map_into_left_body());
                }
            }

            Ok(req
                .into_response(HttpResponse::Forbidden().json(serde_json::json!({
                    "error": "Permission denied",
                    "message": format!("Required permission: {:?}", permission)
                })))
                .map_into_right_body())
        }
        .boxed_local()
    }
}

pub fn require_repository_permission(
    repository_param: &'static str,
    permission: Permission,
) -> RequireRepositoryPermission {
    RequireRepositoryPermission {
        repository_param,
        permission,
    }
}

#[derive(Clone)]
pub struct RequireRepositoryPermission {
    repository_param: &'static str,
    permission: Permission,
}

impl<S, B> Transform<S, ServiceRequest> for RequireRepositoryPermission
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<BoxBody, B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RequireRepositoryPermissionService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequireRepositoryPermissionService {
            service: Rc::new(service),
            repository_param: self.repository_param,
            permission: self.permission.clone(),
        }))
    }
}

pub struct RequireRepositoryPermissionService<S> {
    service: Rc<S>,
    repository_param: &'static str,
    permission: Permission,
}

impl<S, B> Service<ServiceRequest> for RequireRepositoryPermissionService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<BoxBody, B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let permission = self.permission.clone();
        let repository_param = self.repository_param;

        Box::pin(async move {
            let repository_id = req
                .match_info()
                .get(repository_param)
                .unwrap_or("unknown")
                .to_string();

            let username = {
                let extensions = req.extensions();
                extensions
                    .get::<AuthenticatedUser>()
                    .map(|auth_user| auth_user.user.username.clone())
            };

            if let Some(username) = username {
                let rbac_service = req
                    .app_data::<web::Data<Arc<RbacService>>>()
                    .expect("RbacService not found in app data");

                let has_permission = rbac_service
                    .user_has_repository_permission(&username, &repository_id, &permission)
                    .await
                    .unwrap_or(false);

                if has_permission {
                    let resp = service.call(req).await?;
                    return Ok(resp.map_into_right_body());
                }
            }

            Ok(req
                .into_response(HttpResponse::Forbidden().json(serde_json::json!({
                    "error": "Repository permission denied",
                    "message": format!("Required permission: {:?}", permission)
                })))
                .map_into_left_body())
        })
    }
}
