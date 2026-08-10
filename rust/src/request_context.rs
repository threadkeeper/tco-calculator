use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

tokio::task_local! {
    static REQUEST_ID: String;
}

pub async fn assign_request_id(mut request: Request<Body>, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    let header = HeaderValue::from_str(&request_id).expect("UUIDs are valid header values");
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER, header.clone());

    REQUEST_ID
        .scope(request_id, async move {
            let mut response = next.run(request).await;
            response.headers_mut().insert(REQUEST_ID_HEADER, header);
            response
        })
        .await
}

pub fn request_id() -> String {
    REQUEST_ID
        .try_with(Clone::clone)
        .unwrap_or_else(|_| Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::{REQUEST_ID, request_id};

    #[tokio::test]
    async fn returns_the_scoped_request_id() {
        let expected = "4d57cb44-e84a-4dbe-a465-b810aa32bc45".to_owned();
        let actual = REQUEST_ID
            .scope(expected.clone(), async { request_id() })
            .await;

        assert_eq!(actual, expected);
    }
}
