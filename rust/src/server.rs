use std::path::{Component, Path, PathBuf};

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tower_http::{
    compression::CompressionLayer, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};

use crate::{
    api,
    config::AppEnvironment,
    health,
    problem::Problem,
    rate_limit, request_context,
    state::{AppState, StateError},
};

const INDEX_HTML: &str = include_str!("../static/index.html");

#[derive(Debug, Error)]
pub enum ServerError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error("WEB_ASSET_DIR must contain index.html outside local mode: {0}")]
    MissingStaticIndex(PathBuf),
    #[error("WEB_ASSET_DIR index.html contains malformed script markup: {0}")]
    InvalidStaticIndex(PathBuf),
}

pub async fn router(mut config: crate::config::Config) -> Result<Router, ServerError> {
    config.web_asset_dir = validate_asset_root(&config)?;
    let content_security_policy = content_security_policy(&config)?;
    let state = AppState::new(config).await?;
    let consent_gated_router = Router::new()
        .route("/assistant/help", post(api::assistant::help))
        .route("/assistant/turn", post(api::assistant::turn))
        .route("/catalog/aws/regions", get(api::catalog::aws_regions))
        .route("/catalog/azure/regions", get(api::catalog::azure_regions))
        .route(
            "/catalog/aws/ec2/instances",
            get(api::catalog::ec2_instances),
        )
        .route(
            "/catalog/aws/rds/instances",
            get(api::catalog::rds_instances),
        )
        .route("/catalog/aws/rds/options", get(api::catalog::rds_options))
        .route("/catalog/aws/ebs/types", get(api::catalog::ebs_types))
        .route(
            "/catalog/azure/mi/purchase-options",
            get(api::catalog::purchase_options),
        )
        .route("/pricing/aws/resolve", post(api::pricing::resolve_aws))
        .route(
            "/pricing/aws/refresh",
            post(api::pricing::refresh_aws).layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit::enforce_refresh_quota,
            )),
        )
        .route("/pricing/azure/resolve", post(api::pricing::resolve_azure))
        .route(
            "/pricing/azure/refresh",
            post(api::pricing::refresh_azure).layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit::enforce_refresh_quota,
            )),
        )
        .route("/calculations", post(api::calculations::calculate))
        .route(
            "/projects",
            get(api::projects::list).post(api::projects::create),
        )
        .route(
            "/projects/{project_id}",
            get(api::projects::get)
                .put(api::projects::update)
                .delete(api::projects::delete),
        )
        .route(
            "/projects/{project_id}/shares",
            post(api::project_shares::create),
        )
        .route(
            "/projects/{project_id}/shares/{share_id}",
            axum::routing::delete(api::project_shares::revoke),
        )
        .route(
            "/project-shares/resolve",
            post(api::project_shares::resolve),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api::privacy::enforce_accepted_consent,
        ));
    let api_router = Router::new()
        .route("/session", get(api::session::get_session))
        .route("/privacy-consent", put(api::privacy::save))
        .merge(consent_gated_router)
        .fallback(api_not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::enforce_guest_quota,
        ))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(state.clone());

    Ok(Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/version", get(health::version))
        .nest("/api/v1", api_router)
        .fallback(static_asset)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            content_security_policy,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(request_context::assign_request_id)))
}

fn validate_asset_root(config: &crate::config::Config) -> Result<PathBuf, ServerError> {
    let index = config.web_asset_dir.join("index.html");
    if config.environment != AppEnvironment::Local && !index.is_file() {
        return Err(ServerError::MissingStaticIndex(index));
    }
    Ok(config
        .web_asset_dir
        .canonicalize()
        .unwrap_or_else(|_| config.web_asset_dir.clone()))
}

fn content_security_policy(config: &crate::config::Config) -> Result<HeaderValue, ServerError> {
    let index_path = config.web_asset_dir.join("index.html");
    let index = match std::fs::read_to_string(&index_path) {
        Ok(index) => index,
        Err(_) if config.environment == AppEnvironment::Local => INDEX_HTML.to_owned(),
        Err(_) => return Err(ServerError::MissingStaticIndex(index_path)),
    };
    let policy = content_security_policy_for_index(&index, &index_path)?;
    HeaderValue::from_str(&policy).map_err(|_| ServerError::InvalidStaticIndex(index_path))
}

fn content_security_policy_for_index(
    index: &str,
    index_path: &Path,
) -> Result<String, ServerError> {
    let script_hashes = inline_script_hashes(index, index_path)?;
    let mut policy = String::from(
        "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'",
    );
    for script_hash in script_hashes {
        policy.push_str(" 'sha256-");
        policy.push_str(&script_hash);
        policy.push('\'');
    }
    policy.push_str("; frame-ancestors 'none'; base-uri 'self'; form-action 'self'");
    Ok(policy)
}

