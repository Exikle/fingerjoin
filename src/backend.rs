use crate::error::Error;
use crate::webfinger::{JrdResource, parse_jrd};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use url::Url;

#[derive(Debug, Clone)]
pub struct Backend {
    pub name: String,
    pub url: Url,
    pub priority: u16,
}

pub async fn fetch_jrd(backend: &Backend, resource: &str) -> Result<JrdResource, Error> {
    let url = backend
        .url
        .join(".well-known/webfinger")
        .map_err(Error::Url)?;
    let url = url
        .join(&format!("?resource={resource}"))
        .map_err(Error::Url)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(Error::Request)?;

    let resp = client
        .get(url)
        .header("Accept", "application/jrd+json")
        .send()
        .await
        .map_err(Error::Request)?;

    if resp.status() == 404 {
        return Err(Error::AllBackendsFailed);
    }

    let bytes = resp.bytes().await.map_err(Error::Request)?;
    let jrd = parse_jrd(&bytes)?;

    Ok(jrd)
}

pub async fn fan_out(
    backends: &[Backend],
    resource: &str,
    semaphore: Arc<Semaphore>,
) -> Vec<(u16, JrdResource)> {
    let futures = backends.iter().map(|backend| {
        let backend = backend.clone();
        let resource = resource.to_string();
        let sem = semaphore.clone();

        async move {
            let _guard = sem.acquire().await.ok()?;
            fetch_jrd(&backend, &resource)
                .await
                .ok()
                .map(|jr| (backend.priority, jr))
        }
    });

    futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}
