use crate::backend::fan_out;
use crate::error::Error;
use crate::k8s::BackendState;
use crate::webfinger::{merge_jrd, to_json_bytes};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

pub fn app(state: Arc<BackendState>) -> Router {
    Router::new()
        .route("/.well-known/webfinger", get(handle_webfinger))
        .route("/health", get(handle_health))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn handle_webfinger(
    State(state): State<Arc<BackendState>>,
    axum::extract::Query(params): axum::extract::Query<Vec<(String, String)>>,
) -> Result<Response<Body>, Error> {
    let resource = params
        .iter()
        .find(|(k, _)| k == "resource")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| Error::InvalidResource("missing resource param".to_string()))?;

    if !resource.starts_with("acct:") {
        return Err(Error::InvalidResource(resource));
    }

    let backends = state.get_all().await;
    if backends.is_empty() {
        return Err(Error::NoBackends);
    }

    info!(resource = %resource, backends = backends.len(), "handling webfinger request");

    let sem = Arc::new(tokio::sync::Semaphore::new(10));
    let results = fan_out(&backends, &resource, sem).await;

    if results.is_empty() {
        return Err(Error::AllBackendsFailed);
    }

    let merged = merge_jrd(results);
    let body = to_json_bytes(&merged)?;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/jrd+json".parse().unwrap());

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/jrd+json")
        .body(Body::from(body))
        .unwrap())
}

async fn handle_health(State(state): State<Arc<BackendState>>) -> Response<Body> {
    let backends = state.get_all().await;
    let body = serde_json::json!({
        "backends": backends.len()
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
