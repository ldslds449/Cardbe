pub mod cache;
mod fetcher;
mod parser;
pub(crate) mod safety;

use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkPreviewStatus {
    Success,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkPreview {
    pub url: String,
    pub final_url: Option<String>,
    pub domain: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub favicon_url: Option<String>,
    pub status: LinkPreviewStatus,
    pub fetched_at: i64,
}

pub fn normalize(value: &str) -> Option<Url> {
    safety::safe_url(value)
}

impl LinkPreview {
    pub fn unavailable(url: &str) -> Self {
        Self {
            url: url.into(),
            final_url: None,
            domain: Url::parse(url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_default(),
            title: None,
            description: None,
            image_url: None,
            favicon_url: normalize(url)
                .and_then(|url| url.join("/favicon.ico").ok())
                .map(|url| url.to_string()),
            status: LinkPreviewStatus::Unavailable,
            fetched_at: chrono::Utc::now().timestamp(),
        }
    }
}

pub async fn preview(url: Url) -> LinkPreview {
    let mut preview = LinkPreview::unavailable(url.as_str());
    if let Ok(Some((final_url, _, body))) =
        tokio::time::timeout(fetcher::TIMEOUT, fetcher::fetch(url, false)).await
    {
        preview.final_url = Some(final_url.to_string());
        parser::parse(&String::from_utf8_lossy(&body), &final_url, &mut preview);
    }
    preview
}

pub async fn image(value: &str) -> Option<String> {
    use base64::Engine;
    let url = normalize(value)?;
    let (_, mime, body) = tokio::time::timeout(fetcher::TIMEOUT, fetcher::fetch(url, true))
        .await
        .ok()??;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(body)
    ))
}
