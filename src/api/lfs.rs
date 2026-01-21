use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::lfs::{
    Action, Actions, BatchRequest, BatchResponse, LfsStorage, ObjectError, Operation,
    ResponseObject, StorageKey, Transfer,
};

pub struct LfsHandler<S> {
    storage: Arc<S>,
    config: AppConfig,
}

impl<S> LfsHandler<S>
where
    S: LfsStorage + 'static,
{
    pub fn new(storage: Arc<S>, config: AppConfig) -> Self {
        Self { storage, config }
    }

    pub async fn handle_batch(
        &self,
        tenant: String,
        repo: String,
        batch_req: BatchRequest,
    ) -> Result<BatchResponse, ObjectError> {
        if !self.config.lfs.enabled {
            return Err(ObjectError {
                code: 503,
                message: "Git LFS is not enabled on this server".to_string(),
            });
        }

        let mut response_objects = Vec::new();

        for obj in batch_req.objects {
            if obj.size > self.config.lfs.max_object_size {
                response_objects.push(ResponseObject {
                    oid: obj.oid.clone(),
                    size: obj.size,
                    authenticated: Some(true),
                    actions: None,
                    error: Some(ObjectError::too_large(
                        &obj.oid,
                        self.config.lfs.max_object_size,
                    )),
                });
                continue;
            }

            let key = StorageKey::new(&tenant, &repo, &obj.oid);

            match batch_req.operation {
                Operation::Download => match self.storage.object_exists(&key).await {
                    Ok(exists) => {
                        if exists {
                            let download_url =
                                self.storage.get_download_url(&key).await.map_err(|e| {
                                    ObjectError::storage_error(&obj.oid, &e.to_string())
                                })?;

                            response_objects.push(ResponseObject {
                                oid: obj.oid,
                                size: obj.size,
                                authenticated: Some(true),
                                actions: Some(Actions {
                                    download: Some(Action {
                                        href: download_url,
                                        header: None,
                                        expires_in: Some(3600),
                                        expires_at: None,
                                    }),
                                    upload: None,
                                    verify: None,
                                }),
                                error: None,
                            });
                        } else {
                            response_objects.push(ResponseObject {
                                oid: obj.oid.clone(),
                                size: obj.size,
                                authenticated: Some(true),
                                actions: None,
                                error: Some(ObjectError::not_found(&obj.oid)),
                            });
                        }
                    }
                    Err(e) => {
                        response_objects.push(ResponseObject {
                            oid: obj.oid.clone(),
                            size: obj.size,
                            authenticated: Some(true),
                            actions: None,
                            error: Some(ObjectError::storage_error(&obj.oid, &e.to_string())),
                        });
                    }
                },
                Operation::Upload => {
                    let upload_url = self
                        .storage
                        .get_upload_url(&key, std::time::Duration::from_secs(3600))
                        .await
                        .map_err(|e| ObjectError::storage_error(&obj.oid, &e.to_string()))?;

                    response_objects.push(ResponseObject {
                        oid: obj.oid,
                        size: obj.size,
                        authenticated: Some(true),
                        actions: Some(Actions {
                            download: None,
                            upload: Some(Action {
                                href: upload_url,
                                header: None,
                                expires_in: Some(3600),
                                expires_at: None,
                            }),
                            verify: None,
                        }),
                        error: None,
                    });
                }
            }
        }

        Ok(BatchResponse {
            transfer: Transfer::Basic,
            objects: response_objects,
            hash_algo: "sha256".to_string(),
        })
    }
}

pub async fn batch_endpoint<S>(
    _req: HttpRequest,
    path: web::Path<(String, String)>,
    batch_req: web::Json<BatchRequest>,
    handler: web::Data<LfsHandler<S>>,
) -> impl Responder
where
    S: LfsStorage + 'static,
{
    let (tenant, repo) = path.into_inner();
    match handler
        .handle_batch(tenant, repo, batch_req.into_inner())
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(error) => HttpResponse::build(
            actix_web::http::StatusCode::from_u16(error.code)
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
        )
        .json(json!({
            "message": error.message
        })),
    }
}

pub async fn upload_object<S>(
    _req: HttpRequest,
    path: web::Path<(String, String, String)>,
    body: web::Bytes,
    handler: web::Data<LfsHandler<S>>,
) -> impl Responder
where
    S: LfsStorage + 'static,
{
    let (tenant, repo, oid) = path.into_inner();

    if !handler.config.lfs.enabled {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if body.len() as u64 > handler.config.lfs.max_object_size {
        return HttpResponse::PayloadTooLarge().json(json!({
            "message": format!("Object exceeds maximum size of {} bytes", handler.config.lfs.max_object_size)
        }));
    }

    if !crate::lfs::verify::verify_sha256(&body, &oid) {
        return HttpResponse::UnprocessableEntity().json(json!({
            "message": "Object hash does not match OID"
        }));
    }

    let key = StorageKey::new(tenant, repo, oid);

    match handler.storage.put_object(&key, body.to_vec()).await {
        Ok(()) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": format!("Storage error: {}", e)
        })),
    }
}

pub async fn download_object<S>(
    _req: HttpRequest,
    path: web::Path<(String, String, String)>,
    handler: web::Data<LfsHandler<S>>,
) -> impl Responder
where
    S: LfsStorage + 'static,
{
    let (tenant, repo, oid) = path.into_inner();

    if !handler.config.lfs.enabled {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    let key = StorageKey::new(tenant, repo, oid);

    match handler.storage.get_object(&key).await {
        Ok(Some(data)) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .body(data),
        Ok(None) => HttpResponse::NotFound().json(json!({
            "message": "Object not found"
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "message": format!("Storage error: {}", e)
        })),
    }
}

pub fn configure_lfs_routes<S>(cfg: &mut web::ServiceConfig)
where
    S: LfsStorage + 'static,
{
    cfg.service(
        web::scope("/{tenant}/{repo}")
            .route(
                "/info/lfs/objects/batch",
                web::post().to(batch_endpoint::<S>),
            )
            .route("/lfs/objects/{oid}", web::put().to(upload_object::<S>))
            .route("/lfs/objects/{oid}", web::get().to(download_object::<S>)),
    );
}