fn inline_script_hashes(index: &str, index_path: &Path) -> Result<Vec<String>, ServerError> {
    let mut hashes = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = index[offset..].find("<script") {
        let start = offset + relative_start;
        let tag_end = index[start..]
            .find('>')
            .map(|relative_end| start + relative_end)
            .ok_or_else(|| ServerError::InvalidStaticIndex(index_path.to_path_buf()))?;
        let close_start = index[tag_end + 1..]
            .find("</script>")
            .map(|relative_end| tag_end + 1 + relative_end)
            .ok_or_else(|| ServerError::InvalidStaticIndex(index_path.to_path_buf()))?;
        let start_tag = &index[start..=tag_end];
        if !start_tag
            .split_ascii_whitespace()
            .any(|part| part.starts_with("src="))
        {
            let script = &index[tag_end + 1..close_start];
            hashes.push(STANDARD.encode(Sha256::digest(script.as_bytes())));
        }
        offset = close_start + "</script>".len();
    }
    Ok(hashes)
}

async fn static_asset(State(state): State<AppState>, request: Request<Body>) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let request_path = request.uri().path();
    let Some(relative_path) = safe_relative_path(request_path) else {
        return Problem::not_found(request_path, "The requested asset does not exist.")
            .into_response();
    };
    let root = &state.config.web_asset_dir;
    let requested = root.join(&relative_path);
    let asset_path = match tokio::fs::canonicalize(&requested).await {
        Ok(path) if path.starts_with(root) && path.is_file() => Some(path),
        _ if relative_path.extension().is_none() => {
            let index = root.join("index.html");
            tokio::fs::canonicalize(index).await.ok()
        }
        _ => None,
    };

    let Some(asset_path) = asset_path.filter(|path| path.starts_with(root) && path.is_file())
    else {
        if relative_path.extension().is_none() && state.config.environment == AppEnvironment::Local
        {
            return embedded_index(request.method() == Method::HEAD);
        }
        return Problem::not_found(request_path, "The requested asset does not exist.")
            .into_response();
    };

    match tokio::fs::read(&asset_path).await {
        Ok(bytes) => asset_response(
            &asset_path,
            bytes,
            request.method() == Method::HEAD,
            asset_path
                .file_name()
                .is_some_and(|name| name == "index.html"),
        ),
        Err(_) => Problem::internal(request_path).into_response(),
    }
}

fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    if request_path.contains('%') || request_path.contains('\\') {
        return None;
    }
    let value = request_path.trim_start_matches('/');
    if value.is_empty() {
        return Some(PathBuf::from("index.html"));
    }
    let path = Path::new(value);
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

fn embedded_index(head_only: bool) -> Response {
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(INDEX_HTML)
    };
    response_with_headers(body, "text/html; charset=utf-8", "no-cache")
}

fn asset_response(path: &Path, bytes: Vec<u8>, head_only: bool, index: bool) -> Response {
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(bytes)
    };
    let cache_control = if index {
        "no-cache"
    } else if path
        .components()
        .any(|component| component.as_os_str() == "immutable")
    {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };
    response_with_headers(body, content_type(path), cache_control)
}

fn response_with_headers(
    body: Body,
    content_type: &'static str,
    cache_control: &'static str,
) -> Response {
    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    response
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") | Some("map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn api_not_found(request: Request<Body>) -> Problem {
    Problem::not_found(
        request.uri().path(),
        "The requested API route does not exist.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_each_inline_script_exactly() {
        let index = "<script>console.log('one')</script><script type=\"module\">two()</script>";
        let hashes = inline_script_hashes(index, Path::new("index.html")).expect("valid scripts");

        assert_eq!(
            hashes,
            [
                "H3QXV/uMHFl6e+0SaK2PX1M3PYINSznPfB3wKKLHnSs=",
                "iMrPAWirDC2X0OhZzLsM2MRIWSmepJGI4r7D8ZlwBOw="
            ]
        );
    }

    #[test]
    fn policy_allows_exact_inline_script_hashes_without_unsafe_inline() {
        let policy = content_security_policy_for_index(
            "<script>console.log('one')</script>",
            Path::new("index.html"),
        )
        .expect("valid policy");

        assert!(
            policy.contains(
                "script-src 'self' 'sha256-H3QXV/uMHFl6e+0SaK2PX1M3PYINSznPfB3wKKLHnSs='"
            )
        );
        assert!(!policy.contains("script-src 'self' 'unsafe-inline'"));
        assert!(policy.contains("style-src 'self' 'unsafe-inline'"));
    }

    #[test]
    fn ignores_external_scripts_and_accepts_script_free_html() {
        assert!(
            inline_script_hashes("<script src=\"/app.js\"></script>", Path::new("index.html"))
                .expect("valid external script")
                .is_empty()
        );
        assert!(
            inline_script_hashes("<main>Static</main>", Path::new("index.html"))
                .expect("script-free HTML")
                .is_empty()
        );
    }

    #[test]
    fn rejects_unclosed_script_markup() {
        assert!(matches!(
            inline_script_hashes("<script>broken", Path::new("index.html")),
            Err(ServerError::InvalidStaticIndex(_))
        ));
    }
}
