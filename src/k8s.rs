use crate::backend::Backend;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tracing::{error, info};

const WEBFINGER_KEY: &str = "fingerjoin.naktis.eu/webfinger";
const PRIORITY_KEY: &str = "fingerjoin.naktis.eu/priority";
const HTTPS_KEY: &str = "fingerjoin.naktis.eu/https";
const BACKEND_KEY: &str = "fingerjoin.naktis.eu/backend";

pub struct BackendState {
    backends: RwLock<Vec<Backend>>,
}

impl BackendState {
    pub fn new() -> Self {
        Self {
            backends: RwLock::new(Vec::new()),
        }
    }

    pub async fn update(&self, new_backends: Vec<Backend>) {
        let mut backends = self.backends.write().await;
        *backends = new_backends.clone();
        info!(count = new_backends.len(), "backends updated");
    }

    pub async fn get_all(&self) -> Vec<Backend> {
        self.backends.read().await.clone()
    }
}

impl Default for BackendState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Deserialize)]
struct RouteList {
    items: Vec<Route>,
}

#[derive(Debug, serde::Deserialize)]
struct Route {
    metadata: Metadata,
    #[serde(default)]
    spec: RouteSpec,
}

#[derive(Debug, serde::Deserialize)]
struct Metadata {
    annotations: Option<BTreeMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RouteSpec {
    rules: Vec<RouteRule>,
}

#[derive(Debug, serde::Deserialize)]
struct RouteRule {
    #[serde(rename = "backendRefs", default)]
    backend_refs: Vec<BackendRef>,
}

#[derive(Debug, serde::Deserialize)]
struct BackendRef {
    backend: BackendName,
    #[serde(rename = "port", default)]
    port: Option<u16>,
}

#[derive(Debug, serde::Deserialize)]
struct BackendName {
    name: String,
}

pub async fn start_reconciler(client: kube::Client, state: Arc<BackendState>) {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri("/apis/gateway.networking.k8s.io/v1/httproutes?limit=100")
            .body(Vec::new())
            .unwrap();

        let routes: RouteList = match client.request(request).await {
            Ok(r) => r,
            Err(e) => {
                error!(err = %e, "failed to list httproutes");
                continue;
            }
        };

        let mut all_backends = Vec::new();
        for route in routes.items {
            let annotations = match route.metadata.annotations.as_ref() {
                Some(a) => a,
                None => continue,
            };

            let is_webfinger = annotations
                .get(WEBFINGER_KEY)
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if !is_webfinger {
                continue;
            }

            let priority = annotations
                .get(PRIORITY_KEY)
                .and_then(|v| v.parse().ok())
                .unwrap_or(50);

            let https = annotations
                .get(HTTPS_KEY)
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            let backend_index: usize = annotations
                .get(BACKEND_KEY)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);

            let rule = match route.spec.rules.get(backend_index) {
                Some(r) => r,
                None => continue,
            };

            let backend = match rule.backend_refs.first() {
                Some(b) => b,
                None => continue,
            };

            let port = backend.port.unwrap_or(if https { 443 } else { 80 });
            let scheme = if https { "https" } else { "http" };
            let url = format!("{}://{}:{}", scheme, backend.backend.name, port);
            all_backends.push(Backend {
                name: backend.backend.name.clone(),
                url: url::Url::parse(&url).unwrap(),
                priority,
            });
        }
        state.update(all_backends).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_annotations(
        webfinger: Option<&str>,
        priority: Option<&str>,
        https: Option<&str>,
        backend: Option<&str>,
    ) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        if let Some(v) = webfinger {
            m.insert(WEBFINGER_KEY.to_string(), v.to_string());
        }
        if let Some(v) = priority {
            m.insert(PRIORITY_KEY.to_string(), v.to_string());
        }
        if let Some(v) = https {
            m.insert(HTTPS_KEY.to_string(), v.to_string());
        }
        if let Some(v) = backend {
            m.insert(BACKEND_KEY.to_string(), v.to_string());
        }
        m
    }

    #[test]
    fn test_webfinger_annotation_exists() {
        let a = make_annotations(Some("true"), None, None, None);
        assert!(a.get(WEBFINGER_KEY).map(|v| v == "true").unwrap_or(false));
    }

    #[test]
    fn test_priority_default() {
        let a = make_annotations(Some("true"), None, None, None);
        let priority = a
            .get(PRIORITY_KEY)
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        assert_eq!(priority, 50);
    }

    #[test]
    fn test_priority_custom() {
        let a = make_annotations(Some("true"), Some("100"), None, None);
        let priority = a
            .get(PRIORITY_KEY)
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        assert_eq!(priority, 100);
    }

    #[test]
    fn test_https_false() {
        let a = make_annotations(Some("true"), None, Some("false"), None);
        let https = a
            .get(HTTPS_KEY)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        assert!(!https);
    }

    #[test]
    fn test_https_true() {
        let a = make_annotations(Some("true"), None, Some("true"), None);
        let https = a
            .get(HTTPS_KEY)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        assert!(https);
    }

    #[test]
    fn test_backend_index_default() {
        let a = make_annotations(Some("true"), None, None, None);
        let idx: usize = a.get(BACKEND_KEY).and_then(|v| v.parse().ok()).unwrap_or(0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_backend_index_custom() {
        let a = make_annotations(Some("true"), None, None, Some("2"));
        let idx: usize = a.get(BACKEND_KEY).and_then(|v| v.parse().ok()).unwrap_or(0);
        assert_eq!(idx, 2);
    }
}
