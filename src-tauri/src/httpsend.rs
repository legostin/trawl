//! One-shot HTTP send for the HTTP-client plugin.
//!
//! Sends an arbitrary request via a blocking reqwest client (like the scripting
//! `native_send`) and returns the response. `via_proxy` routes through the local
//! Trawl proxy so the request also shows up in the capture list.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
    /// Base64 raw body; when present it overrides `body` (used for multipart/binary).
    #[serde(default)]
    pub body_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub body_is_text: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Request headers that must not be forwarded verbatim (the client sets them).
fn is_hop_by_hop_req(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host" | "content-length" | "connection" | "transfer-encoding" | "accept-encoding"
    )
}

/// Response headers that break consistency once the body is decoded by reqwest.
fn strip_resp_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-encoding" | "content-length" | "transfer-encoding"
    )
}

fn err_resp(start: Instant, error: String) -> SendResponse {
    SendResponse {
        status: 0,
        headers: vec![],
        body: String::new(),
        body_is_text: true,
        duration_ms: start.elapsed().as_millis() as u64,
        error: Some(error),
    }
}

pub fn send_http(req: &SendRequest, via_proxy: bool) -> SendResponse {
    let start = Instant::now();

    let mut builder = reqwest::blocking::Client::builder()
        .use_rustls_tls()
        .gzip(true)
        .timeout(Duration::from_secs(30));
    if via_proxy {
        match reqwest::Proxy::all("http://127.0.0.1:8729") {
            // The local proxy MITMs TLS with its own CA, which reqwest won't trust.
            Ok(p) => builder = builder.proxy(p).danger_accept_invalid_certs(true),
            Err(e) => return err_resp(start, format!("proxy: {e}")),
        }
    }
    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => return err_resp(start, e.to_string()),
    };

    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut rb = client.request(method, &req.url);
    for (k, v) in &req.headers {
        if k.is_empty() || is_hop_by_hop_req(k) {
            continue;
        }
        rb = rb.header(k, v);
    }
    if let Some(b64) = &req.body_b64 {
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(bytes) => rb = rb.body(bytes),
            Err(e) => return err_resp(start, format!("bad body base64: {e}")),
        }
    } else if !req.body.is_empty() {
        rb = rb.body(req.body.clone());
    }

    match rb.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter(|(k, _)| !strip_resp_header(k.as_str()))
                .map(|(k, v)| {
                    (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).to_string())
                })
                .collect();
            let bytes = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
            let body_is_text = std::str::from_utf8(&bytes).is_ok();
            let body = if body_is_text {
                String::from_utf8_lossy(&bytes).to_string()
            } else {
                format!("<binary {} bytes>", bytes.len())
            };
            SendResponse {
                status,
                headers,
                body,
                body_is_text,
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => err_resp(start, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_hop_by_hop_request_headers() {
        assert!(is_hop_by_hop_req("Host"));
        assert!(is_hop_by_hop_req("content-length"));
        assert!(is_hop_by_hop_req("Accept-Encoding"));
        assert!(!is_hop_by_hop_req("authorization"));
        assert!(!is_hop_by_hop_req("content-type"));
    }

    #[test]
    fn strips_encoding_response_headers() {
        assert!(strip_resp_header("Content-Encoding"));
        assert!(strip_resp_header("content-length"));
        assert!(!strip_resp_header("content-type"));
    }

    #[test]
    fn invalid_url_returns_error_response() {
        let r = send_http(
            &SendRequest {
                method: "GET".into(),
                url: "http://".into(),
                headers: vec![],
                body: String::new(),
                body_b64: None,
            },
            false,
        );
        assert_eq!(r.status, 0);
        assert!(r.error.is_some());
    }
}
