//! HTTP client for the optional local semantic-router sidecar ([`SemanticRouterGatewayConfig`]).
//!
//! Only loopback base URLs are accepted to avoid accidental remote calls from config typos.

use serde::Deserialize;

/// Successful classification with a named route and similarity score.
#[derive(Debug, Clone)]
pub struct ClassifyResult {
    pub route: String,
    pub score: f32,
}

#[derive(Debug, Deserialize)]
struct ClassifyResponse {
    route: Option<String>,
    score: Option<f32>,
}

/// Returns true when `base_url` parses as `http`/`https` and the host is loopback.
pub fn base_url_is_loopback(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host_inner = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    host_inner
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// `POST {base_url}/classify` with JSON body `{"text":"..."}`.
pub async fn classify(
    client: &reqwest::Client,
    base_url: &str,
    text: &str,
    timeout: std::time::Duration,
) -> anyhow::Result<Option<ClassifyResult>> {
    if !base_url_is_loopback(base_url) {
        anyhow::bail!(
            "semantic_router.base_url must use loopback host (127.0.0.1, localhost, ::1)"
        );
    }
    let endpoint = format!("{}/classify", base_url.trim_end_matches('/'));
    let resp = client
        .post(endpoint)
        .json(&serde_json::json!({ "text": text }))
        .timeout(timeout)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("semantic router returned HTTP {}", resp.status());
    }
    let body: ClassifyResponse = resp.json().await?;
    Ok(match (body.route, body.score) {
        (Some(route), Some(score)) => Some(ClassifyResult { route, score }),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_loopback_accepted() {
        assert!(base_url_is_loopback("http://127.0.0.1:8099"));
        assert!(base_url_is_loopback("http://127.0.0.1:8099/"));
        assert!(base_url_is_loopback("http://localhost:8099/v1"));
        assert!(base_url_is_loopback("http://[::1]:8099"));
    }

    #[test]
    fn base_url_non_loopback_rejected() {
        assert!(!base_url_is_loopback("http://192.168.1.1:8099"));
        assert!(!base_url_is_loopback("http://example.com"));
        assert!(!base_url_is_loopback("ftp://127.0.0.1:8099"));
        assert!(!base_url_is_loopback("not a url"));
    }
}
