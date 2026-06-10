use crate::{Tool, ToolContext};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, Url, redirect::Policy};
use serde_json::{Value, json};
use sled_core::ToolResult;
use std::net::IpAddr;
use std::time::Duration;

const DEFAULT_MAX_BYTES: u64 = 200_000;
const HARD_MAX_BYTES: u64 = 1_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const HARD_TIMEOUT_MS: u64 = 30_000;

pub struct HttpGetTool {
    client: Client,
}

impl Default for HttpGetTool {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .redirect(Policy::none())
                .build()
                .expect("http_get client configuration is valid"),
        }
    }
}

#[async_trait]
impl Tool for HttpGetTool {
    fn name(&self) -> &'static str {
        "http_get"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> Result<ToolResult> {
        let urls = parse_urls(&args);
        let max_bytes = args["max_bytes"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_BYTES)
            .min(HARD_MAX_BYTES);
        let timeout_ms = args["timeout_ms"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(HARD_TIMEOUT_MS);

        let mut sections = Vec::new();
        for raw_url in urls {
            sections.push(
                self.fetch_one(&raw_url, max_bytes, timeout_ms)
                    .await
                    .unwrap_or_else(
                        |err| json!({"url": raw_url, "ok": false, "error": err.to_string()}),
                    ),
            );
        }

        Ok(ToolResult::completed(
            json!({"ok": true, "sections": sections}),
        ))
    }
}

impl HttpGetTool {
    async fn fetch_one(&self, raw_url: &str, max_bytes: u64, timeout_ms: u64) -> Result<Value> {
        let url = validate_url(raw_url)?;
        let response = self
            .client
            .get(url.clone())
            .timeout(Duration::from_millis(timeout_ms))
            .header("user-agent", "sled/0.1")
            .send()
            .await?;

        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();

        if let Some(content_length) = response.content_length() {
            if content_length > max_bytes {
                return Ok(json!({
                    "url": raw_url,
                    "ok": false,
                    "status": status,
                    "final_url": final_url,
                    "content_type": content_type,
                    "error": format!("response too large: {content_length} bytes > {max_bytes} bytes"),
                }));
            }
        }

        let (body, truncated) = read_limited_body(response, max_bytes).await?;

        Ok(json!({
            "url": raw_url,
            "ok": true,
            "status": status,
            "final_url": final_url,
            "content_type": content_type,
            "truncated": truncated,
            "body": body,
        }))
    }
}

async fn read_limited_body(
    mut response: reqwest::Response,
    max_bytes: u64,
) -> Result<(String, bool)> {
    let limit = max_bytes as usize;
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut truncated = false;

    while let Some(chunk) = response.chunk().await? {
        let remaining = limit.saturating_sub(bytes.len());
        if chunk.len() > remaining {
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

fn parse_urls(args: &Value) -> Vec<String> {
    if let Some(url) = args["url"].as_str() {
        return vec![url.to_string()];
    }
    args["urls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|url| url.as_str())
        .map(ToString::to_string)
        .collect()
}

fn validate_url(raw_url: &str) -> Result<Url> {
    let url = Url::parse(raw_url)?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(anyhow!("unsupported URL scheme: {scheme}")),
    }

    let Some(host) = url.host_str() else {
        return Err(anyhow!("URL has no host"));
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return Err(anyhow!("localhost URLs are not allowed"));
    }
    let ip_host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(&host);
    if let Ok(ip) = ip_host.parse::<IpAddr>() {
        validate_ip(ip)?;
    }

    Ok(url)
}

fn validate_ip(ip: IpAddr) -> Result<()> {
    let blocked = match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    };
    if blocked {
        Err(anyhow!(
            "private, local, or reserved IP URLs are not allowed"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_public_http_urls() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://93.184.216.34").is_ok());
    }

    #[test]
    fn rejects_unsupported_schemes() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/file").is_err());
    }

    #[test]
    fn rejects_local_hosts_and_private_ips() {
        assert!(validate_url("http://localhost:8080").is_err());
        assert!(validate_url("http://api.localhost").is_err());
        assert!(validate_url("http://127.0.0.1").is_err());
        assert!(validate_url("http://10.0.0.1").is_err());
        assert!(validate_url("http://192.168.1.1").is_err());
        assert!(validate_url("http://[::1]").is_err());
        assert!(validate_url("http://[fd00::1]").is_err());
    }
}
