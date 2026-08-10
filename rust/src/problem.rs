use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ProblemDetail {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    instance: String,
    request_id: String,
}

pub struct Problem {
    status: StatusCode,
    detail: ProblemDetail,
}

impl Problem {
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
            status,
            detail: ProblemDetail {
                problem_type,
                title,
                status: status.as_u16(),
                detail: detail.to_owned(),
                instance: instance.to_owned(),
                request_id: Uuid::new_v4().to_string(),
            },
        }
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            [(axum::http::header::CONTENT_TYPE, "application/problem+json")],
            Json(self.detail),
        )
            .into_response()
    }
}
