use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JrdLink {
    pub rel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub titles: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JrdResource {
    #[serde(rename = "subject")]
    pub subject: Option<String>,
    #[serde(rename = "aliases")]
    pub aliases: Vec<String>,
    #[serde(rename = "properties")]
    pub properties: HashMap<String, String>,
    #[serde(rename = "links")]
    pub links: Vec<JrdLink>,
}

impl JrdResource {
    pub fn empty() -> Self {
        Self {
            subject: None,
            aliases: Vec::new(),
            properties: HashMap::new(),
            links: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum Error {
    #[error("invalid JRD format")]
    InvalidFormat,
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn parse_jrd(bytes: &[u8]) -> Result<JrdResource, Error> {
    let resource: JrdResource = serde_json::from_slice(bytes)?;
    Ok(resource)
}

pub fn merge_jrd(responses: Vec<(u16, JrdResource)>) -> JrdResource {
    let mut result = JrdResource::empty();
    let mut seen_links: HashSet<(String, Option<String>)> = HashSet::new();

    for (_, mut resp) in responses {
        if result.subject.is_none() {
            result.subject = resp.subject.take();
        }

        result.aliases.extend(resp.aliases);
        result.properties.extend(resp.properties);

        for link in resp.links {
            let key = (link.rel.clone(), link.href.clone());
            if seen_links.insert(key) {
                result.links.push(link);
            }
        }
    }

    result
}

pub fn to_json_bytes(resource: &JrdResource) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(resource).map_err(Error::Json)
}
