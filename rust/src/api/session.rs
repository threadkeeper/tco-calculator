use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct SessionResponse {
    mode: &'static str,
    display_name: Option<&'static str>,
}

pub async fn get_session() -> Json<SessionResponse> {
    Json(SessionResponse {
        mode: "guest",
        display_name: None,
    })
}
