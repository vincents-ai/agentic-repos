use crate::features::FeatureFlags;
use crate::git::GitService;
use actix_files::Files;
use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;
use tera::{Context, Tera};

#[cfg(test)]
mod tests;

pub struct WebUI {
    git_service: Arc<GitService>,
    templates: Tera,
    feature_flags: FeatureFlags,
}

impl WebUI {
    pub fn new(
        git_service: Arc<GitService>,
        feature_flags: FeatureFlags,
    ) -> Result<Self, tera::Error> {
        if !feature_flags.web_ui {
            return Err(tera::Error::msg("Web UI feature is not enabled"));
        }

        let mut tera = Tera::new("templates/**/*.html")?;
        tera.autoescape_on(vec![".html"]);

        Ok(Self {
            git_service,
            templates: tera,
            feature_flags,
        })
    }

    pub fn configure(cfg: &mut web::ServiceConfig, web_ui: Arc<Self>) {
        if !web_ui.feature_flags.web_ui {
            return;
        }

        cfg.service(
            web::scope("/ui")
                .app_data(web::Data::new(web_ui))
                .route("/", web::get().to(Self::dashboard))
                .route("/repositories", web::get().to(Self::repositories))
                .route(
                    "/repository/{owner}/{repo}",
                    web::get().to(Self::repository_detail),
                )
                .service(Files::new("/static", "static").show_files_listing()),
        );
    }

    async fn dashboard(data: web::Data<Arc<Self>>) -> Result<HttpResponse> {
        if !data.feature_flags.web_ui {
            return Ok(HttpResponse::NotFound().body("Web UI feature not enabled"));
        }

        let mut context = Context::new();
        context.insert("title", "Repository Dashboard");
        context.insert("features", &data.feature_flags);

        match data.git_service.list_repositories(None).await {
            Ok(repos) => {
                let limited_repos: Vec<_> = repos.into_iter().take(10).collect();
                context.insert("repositories", &limited_repos);
            }
            Err(_) => context.insert("repositories", &Vec::<crate::git::RepositoryInfo>::new()),
        }

        let html = data
            .templates
            .render("dashboard.html", &context)
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

        Ok(HttpResponse::Ok().content_type("text/html").body(html))
    }

    async fn repositories(data: web::Data<Arc<Self>>) -> Result<HttpResponse> {
        if !data.feature_flags.web_ui {
            return Ok(HttpResponse::NotFound().body("Web UI feature not enabled"));
        }

        let mut context = Context::new();
        context.insert("title", "All Repositories");
        context.insert("features", &data.feature_flags);

        match data.git_service.list_repositories(None).await {
            Ok(repos) => context.insert("repositories", &repos),
            Err(_) => context.insert("repositories", &Vec::<crate::git::RepositoryInfo>::new()),
        }

        let html = data
            .templates
            .render("repositories.html", &context)
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

        Ok(HttpResponse::Ok().content_type("text/html").body(html))
    }

    async fn repository_detail(
        data: web::Data<Arc<Self>>,
        path: web::Path<(String, String)>,
    ) -> Result<HttpResponse> {
        if !data.feature_flags.web_ui {
            return Ok(HttpResponse::NotFound().body("Web UI feature not enabled"));
        }

        let (owner, repo) = path.into_inner();
        let mut context = Context::new();
        context.insert("title", &format!("{}/{}", owner, repo));
        context.insert("owner", &owner);
        context.insert("repo", &repo);
        context.insert("features", &data.feature_flags);

        match data.git_service.get_repository(&owner, &repo).await {
            Ok(repo_info) => {
                context.insert("repository", &repo_info);

                match data.git_service.list_branches(&owner, &repo).await {
                    Ok(branches) => context.insert("branches", &branches),
                    Err(_) => context.insert("branches", &Vec::<crate::git::BranchInfo>::new()),
                }
            }
            Err(_) => {
                return Ok(HttpResponse::NotFound()
                    .content_type("text/html")
                    .body("<h1>Repository not found</h1>"));
            }
        }

        let html = data
            .templates
            .render("repository.html", &context)
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

        Ok(HttpResponse::Ok().content_type("text/html").body(html))
    }
}
