use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::lfs::{
    record_batch_request, record_download, record_error, record_upload, Action, Actions,
    BatchRequest, BatchResponse, LfsStorage, ObjectError, Operation, QuotaManager, ResponseObject,
    StorageKey, Transfer,
};

pub struct LfsHandler<S> {
    storage: Arc<S>,
    config: AppConfig,
    quota_manager: Option<Arc<QuotaManager>>,
}

impl<S> LfsHandler<S>
where
    S: LfsStorage + 'static,
{
    pub fn new(storage: Arc<S>, config: AppConfig) -> Self {
        Self {
            storage,
            config,
            quota_manager: None,
        }
    }

    pub fn with_quota_manager(mut self, quota_manager: Arc<QuotaManager>) -> Self {
        self.quota_manager = Some(quota_manager);
        self
    }

    pub async fn handle_batch(
        &self,
        tenant: String,
        repo: String,
        batch_req: BatchRequest,
    ) -> Result<BatchResponse, ObjectError> {
        record_batch_request(&format!("{:?}", batch_req.operation), &tenant, &repo);

        if !self.config.lfs.enabled {
            record_error("lfs_disabled", &tenant, &repo);
            return Err(ObjectError {
                code: 503,
                message: "Git LFS is not enabled on this server".to_string(),
            });
        }

        let mut response_objects = Vec::new();

        for obj in batch_req.objects {
            if obj.size > self.config.lfs.max_object_size {
                record_error("object_too_large", &tenant, &repo);
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
                            record_error("object_not_found", &tenant, &repo);
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
                        record_error("storage_error", &tenant, &repo);
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
                    if let Some(ref quota_manager) = self.quota_manager {
                        if self.config.lfs.quota.enabled {
                            match quota_manager.check_quota(&tenant, &repo, obj.size).await {
                                Ok(_) => {}
                                Err(e) => {
                                    record_error("quota_exceeded", &tenant, &repo);
                                    response_objects.push(ResponseObject {
                                        oid: obj.oid.clone(),
                                        size: obj.size,
                                        authenticated: Some(true),
                                        actions: None,
                                        error: Some(ObjectError {
                                            code: 507,
                                            message: format!("Storage quota exceeded: {}", e),
                                        }),
                                    });
                                    continue;
                                }
                            }
                        }
                    }

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
        record_error("lfs_disabled", &tenant, &repo);
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if body.len() as u64 > handler.config.lfs.max_object_size {
        record_error("object_too_large", &tenant, &repo);
        return HttpResponse::PayloadTooLarge().json(json!({
            "message": format!("Object exceeds maximum size of {} bytes", handler.config.lfs.max_object_size)
        }));
    }

    if !crate::lfs::verify::verify_sha256(&body, &oid) {
        record_error("hash_mismatch", &tenant, &repo);
        return HttpResponse::UnprocessableEntity().json(json!({
            "message": "Object hash does not match OID"
        }));
    }

    let key = StorageKey::new(&tenant, &repo, &oid);

    match handler.storage.put_object(&key, body.to_vec()).await {
        Ok(()) => {
            record_upload(&tenant, &repo, body.len() as u64);

            if let Some(ref quota_manager) = handler.quota_manager {
                if handler.config.lfs.quota.enabled {
                    quota_manager
                        .update_usage(&tenant, &repo, body.len() as i64)
                        .await;
                }
            }

            HttpResponse::Ok().finish()
        }
        Err(e) => {
            record_error("storage_error", &tenant, &repo);
            HttpResponse::InternalServerError().json(json!({
                "message": format!("Storage error: {}", e)
            }))
        }
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
        record_error("lfs_disabled", &tenant, &repo);
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    let key = StorageKey::new(&tenant, &repo, &oid);

    match handler.storage.get_object(&key).await {
        Ok(Some(data)) => {
            record_download(&tenant, &repo, data.len() as u64);
            HttpResponse::Ok()
                .content_type("application/octet-stream")
                .body(data)
        }
        Ok(None) => {
            record_error("object_not_found", &tenant, &repo);
            HttpResponse::NotFound().json(json!({
                "message": "Object not found"
            }))
        }
        Err(e) => {
            record_error("storage_error", &tenant, &repo);
            HttpResponse::InternalServerError().json(json!({
                "message": format!("Storage error: {}", e)
            }))
        }
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
            .route("/lfs/objects/{oid}", web::get().to(download_object::<S>))
            .route("/quota", web::get().to(get_quota_info::<S>)),
    );
}

pub async fn get_quota_info<S>(
    _req: HttpRequest,
    path: web::Path<(String, String)>,
    handler: web::Data<LfsHandler<S>>,
) -> impl Responder
where
    S: LfsStorage + 'static,
{
    let (tenant, repo) = path.into_inner();

    if !handler.config.lfs.enabled {
        return HttpResponse::ServiceUnavailable().json(json!({
            "message": "Git LFS is not enabled on this server"
        }));
    }

    if !handler.config.lfs.quota.enabled {
        return HttpResponse::Ok().json(json!({
            "enabled": false,
            "message": "Quota enforcement is not enabled"
        }));
    }

    if let Some(ref quota_manager) = handler.quota_manager {
        let limit = quota_manager.get_quota_limit(&tenant, &repo);
        let (used, count) = quota_manager.get_current_usage(&tenant, &repo).await;

        HttpResponse::Ok().json(json!({
            "enabled": true,
            "tenant": tenant,
            "repo": repo,
            "limit_bytes": limit,
            "used_bytes": used,
            "available_bytes": limit.saturating_sub(used),
            "object_count": count,
            "usage_percent": if limit > 0 { (used as f64 / limit as f64 * 100.0) } else { 0.0 }
        }))
    } else {
        HttpResponse::Ok().json(json!({
            "enabled": false,
            "message": "Quota manager not initialized"
        }))
    }
}
