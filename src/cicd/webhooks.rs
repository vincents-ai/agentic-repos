//! CI/CD webhook routing — routes incoming payloads from GitHub, GitLab, and Jenkins
//! to the appropriate parser and stores the resulting [`BuildReport`].

use crate::cicd::github::parse_github_event;
use crate::cicd::gitlab::parse_gitlab_event;
use crate::cicd::jenkins::parse_jenkins_event;
use crate::cicd::models::{BuildReport, CiWebhookEvent};
use crate::cicd::store::BuildReportStore;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebhookRoutingError {
    #[error("Unsupported provider: {0}")]
    UnsupportedProvider(String),
    #[error("GitHub parse error: {0}")]
    Github(#[from] crate::cicd::github::GithubParseError),
    #[error("GitLab parse error: {0}")]
    Gitlab(#[from] crate::cicd::gitlab::GitlabParseError),
    #[error("Jenkins parse error: {0}")]
    Jenkins(#[from] crate::cicd::jenkins::JenkinsParseError),
    #[error("Store error: {0}")]
    Store(#[from] crate::cicd::store::StoreError),
}

/// Identifies the incoming CI/CD provider from request headers / path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderHint {
    /// The `X-GitHub-Event` header was present.
    Github { event_type: String },
    /// The `X-Gitlab-Event` header was present.
    Gitlab,
    /// The request arrived at the Jenkins-specific endpoint.
    Jenkins,
}

/// Route an incoming CI/CD webhook to the correct parser and persist the build report.
///
/// Returns the stored [`BuildReport`].
pub fn route_webhook(
    store: &dyn BuildReportStore,
    hint: ProviderHint,
    payload: serde_json::Value,
) -> Result<BuildReport, WebhookRoutingError> {
    let event: CiWebhookEvent = match hint {
        ProviderHint::Github { ref event_type } => parse_github_event(event_type, payload)?,
        ProviderHint::Gitlab => parse_gitlab_event(payload)?,
        ProviderHint::Jenkins => parse_jenkins_event(payload)?,
    };

    let report = BuildReport::new(
        event.commit_sha.clone(),
        event.branch.clone().unwrap_or_default(),
        event.repository.clone(),
        event.provider.clone(),
        event.pipeline_id.clone(),
    )
    .with_status(event.status.clone());

    let report = if let Some(url) = event.pipeline_url {
        report.with_url(url)
    } else {
        report
    };

    let stored = store.upsert(report)?;
    Ok(stored)
}

/// actix-web handlers for the CI/CD webhook endpoints.
pub mod handlers {
    use super::*;
    use crate::cicd::store::InMemoryBuildReportStore;
    use actix_web::{web, HttpRequest, HttpResponse, Result as ActixResult};

    /// Get the webhook secret from environment variable.
    /// Each provider has its own env var: CICD_GITHUB_SECRET, CICD_GITLAB_TOKEN, CICD_JENKINS_SECRET.
    fn get_webhook_secret(env_key: &str) -> Option<String> {
        std::env::var(env_key).ok()
    }

    /// Verify GitHub webhook signature (HMAC-SHA256).
    ///
    /// GitHub sends `X-Hub-Signature-256: sha256=<hex_hmac>`.
    /// We compute HMAC-SHA256 over the raw body with the shared secret and
    /// compare using constant-time comparison.
    fn verify_github_signature(req: &HttpRequest, body: &[u8]) -> Result<(), WebhookRoutingError> {
        let secret = match get_webhook_secret("CICD_GITHUB_SECRET") {
            Some(s) => s,
            None => {
                // No secret configured — allow through but log warning
                tracing::warn!(
                    "No CICD_GITHUB_SECRET configured — webhook signature verification disabled"
                );
                return Ok(());
            }
        };

        let sig_header = req
            .headers()
            .get("X-Hub-Signature-256")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                WebhookRoutingError::UnsupportedProvider(
                    "Missing X-Hub-Signature-256 header".to_string(),
                )
            })?;

        let expected_hex = sig_header.strip_prefix("sha256=").ok_or_else(|| {
            WebhookRoutingError::UnsupportedProvider(
                "Invalid X-Hub-Signature-256 format — expected 'sha256=...'".to_string(),
            )
        })?;

        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(body);
        let computed = mac.finalize().into_bytes();

        // Constant-time comparison via hmac crate
        let expected_bytes: Vec<u8> = hex::decode(expected_hex).map_err(|_| {
            WebhookRoutingError::UnsupportedProvider(
                "Invalid hex in X-Hub-Signature-256".to_string(),
            )
        })?;

        if expected_bytes.len() != computed.as_slice().len() {
            return Err(WebhookRoutingError::UnsupportedProvider(
                "Webhook signature verification failed".to_string(),
            ));
        }

        // Constant-time byte comparison
        let mut diff: u8 = 0;
        for (a, b) in computed.as_slice().iter().zip(expected_bytes.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(WebhookRoutingError::UnsupportedProvider(
                "Webhook signature verification failed".to_string(),
            ));
        }

        Ok(())
    }

    /// Verify GitLab webhook token.
    ///
    /// GitLab sends `X-Gitlab-Token: <token>` header.
    /// Constant-time comparison against configured token.
    fn verify_gitlab_token(req: &HttpRequest) -> Result<(), WebhookRoutingError> {
        let secret = match get_webhook_secret("CICD_GITLAB_TOKEN") {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "No CICD_GITLAB_TOKEN configured — webhook token verification disabled"
                );
                return Ok(());
            }
        };

        let token = req
            .headers()
            .get("X-Gitlab-Token")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                WebhookRoutingError::UnsupportedProvider(
                    "Missing X-Gitlab-Token header".to_string(),
                )
            })?;

        // Constant-time comparison
        if token.as_bytes().len() != secret.as_bytes().len() {
            return Err(WebhookRoutingError::UnsupportedProvider(
                "GitLab webhook token verification failed".to_string(),
            ));
        }
        let mut diff: u8 = 0;
        for (a, b) in token.as_bytes().iter().zip(secret.as_bytes().iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(WebhookRoutingError::UnsupportedProvider(
                "GitLab webhook token verification failed".to_string(),
            ));
        }

        Ok(())
    }

    /// Verify Jenkins webhook signature (HMAC-SHA1 or SHA256).
    ///
    /// Jenkins can send `X-Jenkins-Signature: <hex_hmac>`.
    fn verify_jenkins_signature(req: &HttpRequest, body: &[u8]) -> Result<(), WebhookRoutingError> {
        let secret = match get_webhook_secret("CICD_JENKINS_SECRET") {
            Some(s) => s,
            None => {
                tracing::warn!(
                    "No CICD_JENKINS_SECRET configured — webhook signature verification disabled"
                );
                return Ok(());
            }
        };

        let sig_header = req
            .headers()
            .get("X-Jenkins-Signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                WebhookRoutingError::UnsupportedProvider(
                    "Missing X-Jenkins-Signature header".to_string(),
                )
            })?;

        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(body);
        let computed = mac.finalize().into_bytes();

        let expected_hex = sig_header.strip_prefix("sha256=").unwrap_or(sig_header);
        let expected_bytes: Vec<u8> = hex::decode(expected_hex).map_err(|_| {
            WebhookRoutingError::UnsupportedProvider(
                "Invalid hex in X-Jenkins-Signature".to_string(),
            )
        })?;

        let mut diff: u8 = 0;
        for (a, b) in computed.as_slice().iter().zip(expected_bytes.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(WebhookRoutingError::UnsupportedProvider(
                "Jenkins webhook signature verification failed".to_string(),
            ));
        }

        Ok(())
    }

    pub fn configure_cicd_routes(cfg: &mut web::ServiceConfig) {
        cfg.route("/webhooks/github", web::post().to(github_webhook))
            .route("/webhooks/gitlab", web::post().to(gitlab_webhook))
            .route("/webhooks/jenkins", web::post().to(jenkins_webhook))
            .route("/builds/{repo_id}", web::get().to(list_builds))
            .route(
                "/builds/{repo_id}/{commit}",
                web::get().to(list_builds_for_commit),
            );
    }

    async fn github_webhook(
        store: web::Data<Arc<InMemoryBuildReportStore>>,
        req: HttpRequest,
        payload: web::Json<serde_json::Value>,
    ) -> ActixResult<HttpResponse> {
        let event_type = req
            .headers()
            .get("X-GitHub-Event")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        // Verify GitHub HMAC-SHA256 signature
        let body_bytes = serde_json::to_vec(&payload).unwrap_or_default();
        if let Err(e) = verify_github_signature(&req, &body_bytes) {
            return Ok(
                HttpResponse::Unauthorized().json(serde_json::json!({"error": e.to_string()}))
            );
        }

        match route_webhook(
            store.as_ref().as_ref(),
            ProviderHint::Github { event_type },
            payload.into_inner(),
        ) {
            Ok(report) => Ok(HttpResponse::Ok().json(report)),
            Err(WebhookRoutingError::UnsupportedProvider(e))
            | Err(WebhookRoutingError::Github(
                crate::cicd::github::GithubParseError::UnknownEvent(e),
            )) => Ok(HttpResponse::UnprocessableEntity()
                .json(serde_json::json!({"error": format!("unsupported event: {}", e)}))),
            Err(e) => {
                Ok(HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})))
            }
        }
    }

    async fn gitlab_webhook(
        store: web::Data<Arc<InMemoryBuildReportStore>>,
        req: HttpRequest,
        payload: web::Json<serde_json::Value>,
    ) -> ActixResult<HttpResponse> {
        // Verify GitLab token
        if let Err(e) = verify_gitlab_token(&req) {
            return Ok(
                HttpResponse::Unauthorized().json(serde_json::json!({"error": e.to_string()}))
            );
        }

        match route_webhook(
            store.as_ref().as_ref(),
            ProviderHint::Gitlab,
            payload.into_inner(),
        ) {
            Ok(report) => Ok(HttpResponse::Ok().json(report)),
            Err(e) => {
                Ok(HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})))
            }
        }
    }

    async fn jenkins_webhook(
        store: web::Data<Arc<InMemoryBuildReportStore>>,
        req: HttpRequest,
        payload: web::Json<serde_json::Value>,
    ) -> ActixResult<HttpResponse> {
        // Verify Jenkins HMAC
        let body_bytes = serde_json::to_vec(&payload).unwrap_or_default();
        if let Err(e) = verify_jenkins_signature(&req, &body_bytes) {
            return Ok(
                HttpResponse::Unauthorized().json(serde_json::json!({"error": e.to_string()}))
            );
        }

        match route_webhook(
            store.as_ref().as_ref(),
            ProviderHint::Jenkins,
            payload.into_inner(),
        ) {
            Ok(report) => Ok(HttpResponse::Ok().json(report)),
            Err(e) => {
                Ok(HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()})))
            }
        }
    }

    async fn list_builds(
        store: web::Data<Arc<InMemoryBuildReportStore>>,
        path: web::Path<String>,
    ) -> ActixResult<HttpResponse> {
        let repo_id = path.into_inner();
        match store.list_by_repo(&repo_id) {
            Ok(reports) => Ok(HttpResponse::Ok().json(reports)),
            Err(e) => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        }
    }

    async fn list_builds_for_commit(
        store: web::Data<Arc<InMemoryBuildReportStore>>,
        path: web::Path<(String, String)>,
    ) -> ActixResult<HttpResponse> {
        let (repo_id, commit) = path.into_inner();
        match store.list_by_commit(&repo_id, &commit) {
            Ok(reports) => Ok(HttpResponse::Ok().json(reports)),
            Err(e) => Ok(HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cicd::models::BuildStatus;
    use crate::cicd::store::InMemoryBuildReportStore;

    fn gh_workflow_payload(status: &str, conclusion: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "action": "completed",
            "workflow_run": {
                "id": 1,
                "head_sha": "sha1",
                "head_branch": "main",
                "status": status,
                "conclusion": conclusion,
                "html_url": null
            },
            "repository": { "full_name": "org/repo" }
        })
    }

    #[test]
    fn test_route_github_success() {
        let store = InMemoryBuildReportStore::new();
        let report = route_webhook(
            &store,
            ProviderHint::Github {
                event_type: "workflow_run".to_string(),
            },
            gh_workflow_payload("completed", Some("success")),
        )
        .unwrap();
        assert_eq!(report.status, BuildStatus::Success);
        assert_eq!(report.repository_id, "org/repo");
        // Report is persisted
        let stored = store.get(&report.id).unwrap().unwrap();
        assert_eq!(stored.id, report.id);
    }

    #[test]
    fn test_route_gitlab() {
        let store = InMemoryBuildReportStore::new();
        let payload = serde_json::json!({
            "object_kind": "pipeline",
            "object_attributes": {
                "id": 5,
                "status": "success",
                "ref": "main",
                "sha": "aaabbb",
                "url": null
            },
            "project": { "path_with_namespace": "group/proj", "web_url": null },
            "commit": null
        });
        let report = route_webhook(&store, ProviderHint::Gitlab, payload).unwrap();
        assert_eq!(report.status, BuildStatus::Success);
    }

    #[test]
    fn test_route_jenkins() {
        let store = InMemoryBuildReportStore::new();
        let payload = serde_json::json!({
            "name": "my-job",
            "url": null,
            "build": {
                "number": 7,
                "phase": "COMPLETED",
                "status": "SUCCESS",
                "url": null,
                "scm": { "url": "https://github.com/org/repo.git", "branch": "main", "commit": "deadbeef" }
            }
        });
        let report = route_webhook(&store, ProviderHint::Jenkins, payload).unwrap();
        assert_eq!(report.status, BuildStatus::Success);
        assert_eq!(report.commit_sha, "deadbeef");
    }

    #[test]
    fn test_route_unknown_github_event_returns_error() {
        let store = InMemoryBuildReportStore::new();
        let result = route_webhook(
            &store,
            ProviderHint::Github {
                event_type: "pull_request".to_string(),
            },
            serde_json::json!({}),
        );
        assert!(result.is_err());
    }
}
