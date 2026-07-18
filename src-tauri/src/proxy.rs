use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{body::Bytes, header::HeaderMap, Request, Response},
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use tokio::sync::oneshot;

use crate::ca::generate_ephemeral_ca;
use crate::model::{Flow, FlowState, HttpMessage, ResponseMessage, UrlParts};
use crate::store::FlowStore;

pub type EmitFn = Arc<dyn Fn(&str, &Flow) + Send + Sync>;

#[derive(Clone)]
struct CaptureHandler {
    store: FlowStore,
    emit: EmitFn,
    current_id: Option<u64>,
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

impl HttpHandler for CaptureHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
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
        let flow = Flow::new_request(
            id,
            parts.method.to_string(),
            url,
            HttpMessage { headers, body: bytes.clone(), body_is_text: is_text },
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
        if let Some(id) = self.current_id {
            self.store.update(id, |f| {
                f.response = Some(ResponseMessage {
                    status,
                    headers: headers.clone(),
                    body: bytes.clone(),
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

pub async fn start(addr: SocketAddr, store: FlowStore, emit: EmitFn) -> Result<ProxyHandle> {
    let (ca_key, ca_cert) = generate_ephemeral_ca()?;
    let authority = RcgenAuthority::new(ca_key, ca_cert, 1_000);

    // забиндиться заранее, чтобы узнать реальный порт при :0
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let handler = CaptureHandler { store, emit, current_id: None };
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
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

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
        let handle = start(proxy_addr, store.clone(), emit).await.unwrap();
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
    }
}
