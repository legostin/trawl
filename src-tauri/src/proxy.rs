use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{body::Bytes, header::HeaderMap, Request, Response},
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use tokio::sync::oneshot;

use crate::ca::load_or_create_ca;
use crate::model::{Flow, FlowState, HttpMessage, ResponseMessage, UrlParts};
use crate::store::FlowStore;

pub type EmitFn = Arc<dyn Fn(&str, &Flow) + Send + Sync>;

#[derive(Clone)]
struct CaptureHandler {
    store: FlowStore,
    emit: EmitFn,
    current_id: Option<u64>,
    ca_pem: String,
}

fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                String::from_utf8_lossy(v.as_bytes()).to_string(),
            )
        })
        .collect()
}

fn looks_textual(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("content-type")
            && (v.contains("text")
                || v.contains("json")
                || v.contains("xml")
                || v.contains("form-urlencoded"))
    })
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Распаковывает тело по Content-Encoding для отображения. При любой ошибке
/// возвращает исходные байты (лучше показать сырое, чем потерять данные).
fn decode_body(raw: &[u8], encoding: Option<&str>) -> Vec<u8> {
    use std::io::Read;
    let enc = match encoding {
        Some(e) => e.trim().to_ascii_lowercase(),
        None => return raw.to_vec(),
    };
    if enc.contains("gzip") || enc.contains("x-gzip") {
        let mut out = Vec::new();
        let mut dec = flate2::read::MultiGzDecoder::new(raw);
        if dec.read_to_end(&mut out).is_ok() {
            return out;
        }
    } else if enc.contains("br") {
        let mut out = Vec::new();
        let mut dec = brotli::Decompressor::new(raw, 4096);
        if dec.read_to_end(&mut out).is_ok() {
            return out;
        }
    } else if enc.contains("deflate") {
        let mut out = Vec::new();
        let mut dec = flate2::read::ZlibDecoder::new(raw);
        if dec.read_to_end(&mut out).is_ok() {
            return out;
        }
        // deflate иногда без zlib-обёртки (raw DEFLATE)
        let mut out2 = Vec::new();
        let mut dec2 = flate2::read::DeflateDecoder::new(raw);
        if dec2.read_to_end(&mut out2).is_ok() {
            return out2;
        }
    }
    raw.to_vec()
}

impl HttpHandler for CaptureHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        // Раздача CA-сертификата: клиент с настроенным прокси открывает http://http-catch/
        if req.uri().host() == Some("http-catch") {
            let body = Body::from(Full::new(Bytes::from(self.ca_pem.clone().into_bytes())));
            let resp = Response::builder()
                .status(200)
                .header("content-type", "application/x-x509-ca-cert")
                .header(
                    "content-disposition",
                    "attachment; filename=\"http-catch-ca.pem\"",
                )
                .body(body)
                .expect("build cert response");
            return RequestOrResponse::Response(resp);
        }

        let (parts, body) = req.into_parts();
        let bytes = match body.collect().await {
            Ok(c) => c.to_bytes().to_vec(),
            Err(_) => Vec::new(),
        };
        let headers = headers_to_vec(&parts.headers);
        let uri = &parts.uri;
        let url = UrlParts {
            scheme: uri.scheme_str().unwrap_or("http").to_string(),
            host: uri.host().unwrap_or_default().to_string(),
            port: uri.port_u16().unwrap_or(80),
            path: uri
                .path_and_query()
                .map(|p| p.to_string())
                .unwrap_or_else(|| "/".into()),
        };
        let id = self.store.next_id();
        let is_text = looks_textual(&headers);
        let display_body = decode_body(&bytes, header_value(&headers, "content-encoding"));
        let flow = Flow::new_request(
            id,
            parts.method.to_string(),
            url,
            HttpMessage { headers, body: display_body, body_is_text: is_text },
        );
        self.store.insert(flow.clone());
        (self.emit)("flow-added", &flow);
        self.current_id = Some(id);

        let rebuilt = Request::from_parts(parts, Body::from(Full::new(Bytes::from(bytes))));
        rebuilt.into()
    }

    async fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        res: Response<Body>,
    ) -> Response<Body> {
        let (parts, body) = res.into_parts();
        let bytes = match body.collect().await {
            Ok(c) => c.to_bytes().to_vec(),
            Err(_) => Vec::new(),
        };
        let headers = headers_to_vec(&parts.headers);
        let is_text = looks_textual(&headers);
        let status = parts.status.as_u16();
        // Для отображения храним распакованное тело; клиенту ниже уходят исходные байты.
        let display_body = decode_body(&bytes, header_value(&headers, "content-encoding"));
        if let Some(id) = self.current_id {
            self.store.update(id, |f| {
                f.response = Some(ResponseMessage {
                    status,
                    headers: headers.clone(),
                    body: display_body.clone(),
                    body_is_text: is_text,
                });
                f.state = FlowState::Completed;
            });
            if let Some(updated) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-updated", &updated);
            }
        }
        Response::from_parts(parts, Body::from(Full::new(Bytes::from(bytes))))
    }
}

pub struct ProxyHandle {
    shutdown: Option<oneshot::Sender<()>>,
    addr: SocketAddr,
}

impl ProxyHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

