use axum::{
    Json,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use serde::Serialize;

use crate::{domain::project::ValidationIssue, request_context};

#[derive(Debug, Serialize)]
pub struct ProblemDetail {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    instance: String,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<ValidationIssue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_etag: Option<String>,
}

struct ProblemResponse {
    status: StatusCode,
    detail: ProblemDetail,
    headers: HeaderMap,
}

pub struct Problem {
    response: Box<ProblemResponse>,
}

impl Problem {
    pub fn malformed_request(instance: &str) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "urn:azure-sql-tco:problem:malformed-request",
            "Malformed Request",
            instance,
            "The request body is not valid for this operation.",
        )
    }

    pub fn unauthorized(instance: &str, detail: &str) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "urn:azure-sql-tco:problem:unauthorized",
            "Unauthorized",
            instance,
            detail,
        )
    }

    pub fn not_found(instance: &str, detail: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "urn:azure-sql-tco:problem:not-found",
            "Not Found",
            instance,
            detail,
        )
    }

    pub fn gone(instance: &str, detail: &str) -> Self {
        Self::new(
            StatusCode::GONE,
            "urn:azure-sql-tco:problem:gone",
            "Gone",
            instance,
            detail,
        )
    }

    pub fn validation(instance: &str, errors: Vec<ValidationIssue>) -> Self {
        let mut problem = Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "urn:azure-sql-tco:problem:validation-error",
            "Validation Error",
            instance,
            "One or more request fields are invalid.",
        );
        problem.response.detail.errors = Some(errors);
        problem
    }

    pub fn precondition_required(instance: &str) -> Self {
        Self::new(
            StatusCode::PRECONDITION_REQUIRED,
            "urn:azure-sql-tco:problem:precondition-required",
            "Precondition Required",
            instance,
            "The If-Match header is required.",
        )
    }

    pub fn precondition_failed(instance: &str, current_etag: Option<&str>) -> Self {
        let mut problem = Self::new(
            StatusCode::PRECONDITION_FAILED,
            "urn:azure-sql-tco:problem:precondition-failed",
            "Precondition Failed",
            instance,
            "The project has changed; reload it before saving again.",
        );
        if let Some(current_etag) = current_etag {
            problem.response.detail.current_etag = Some(current_etag.to_owned());
            if let Ok(value) = HeaderValue::from_str(current_etag) {
                problem.response.headers.insert(header::ETAG, value);
            }
        }
        problem
    }

    pub fn payload_too_large(instance: &str) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "urn:azure-sql-tco:problem:payload-too-large",
            "Payload Too Large",
            instance,
            "The request or persisted project exceeds the allowed size.",
        )
    }

    pub fn snapshot_unavailable(instance: &str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "urn:azure-sql-tco:problem:snapshot-unavailable",
            "Snapshot Unavailable",
            instance,
            "A referenced price snapshot is unknown, expired, or does not match the project scope.",
        )
    }

    pub fn provider_unavailable(instance: &str) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "urn:azure-sql-tco:problem:provider-unavailable",
            "Provider Unavailable",
            instance,
            "The requested pricing operation is temporarily unavailable.",
        )
    }

    pub fn rate_limited(instance: &str, retry_after_seconds: u64) -> Self {
        let mut problem = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "urn:azure-sql-tco:problem:rate-limited",
            "Rate Limited",
            instance,
            "The request quota has been exceeded. Retry after the indicated delay.",
        );
        if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
            problem.response.headers.insert(header::RETRY_AFTER, value);
        }
        problem
    }

    pub fn internal(instance: &str) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "urn:azure-sql-tco:problem:internal-error",
            "Internal Error",
            instance,
            "The server could not complete the request.",
        )
    }

    pub fn not_implemented(instance: &str, detail: &str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "urn:azure-sql-tco:problem:not-implemented",
            "Not Implemented",
            instance,
            detail,
        )
    }

    fn new(
        status: StatusCode,
        problem_type: &'static str,
        title: &'static str,
        instance: &str,
        detail: &str,
    ) -> Self {
        Self {
            response: Box::new(ProblemResponse {
                status,
                detail: ProblemDetail {
                    problem_type,
                    title,
                    status: status.as_u16(),
                    detail: detail.to_owned(),
                    instance: instance.to_owned(),
                    request_id: request_context::request_id(),
                    errors: None,
                    current_etag: None,
                },
                headers: HeaderMap::new(),
            }),
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> axum::response::Response {
        let ProblemResponse {
            status,
            detail,
            headers,
        } = *self.response;
        let mut response = (
            status,
            [(axum::http::header::CONTENT_TYPE, "application/problem+json")],
            Json(detail),
        )
            .into_response();
        response.headers_mut().extend(headers);
        response
    }
}
