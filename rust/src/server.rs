use axum::{
    Router,
    body::Body,
    http::{HeaderName, HeaderValue, Request, header},
    response::Html,
    routing::{get, post},
};
use tower_http::{
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{api, health, problem::Problem};

const INDEX_HTML: &str = include_str!("../static/index.html");
const REQUEST_ID_HEADER: &str = "x-request-id";

pub fn router(_config: crate::config::Config) -> Router {
    let request_id_header = HeaderName::from_static(REQUEST_ID_HEADER);
    let api_router = Router::new()
        .route("/session", get(api::session::get_session))
        .route("/catalog/aws/regions", get(api::catalog::aws_regions))
        .route(
            "/catalog/azure/mi/purchase-options",
            get(api::catalog::purchase_options),
        )
        .route("/pricing/aws/resolve", post(api::pricing::resolve_aws))
        .route("/pricing/aws/refresh", post(api::pricing::refresh_aws))
        .route("/pricing/azure/resolve", post(api::pricing::resolve_azure))
        .route("/pricing/azure/refresh", post(api::pricing::refresh_azure))
        .route("/calculations", post(api::calculations::calculate))
        .route(
            "/projects",
            get(api::projects::unauthorized).post(api::projects::unauthorized),
        )
        .route(
            "/projects/{project_id}",
            get(api::projects::unauthorized)
                .put(api::projects::unauthorized)
                .delete(api::projects::unauthorized),
        )
        .fallback(api_not_found);

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/version", get(health::version))
        .nest("/api/v1", api_router)
        .fallback(index)
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
            HeaderValue::from_static(
                "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
            ),
        ))
        .layer(RequestBodyLimitLayer::new(1_048_576))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(SetRequestIdLayer::new(
            request_id_header,
            MakeRequestUuid,
        ))
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_not_found(request: Request<Body>) -> Problem {
    Problem::not_found(
        request.uri().path(),
        "The requested API route does not exist.",
    )
}
