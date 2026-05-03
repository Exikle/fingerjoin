use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("webfinger parse error: {0}")]
    Webfinger(#[from] super::webfinger::Error),

    #[error("http request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("invalid resource format: {0}")]
    InvalidResource(String),

    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("no backends available")]
    NoBackends,

    #[error("all backends failed")]
    AllBackendsFailed,
}

impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            Error::InvalidResource(_) => (axum::http::StatusCode::BAD_REQUEST, self.to_string()),
            Error::NoBackends => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "no backends configured".to_string(),
            ),
            Error::AllBackendsFailed => (
                axum::http::StatusCode::BAD_GATEWAY,
                "all backends failed".to_string(),
            ),
            _ => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            ),
        };

        let body = serde_json::json!({
            "error": msg
        });

        axum::response::Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap()
    }
}