pub async fn start(
    addr: SocketAddr,
    store: FlowStore,
    emit: EmitFn,
    ca_dir: PathBuf,
) -> Result<ProxyHandle> {
    let ca = load_or_create_ca(&ca_dir)?;
    let authority = RcgenAuthority::new(ca.key_pair, ca.ca_cert, 1_000);

    // забиндиться заранее, чтобы узнать реальный порт при :0
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let handler = CaptureHandler {
        store,
        emit,
        current_id: None,
        ca_pem: ca.cert_pem,
    };
    let (tx, rx) = oneshot::channel::<()>();

    let proxy = Proxy::builder()
        .with_listener(listener)
        .with_rustls_client()
        .with_ca(authority)
        .with_http_handler(handler)
        .with_graceful_shutdown(async move {
            let _ = rx.await;
        })
        .build();

    tokio::spawn(async move {
        let _ = proxy.start().await;
    });

    Ok(ProxyHandle { shutdown: Some(tx), addr: bound })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FlowStore;
    use std::io::Write;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    #[test]
    fn decode_body_gunzips_gzip_content() {
        let original = b"{\"hello\":\"world\"}";
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(original).unwrap();
        let gz = enc.finish().unwrap();
        assert_ne!(gz, original, "sanity: gzip-байты отличаются от исходных");

        let decoded = decode_body(&gz, Some("gzip"));
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_body_passes_through_when_no_encoding() {
        let raw = b"plain text";
        assert_eq!(decode_body(raw, None), raw);
    }

    #[test]
    fn decode_body_returns_raw_on_bad_gzip() {
        let not_gzip = b"not actually gzipped";
        assert_eq!(decode_body(not_gzip, Some("gzip")), not_gzip);
    }

    // Поднимает простой upstream HTTP-сервер, гоняет запрос через прокси,
    // проверяет, что Flow собрался со статусом ответа.
    #[tokio::test]
    async fn captures_http_flow_through_proxy() {
        // 1. upstream: отвечает 200 "hello"
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });

        // 2. прокси
        let store = FlowStore::new(100);
        let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(vec![]));
        let seen2 = seen.clone();
        let emit: EmitFn = Arc::new(move |_ev, flow| {
            seen2.lock().unwrap().push(flow.id);
        });
        let proxy_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let ca_dir =
            std::env::temp_dir().join(format!("httpcatch-proxy-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start(proxy_addr, store.clone(), emit, ca_dir.clone())
            .await
            .unwrap();
        let bound = handle.local_addr();

        // 3. запрос через прокси на upstream
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let url = format!("http://{}/ping", upstream_addr);
        let body = client.get(&url).send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "hello");

        // 4. Flow собран
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let flows = store.all();
        assert_eq!(flows.len(), 1);
        assert_eq!(flows[0].response.as_ref().unwrap().status, 200);
        assert!(!seen.lock().unwrap().is_empty());

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    #[tokio::test]
    async fn serves_ca_pem_on_magic_host() {
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-cert-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, ca_dir.clone())
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client.get("http://http-catch/").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let text = resp.text().await.unwrap();
        assert!(text.contains("BEGIN CERTIFICATE"), "got: {text}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Локальный upstream отдаёт gzip-тело; проверяем, что в сторе оно распаковано.
    #[tokio::test]
    async fn decompresses_gzip_response_body() {
        let payload = b"{\"gzipped\": true, \"msg\": \"hello\"}";
        let mut enc =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(payload).unwrap();
        let gz = enc.finish().unwrap();

        // upstream: HTTP/1.1 200 с Content-Encoding: gzip и сжатым телом
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let gz_for_task = gz.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                let gz = gz_for_task.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                        gz.len()
                    );
                    let mut out = header.into_bytes();
                    out.extend_from_slice(&gz);
                    let _ = sock.write_all(&out).await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-gz-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, ca_dir.clone())
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client
            .get(format!("http://{upstream_addr}/gzip"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let flows = store.all();
        let flow = flows.iter().find(|f| f.url.path.contains("/gzip")).unwrap();
        let body = flow.response.as_ref().unwrap().body.clone();
        assert_eq!(body, payload, "тело ответа должно быть распаковано в сторе");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Сквозной HTTPS-smoke: ходит на реальный сайт через прокси, доверяя нашему CA,
    // и проверяет, что поток расшифрован (scheme == https, статус 200).
    // Помечен #[ignore], т.к. требует сети; запускать вручную: cargo test -- --ignored
    #[tokio::test]
    #[ignore = "network: hits a real https site to verify MITM decryption"]
    async fn decrypts_https_through_proxy() {
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-https-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, ca_dir.clone())
            .await
            .unwrap();
        let bound = handle.local_addr();

        let ca_pem = std::fs::read(ca_dir.join("ca.pem")).unwrap();
        let cert = reqwest::Certificate::from_pem(&ca_pem).unwrap();
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{bound}")).unwrap())
            .add_root_certificate(cert)
            .build()
            .unwrap();

        let resp = client.get("https://example.com/").send().await.unwrap();
        assert_eq!(resp.status(), 200);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let flows = store.all();
        assert!(
            flows.iter().any(|f| f.url.scheme == "https"),
            "ожидали расшифрованный https-поток, потоки: {:?}",
            flows.iter().map(|f| &f.url.scheme).collect::<Vec<_>>()
        );

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }
}
