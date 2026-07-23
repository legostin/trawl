use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use http_body_util::{BodyExt, Full};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{
        body::Bytes,
        header::{HeaderMap, HeaderName, HeaderValue},
        Request, Response,
    },
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::ca::load_or_create_ca;
use crate::db::DbHandle;
use crate::model::{Flow, FlowState, HttpMessage, ResponseMessage, UrlParts};
use crate::projects::{
    env_from_object, merged_env_object, split_env_writeback, update_global_env,
    update_project_env, Project,
};
use crate::rules::{glob_match_env, Phase, Rule};
use crate::scripting::ScriptClient;
use crate::store::FlowStore;

pub type EmitFn = Arc<dyn Fn(&str, &Flow) + Send + Sync>;
/// Delivers a named app event payload to the app (e.g. Tauri events
/// `script-notify`, `rule-applied`, `rule-error`).
pub type AppEventFn = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
pub type SharedRules = Arc<RwLock<Vec<Rule>>>;
pub type SharedLibrary = Arc<RwLock<String>>;
pub type SharedProject = Arc<RwLock<Option<Project>>>;
pub type SharedGlobalEnv = Arc<RwLock<Vec<crate::projects::EnvVar>>>;
pub type SharedBreakpoints = Arc<RwLock<Vec<crate::breakpoints::Breakpoint>>>;
pub type SharedIntercept = Arc<RwLock<bool>>;
/// Auto-continue timeout in seconds (0 = hold forever).
pub type SharedTimeout = Arc<RwLock<u64>>;
/// Hold new requests while any flow is paused on a breakpoint.
pub type SharedPauseOthers = Arc<RwLock<bool>>;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BpPhase {
    Request,
    Response,
}

pub enum Resolution {
    Execute {
        method: Option<String>,
        /// Edited request path+query (request phase only); None = unchanged.
        path: Option<String>,
        status: Option<u16>,
        headers: Vec<(String, String)>,
        body: String,
        /// Raw body bytes (e.g. an uploaded file); overrides `body` when Some.
        body_bytes: Option<Vec<u8>>,
    },
    Abort(String),
    Respond {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
        body_bytes: Option<Vec<u8>>,
    },
}

pub type BreakpointRegistry = Arc<Mutex<HashMap<(u64, BpPhase), oneshot::Sender<Resolution>>>>;

#[derive(Clone)]
struct CaptureHandler {
    store: FlowStore,
    emit: EmitFn,
    app_event: AppEventFn,
    secret_fn: crate::scripting::SecretFn,
    current_id: Option<u64>,
    ca_pem: String,
    started: std::time::Instant,
    scripts: ScriptClient,
    rules: SharedRules,
    library: SharedLibrary,
    active_project: SharedProject,
    global_env: SharedGlobalEnv,
    data_dir: PathBuf,
    db: Option<DbHandle>,
    breakpoints: SharedBreakpoints,
    intercept: SharedIntercept,
    pending: BreakpointRegistry,
    timeout_secs: SharedTimeout,
    pause_others: SharedPauseOthers,
}

impl CaptureHandler {
    /// Persist a captured/updated flow to the shared DB (no-op if DB is absent).
    fn persist(&self, flow: &Flow) {
        if let Some(db) = &self.db {
            db.record(flow, self.active_scope().as_deref());
        }
    }

    /// Forward notify() calls collected by a rule script to the app.
    fn emit_notifications(&self, res: &crate::scripting::ScriptResult, rule_name: &str, flow_id: u64) {
        for n in &res.notifications {
            let mut p = serde_json::Map::new();
            p.insert("text".into(), n.get("text").cloned().unwrap_or_else(|| "".into()));
            for k in ["channel", "title"] {
                if let Some(v) = n.get(k).filter(|v| !v.is_null()) {
                    p.insert(k.into(), v.clone());
                }
            }
            p.insert("source".into(), "rule".into());
            p.insert("ruleName".into(), rule_name.into());
            p.insert("flowId".into(), flow_id.into());
            (self.app_event)("script-notify", serde_json::Value::Object(p));
        }
    }

    /// Report a rule outcome ("rule-applied" / "rule-error") to the app.
    fn emit_rule_event(
        &self,
        event: &str,
        rule_name: &str,
        phase: &str,
        flow_id: u64,
        method: &str,
        host: &str,
        path: &str,
        error: Option<&str>,
    ) {
        let mut p = serde_json::Map::new();
        p.insert("ruleName".into(), rule_name.into());
        p.insert("phase".into(), phase.into());
        p.insert("flowId".into(), flow_id.into());
        p.insert("method".into(), method.into());
        p.insert("host".into(), host.into());
        p.insert("path".into(), path.into());
        if let Some(e) = error {
            p.insert("error".into(), e.into());
        }
        (self.app_event)(event, serde_json::Value::Object(p));
    }
}

fn headers_to_json(headers: &[(String, String)]) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers {
        map.insert(k.clone(), Value::String(v.clone()));
    }
    Value::Object(map)
}

fn json_to_headers(v: &Value) -> Vec<(String, String)> {
    match v.as_object() {
        Some(obj) => obj
            .iter()
            .map(|(k, val)| {
                let s = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), s)
            })
            .collect(),
        None => vec![],
    }
}

/// Собирает HeaderMap из пар, выставляя корректный content-length и убирая
/// конфликтующие с новым телом заголовки. При `strip_encoding` также убирает
/// content-encoding (когда тело уже распаковано — иначе клиент не декодирует).
fn build_header_map(headers: &[(String, String)], body_len: usize, strip_encoding: bool) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        let lk = k.to_ascii_lowercase();
        if lk == "content-length" || lk == "transfer-encoding" {
            continue;
        }
        if strip_encoding && lk == "content-encoding" {
            continue;
        }
        if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v))
        {
            map.append(name, val);
        }
    }
    if let Ok(val) = HeaderValue::from_str(&body_len.to_string()) {
        map.insert(hudsucker::hyper::header::CONTENT_LENGTH, val);
    }
    map
}

fn text_of(body: &[u8], is_text: bool) -> String {
    if is_text {
        String::from_utf8_lossy(body).to_string()
    } else {
        String::new()
    }
}

/// Rebuilds a request URI with a new path+query, preserving scheme+authority
/// (absolute-form for MITM'd requests). Falls back to origin-form otherwise.
fn rebuild_uri(orig: &hudsucker::hyper::Uri, path: &str) -> Option<hudsucker::hyper::Uri> {
    match (orig.scheme_str(), orig.authority()) {
        (Some(scheme), Some(auth)) => format!("{scheme}://{auth}{path}").parse().ok(),
        _ => path.parse().ok(),
    }
}

enum Directive {
    Continue,
    Mock(Value),
    Abort(String),
    Breakpoint,
}

fn build_mock_response(spec: &Value) -> Response<Body> {
    let status = spec.get("status").and_then(|s| s.as_u64()).unwrap_or(200) as u16;
    let body = spec
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string()
        .into_bytes();
    let mut builder = Response::builder().status(status);
    let mut has_ct = false;
    if let Some(h) = spec.get("headers").and_then(|h| h.as_object()) {
        for (k, v) in h {
            let lk = k.to_ascii_lowercase();
            // тело мока/handler'а — уже готовые байты; не тащим кодировку/длину
            if matches!(lk.as_str(), "content-encoding" | "content-length" | "transfer-encoding") {
                continue;
            }
            if let Some(vs) = v.as_str() {
                if lk == "content-type" {
                    has_ct = true;
                }
                builder = builder.header(k.as_str(), vs);
            }
        }
    }
    if !has_ct {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(Body::from(Full::new(Bytes::from(body))))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// Build a response from raw bytes (a substituted/uploaded body). Strips
/// content-encoding/length (the bytes are final) and sets a correct length.
fn build_bytes_response(status: u16, headers: &[(String, String)], body: Vec<u8>) -> Response<Body> {
    let map = build_header_map(headers, body.len(), true);
    let mut resp = Response::new(Body::from(Full::new(Bytes::from(body))));
    *resp.status_mut() =
        hudsucker::hyper::StatusCode::from_u16(status).unwrap_or(hudsucker::hyper::StatusCode::OK);
    *resp.headers_mut() = map;
    resp
}

fn build_abort_response(reason: &str) -> Response<Body> {
    Response::builder()
        .status(502)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(Full::new(Bytes::from(
            format!("trawl aborted: {reason}").into_bytes(),
        ))))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

impl CaptureHandler {
    /// Область правил: активный проект → только его правила; иначе — глобальные.
    fn active_scope(&self) -> Option<String> {
        self.active_project.read().unwrap().as_ref().map(|p| p.id.clone())
    }

    /// Эффективный env (global + активный проект, проект побеждает) как JSON-объект.
    fn active_env(&self) -> Value {
        let global = self.global_env.read().unwrap();
        let guard = self.active_project.read().unwrap();
        merged_env_object(&global, guard.as_ref())
    }

    /// Записывает изменённый скриптом env: при активном проекте — в проект
    /// (изменённый глобальный ключ становится проектным перекрытием),
    /// без проекта — в глобальный env.
    fn apply_env(&self, new_env: &Value) {
        if *new_env == self.active_env() {
            return; // без изменений — не пишем
        }
        let global = self.global_env.read().unwrap().clone();
        let mut guard = self.active_project.write().unwrap();
        if let Some(proj) = guard.as_ref() {
            let env = split_env_writeback(new_env, &proj.env, &global);
            let id = proj.id.clone();
            let mut updated = proj.clone();
            updated.env = env.clone();
            *guard = Some(updated);
            drop(guard);
            let _ = update_project_env(&self.data_dir, &id, env);
        } else {
            drop(guard);
            let env = env_from_object(new_env);
            *self.global_env.write().unwrap() = env.clone();
            let _ = update_global_env(&self.data_dir, env);
        }
    }

    /// Правило совпадает, если его паттерн матчит любой из кандидатов
    /// (`host/path` и `host:port/path`) и оно в области активного проекта.
    fn matching(&self, phase: Phase, targets: &[String]) -> Vec<Rule> {
        let scope = self.active_scope();
        let env = self.active_env();
        self.rules
            .read()
            .unwrap()
            .iter()
            .filter(|r| {
                r.enabled
                    && r.project_id == scope
                    && r.runs_in(phase)
                    && targets.iter().any(|t| glob_match_env(&r.pattern, t, &env))
            })
            .cloned()
            .collect()
    }

    /// Does any enabled, in-scope breakpoint match this flow in `phase`?
    fn breakpoint_matches(&self, phase: BpPhase, targets: &[String], method: &str) -> bool {
        if !*self.intercept.read().unwrap() {
            return false;
        }
        let scope = self.active_scope();
        let env = self.active_env();
        self.breakpoints.read().unwrap().iter().any(|b| {
            b.enabled
                && b.project_id == scope
                && match phase {
                    BpPhase::Request => b.on_request,
                    BpPhase::Response => b.on_response,
                }
                && b.method
                    .as_deref()
                    .map_or(true, |m| m == "*" || m.eq_ignore_ascii_case(method))
                && targets.iter().any(|t| glob_match_env(&b.pattern, t, &env))
        })
    }

    /// Register a pending breakpoint and await the UI's resolution. Returns None
    /// (continue unmodified) if the sender was dropped (proxy stopped) or the
    /// auto-continue timeout elapsed.
    async fn await_resolution(&self, id: u64, phase: BpPhase) -> Option<Resolution> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert((id, phase), tx);
        let secs = *self.timeout_secs.read().unwrap();
        if secs == 0 {
            return rx.await.ok();
        }
        match tokio::time::timeout(std::time::Duration::from_secs(secs), rx).await {
            Ok(r) => r.ok(),
            Err(_) => {
                // Timed out — drop the pending sender and un-pause so the UI closes.
                self.pending.lock().unwrap().remove(&(id, phase));
                self.store.update(id, |f| {
                    if f.paused_phase.is_some() {
                        f.state = FlowState::Pending;
                        f.paused_phase = None;
                    }
                });
                if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                    (self.emit)("flow-updated", &u);
                    (self.emit)("breakpoint-timeout", &u);
                }
                None
            }
        }
    }

    /// While "pause others" is on, hold a new request until no flow is paused.
    async fn hold_while_paused(&self) {
        if !*self.pause_others.read().unwrap() {
            return;
        }
        while !self.pending.lock().unwrap().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Store a flow, updating it in place if a request breakpoint already inserted
    /// it (so a paused-then-resumed flow keeps its single row), else inserting new.
    fn upsert_flow(&self, preinserted: bool, flow: &Flow) {
        if preinserted {
            self.store.update(flow.id, |x| *x = flow.clone());
        } else {
            self.store.insert(flow.clone());
        }
    }

    /// If a response breakpoint matches, pause on this response for live editing
    /// and return the (possibly edited) response. Used for synthetic responses
    /// produced inside `handle_request` (handler rules) that never reach
    /// `handle_response`. The flow MUST already be in the store. Ok = send it,
    /// Err(reason) = abort. Response phase only, so a String body is fine.
    /// Returns the resolved (status, headers, body) or an abort reason, plus
    /// whether the pause was resolved by an explicit Resolution (Execute/
    /// Respond/Abort) — as opposed to a dropped sender or auto-continue
    /// timeout. The caller uses that flag to emit "flow-resumed" only once
    /// the final resolved response has been written back to the store.
    async fn break_response(
        &self,
        id: u64,
        targets: &[String],
        method: &str,
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    ) -> (Result<(u16, Vec<(String, String)>, Vec<u8>), String>, bool) {
        if !self.breakpoint_matches(BpPhase::Response, targets, method) {
            return (Ok((status, headers, body.into_bytes())), false);
        }
        self.store.update(id, |f| {
            f.state = FlowState::Paused;
            f.paused_phase = Some("response".into());
            f.response = Some(ResponseMessage {
                status,
                headers: headers.clone(),
                body: body.clone().into_bytes(),
                body_is_text: true,
            });
        });
        if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
            (self.emit)("flow-paused", &u);
        }
        let resolution = self.await_resolution(id, BpPhase::Response).await;
        let explicit = resolution.is_some();
        let out = match resolution {
            Some(Resolution::Execute { status: st, headers: h, body: b, body_bytes: bb, .. }) => {
                Ok((st.unwrap_or(status), h, bb.unwrap_or_else(|| b.into_bytes())))
            }
            Some(Resolution::Respond { status: st, headers: h, body: b, body_bytes: bb }) => {
                Ok((st, h, bb.unwrap_or_else(|| b.into_bytes())))
            }
            Some(Resolution::Abort(reason)) => Err(reason),
            None => Ok((status, headers, body.into_bytes())),
        };
        self.store.update(id, |f| {
            f.state = FlowState::Pending;
            f.paused_phase = None;
        });
        (out, explicit)
    }

    fn matching_handler(&self, targets: &[String]) -> Option<Rule> {
        let scope = self.active_scope();
        let env = self.active_env();
        self.rules
            .read()
            .unwrap()
            .iter()
            .find(|r| {
                r.enabled
                    && r.project_id == scope
                    && r.phase == Phase::Handler
                    && targets.iter().any(|t| glob_match_env(&r.pattern, t, &env))
            })
            .cloned()
    }

    fn record_mock_response(&self, id: u64, spec: &Value) {
        let status = spec.get("status").and_then(|s| s.as_u64()).unwrap_or(200) as u16;
        let headers = spec.get("headers").map(json_to_headers).unwrap_or_default();
        let body = spec
            .get("body")
            .and_then(|b| b.as_str())
            .unwrap_or("")
            .as_bytes()
            .to_vec();
        let done = self.started.elapsed().as_millis() as u64;
        self.store.update(id, |f| {
            f.response = Some(ResponseMessage { status, headers: headers.clone(), body: body.clone(), body_is_text: true });
            f.timings.done = Some(done);
            f.state = FlowState::Completed;
        });
        if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
            (self.emit)("flow-updated", &u);
            self.persist(&u);
        }
    }
}

fn unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        // CONNECT — это установка HTTPS-туннеля, а не реальный запрос: пропускаем
        // без захвата и без правил, иначе handler-правило (напр. */*) перехватит
        // CONNECT и сломает весь туннель.
        if req.method() == hudsucker::hyper::Method::CONNECT {
            return req.into();
        }

        // Раздача CA-сертификата: клиент с настроенным прокси открывает http://trawl/.
        // Обязательно ДО фильтра активного проекта: "trawl" — не реальный хост,
        // и уйдя в апстрим такой запрос закончится 502.
        if req.uri().host() == Some("trawl") {
            let body = Body::from(Full::new(Bytes::from(self.ca_pem.clone().into_bytes())));
            let resp = Response::builder()
                .status(200)
                .header("content-type", "application/x-x509-ca-cert")
                .header(
                    "content-disposition",
                    "attachment; filename=\"trawl-ca.pem\"",
                )
                .body(body)
                .expect("build cert response");
            return RequestOrResponse::Response(resp);
        }

        // Активный проект: запросы к нетрекаемым хостам проксируем, но не пишем
        // (экономия памяти) — без flow, эмита и правил.
        {
            let active = self.active_project.read().unwrap();
            if let Some(proj) = active.as_ref() {
                let host = req
                    .uri()
                    .host()
                    .map(|h| h.to_string())
                    .or_else(|| {
                        req.headers()
                            .get("host")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.split(':').next().unwrap_or(s).to_string())
                    })
                    .unwrap_or_default();
                if !proj.tracks(&host) {
                    drop(active);
                    return req.into();
                }
            }
        }

        let (parts, body) = req.into_parts();
        let bytes = match body.collect().await {
            Ok(c) => c.to_bytes().to_vec(),
            Err(_) => Vec::new(),
        };
        let orig_headers = headers_to_vec(&parts.headers);
        let mut full_url = parts.uri.to_string();
        let uri = &parts.uri;
        let mut url = UrlParts {
            scheme: uri.scheme_str().unwrap_or("http").to_string(),
            host: uri.host().unwrap_or_default().to_string(),
            port: uri.port_u16().unwrap_or(80),
            path: uri
                .path_and_query()
                .map(|p| p.to_string())
                .unwrap_or_else(|| "/".into()),
        };
        let id = self.store.next_id();
        let is_text = looks_textual(&orig_headers);
        let display_body = decode_body(&bytes, header_value(&orig_headers, "content-encoding"));

        let targets = vec![
            format!("{}{}", url.host, url.path),
            format!("{}:{}{}", url.host, url.port, url.path),
        ];

        // "Pause others": if enabled, hold this new request while another flow is
        // paused on a breakpoint, so nothing slips through mid-interception.
        self.hold_while_paused().await;

        // Working request values. A definition-based request breakpoint (below)
        // may edit these BEFORE any rule runs; the handler rule and request-phase
        // rules then see the edited request. `preinserted` tracks that the flow
        // row already exists (created while paused) so we update instead of dup.
        let mut req_method = parts.method.to_string();
        let mut req_headers = orig_headers.clone();
        let mut req_body_text = text_of(&display_body, is_text);
        let mut edited_path: Option<String> = None;
        let mut preinserted = false;

        // ── БРЕЙКПОИНТ ФАЗЫ ЗАПРОСА: срабатывает до всех правил ──
        if self.breakpoint_matches(BpPhase::Request, &targets, &req_method) {
            let mut flow = Flow::new_request(
                id,
                req_method.clone(),
                url.clone(),
                HttpMessage {
                    headers: req_headers.clone(),
                    body: if is_text { req_body_text.clone().into_bytes() } else { display_body.clone() },
                    body_is_text: is_text,
                },
            );
            flow.timestamp = unix_ms();
            flow.timings.sent = Some(self.started.elapsed().as_millis() as u64);
            flow.state = FlowState::Paused;
            flow.paused_phase = Some("request".into());
            self.store.insert(flow.clone());
            self.current_id = Some(id);
            preinserted = true;
            (self.emit)("flow-added", &flow);
            (self.emit)("flow-paused", &flow);

            match self.await_resolution(id, BpPhase::Request).await {
                Some(Resolution::Execute { method, path, headers, body, .. }) => {
                    if let Some(m) = method {
                        req_method = m;
                    }
                    if let Some(p) = path {
                        url.path = p.clone();
                        full_url = rebuild_uri(&parts.uri, &p)
                            .map(|u| u.to_string())
                            .unwrap_or(full_url);
                        edited_path = Some(p);
                    }
                    req_headers = headers;
                    if is_text {
                        req_body_text = body;
                    }
                    self.store.update(id, |f| {
                        f.state = FlowState::Pending;
                        f.paused_phase = None;
                        f.method = req_method.clone();
                        f.url.path = url.path.clone();
                        f.request.headers = req_headers.clone();
                        if is_text {
                            f.request.body = req_body_text.clone().into_bytes();
                        }
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        (self.emit)("flow-resumed", &u);
                    }
                }
                Some(Resolution::Abort(reason)) => {
                    self.store.update(id, |f| {
                        f.state = FlowState::Error;
                        f.paused_phase = None;
                        f.error = Some(reason.clone());
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        (self.emit)("flow-resumed", &u);
                        self.persist(&u);
                    }
                    return RequestOrResponse::Response(build_abort_response(&reason));
                }
                Some(Resolution::Respond { status, headers, body, body_bytes }) => {
                    let bytes = body_bytes.unwrap_or_else(|| body.into_bytes());
                    let is_text = looks_textual(&headers);
                    let done = self.started.elapsed().as_millis() as u64;
                    self.store.update(id, |f| {
                        f.response = Some(ResponseMessage {
                            status,
                            headers: headers.clone(),
                            body: bytes.clone(),
                            body_is_text: is_text,
                        });
                        f.timings.done = Some(done);
                        f.state = FlowState::Completed;
                        f.paused_phase = None;
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        (self.emit)("flow-resumed", &u);
                        self.persist(&u);
                    }
                    return RequestOrResponse::Response(build_bytes_response(status, &headers, bytes));
                }
                None => {
                    self.store.update(id, |f| {
                        f.state = FlowState::Pending;
                        f.paused_phase = None;
                    });
                }
            }
        }

        // ── handler-режим: скрипт сам выполняет запрос (send) и возвращает ответ ──
        if let Some(hrule) = self.matching_handler(&targets) {
            let input = json!({
                "request": {
                    "method": req_method,
                    "url": full_url,
                    "host": url.host,
                    "path": url.path,
                    "headers": headers_to_json(&req_headers),
                    "body": req_body_text,
                },
                "env": self.active_env(),
            })
            .to_string();
            let prelude = self.library.read().unwrap().clone();
            let script = hrule.script.clone();
            let secret_fn = self.secret_fn.clone();
            let res = tokio::task::spawn_blocking(move || {
                crate::scripting::execute_handler(
                    &prelude,
                    &script,
                    &input,
                    std::time::Duration::from_secs(30),
                    secret_fn,
                )
            })
            .await
            .unwrap_or_else(|_| crate::scripting::ScriptResult::error("handler panicked"));
            self.emit_notifications(&res, &hrule.name, id);
            if res.action == "respond" {
                self.emit_rule_event(
                    "rule-applied", &hrule.name, "handler", id, &req_method, &url.host, &url.path, None,
                );
            } else {
                self.emit_rule_event(
                    "rule-error", &hrule.name, "handler", id, &req_method, &url.host, &url.path,
                    Some(res.error.as_deref().unwrap_or("unknown error")),
                );
            }
            if let Some(e) = &res.env {
                self.apply_env(e);
            }

            let mut flow = Flow::new_request(
                id,
                req_method.clone(),
                url.clone(),
                HttpMessage {
                    headers: req_headers.clone(),
                    body: if is_text { req_body_text.clone().into_bytes() } else { display_body.clone() },
                    body_is_text: is_text,
                },
            );
            flow.timestamp = unix_ms();
            flow.timings.sent = Some(self.started.elapsed().as_millis() as u64);
            flow.applied_rules = vec![hrule.name.clone()];
            self.current_id = Some(id);

            if res.action == "respond" {
                if let Some(spec) = res.response {
                    let status = spec.get("status").and_then(|s| s.as_u64()).unwrap_or(200) as u16;
                    let headers = spec.get("headers").map(json_to_headers).unwrap_or_default();
                    let body_str = spec.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
                    // Put the flow in the store first so a response breakpoint can pause it.
                    flow.response = Some(ResponseMessage {
                        status,
                        headers: headers.clone(),
                        body: body_str.clone().into_bytes(),
                        body_is_text: true,
                    });
                    flow.timings.done = Some(self.started.elapsed().as_millis() as u64);
                    flow.state = FlowState::Completed;
                    self.upsert_flow(preinserted, &flow);
                    (self.emit)("flow-added", &flow);

                    // Response breakpoint fires on the handler's synthetic response too.
                    match self.break_response(id, &targets, &req_method, status, headers, body_str).await {
                        (Ok((status, headers, body_bytes)), explicit) => {
                            let is_text = looks_textual(&headers);
                            self.store.update(id, |f| {
                                f.response = Some(ResponseMessage {
                                    status,
                                    headers: headers.clone(),
                                    body: body_bytes.clone(),
                                    body_is_text: is_text,
                                });
                                f.state = FlowState::Completed;
                            });
                            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                                (self.emit)("flow-updated", &u);
                                if explicit {
                                    (self.emit)("flow-resumed", &u);
                                }
                                self.persist(&u);
                            }
                            return RequestOrResponse::Response(build_bytes_response(status, &headers, body_bytes));
                        }
                        (Err(reason), explicit) => {
                            self.store.update(id, |f| {
                                f.state = FlowState::Error;
                                f.error = Some(reason.clone());
                            });
                            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                                (self.emit)("flow-updated", &u);
                                if explicit {
                                    (self.emit)("flow-resumed", &u);
                                }
                                self.persist(&u);
                            }
                            return RequestOrResponse::Response(build_abort_response(&reason));
                        }
                    }
                }
            }
            // ошибка handler
            flow.state = FlowState::Error;
            flow.error = Some(res.error.unwrap_or_else(|| "handler error".into()));
            self.upsert_flow(preinserted, &flow);
            (self.emit)("flow-added", &flow);
            self.persist(&flow);
            return RequestOrResponse::Response(build_abort_response(
                flow.error.as_deref().unwrap_or("handler error"),
            ));
        }

        // ── скрипты фазы запроса (цепочка правил над, возможно, отредактированным запросом) ──
        let rules = self.matching(Phase::Request, &targets);
        let mut work_headers = req_headers.clone();
        let mut work_body = req_body_text.clone();
        let mut applied: Vec<String> = Vec::new();
        let mut directive = Directive::Continue;
        let mut script_error: Option<String> = None;
        let mut env = self.active_env();
        if !rules.is_empty() {
            let prelude = self.library.read().unwrap().clone();
            for rule in &rules {
                let input = json!({
                    "request": {
                        "method": req_method.clone(),
                        "url": full_url,
                        "host": url.host,
                        "path": url.path,
                        "headers": headers_to_json(&work_headers),
                        "body": work_body,
                    },
                    "env": env,
                })
                .to_string();
                let res = self.scripts.run(prelude.clone(), rule.script.clone(), input).await;
                self.emit_notifications(&res, &rule.name, id);
                if let Some(e) = res.env.clone() {
                    env = e;
                }
                match res.action.as_str() {
                    "continue" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "request", id, &req_method, &url.host, &url.path, None,
                        );
                        if let Some(rv) = &res.request {
                            if let Some(h) = rv.get("headers") {
                                work_headers = json_to_headers(h);
                            }
                            if let Some(b) = rv.get("body").and_then(|b| b.as_str()) {
                                work_body = b.to_string();
                            }
                        }
                        applied.push(rule.name.clone());
                    }
                    "mock" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "request", id, &req_method, &url.host, &url.path, None,
                        );
                        if let Some(m) = res.mock {
                            directive = Directive::Mock(m);
                        }
                        applied.push(rule.name.clone());
                        break;
                    }
                    "abort" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "request", id, &req_method, &url.host, &url.path, None,
                        );
                        directive = Directive::Abort(res.reason.unwrap_or_else(|| "aborted".into()));
                        applied.push(rule.name.clone());
                        break;
                    }
                    "breakpoint" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "request", id, &req_method, &url.host, &url.path, None,
                        );
                        if let Some(rv) = &res.request {
                            if let Some(h) = rv.get("headers") {
                                work_headers = json_to_headers(h);
                            }
                            if let Some(b) = rv.get("body").and_then(|b| b.as_str()) {
                                work_body = b.to_string();
                            }
                        }
                        directive = Directive::Breakpoint;
                        applied.push(rule.name.clone());
                        break;
                    }
                    _ => {
                        self.emit_rule_event(
                            "rule-error", &rule.name, "request", id, &req_method, &url.host, &url.path,
                            Some(res.error.as_deref().unwrap_or("unknown error")),
                        );
                        script_error = res.error;
                    }
                }
            }
            self.apply_env(&env);
        }

        let stored_body: Vec<u8> = if is_text { work_body.clone().into_bytes() } else { display_body };
        let mut flow = Flow::new_request(
            id,
            req_method.clone(),
            url,
            HttpMessage { headers: work_headers.clone(), body: stored_body, body_is_text: is_text },
        );
        flow.timestamp = unix_ms();
        flow.timings.sent = Some(self.started.elapsed().as_millis() as u64);
        flow.applied_rules = applied;
        flow.error = script_error;
        self.upsert_flow(preinserted, &flow);
        (self.emit)(if preinserted { "flow-updated" } else { "flow-added" }, &flow);
        self.current_id = Some(id);

        // Pause only on a rule-triggered ctx.breakpoint(); definition-based request
        // breakpoints already paused above, before any rule ran.
        let want_break = matches!(directive, Directive::Breakpoint);
        if !want_break {
            self.persist(&flow);
        } else {
            self.store.update(id, |f| {
                f.state = FlowState::Paused;
                f.paused_phase = Some("request".into());
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-paused", &u);
            }
            let mut explicit_resume = false;
            match self.await_resolution(id, BpPhase::Request).await {
                Some(Resolution::Execute { method, headers, body, .. }) => {
                    if let Some(m) = method {
                        req_method = m;
                    }
                    work_headers = headers;
                    if is_text {
                        work_body = body;
                    }
                    directive = Directive::Continue;
                    explicit_resume = true;
                }
                Some(Resolution::Abort(reason)) => {
                    directive = Directive::Abort(reason);
                    explicit_resume = true;
                }
                Some(Resolution::Respond { status, headers, body, body_bytes }) => {
                    let bytes = body_bytes.unwrap_or_else(|| body.into_bytes());
                    let is_text = looks_textual(&headers);
                    let done = self.started.elapsed().as_millis() as u64;
                    self.store.update(id, |f| {
                        f.response = Some(ResponseMessage {
                            status,
                            headers: headers.clone(),
                            body: bytes.clone(),
                            body_is_text: is_text,
                        });
                        f.timings.done = Some(done);
                        f.state = FlowState::Completed;
                        f.paused_phase = None;
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        (self.emit)("flow-resumed", &u);
                        self.persist(&u);
                    }
                    return RequestOrResponse::Response(build_bytes_response(status, &headers, bytes));
                }
                None => directive = Directive::Continue,
            }
            self.store.update(id, |f| {
                f.state = FlowState::Pending;
                f.paused_phase = None;
                f.method = req_method.clone();
                f.request.headers = work_headers.clone();
                if is_text {
                    f.request.body = work_body.clone().into_bytes();
                }
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-updated", &u);
                if explicit_resume {
                    (self.emit)("flow-resumed", &u);
                }
            }
        }

        let out_body: Vec<u8> = if is_text { work_body.clone().into_bytes() } else { bytes.clone() };

        match directive {
            Directive::Mock(spec) => {
                self.record_mock_response(id, &spec);
                RequestOrResponse::Response(build_mock_response(&spec))
            }
            Directive::Abort(reason) => {
                self.store.update(id, |f| {
                    f.state = FlowState::Error;
                    f.error = Some(reason.clone());
                });
                if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                    (self.emit)("flow-updated", &u);
                    self.persist(&u);
                }
                RequestOrResponse::Response(build_abort_response(&reason))
            }
            Directive::Breakpoint | Directive::Continue => {
                let mut new_parts = parts;
                if let Ok(method) = hudsucker::hyper::Method::from_bytes(req_method.as_bytes()) {
                    new_parts.method = method;
                }
                if let Some(p) = &edited_path {
                    if let Some(u) = rebuild_uri(&new_parts.uri, p) {
                        new_parts.uri = u;
                    }
                }
                new_parts.headers = build_header_map(&work_headers, out_body.len(), is_text);
                Request::from_parts(new_parts, Body::from(Full::new(Bytes::from(out_body)))).into()
            }
        }
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
        let orig_headers = headers_to_vec(&parts.headers);
        let is_text = looks_textual(&orig_headers);
        let status = parts.status.as_u16();
        // Для отображения храним распакованное тело; клиенту ниже уходят исходные байты.
        let display_body = decode_body(&bytes, header_value(&orig_headers, "content-encoding"));
        let done_ms = self.started.elapsed().as_millis() as u64;

        let id = match self.current_id {
            Some(id) => id,
            None => return Response::from_parts(parts, Body::from(Full::new(Bytes::from(bytes)))),
        };

        // ── скрипты фазы ответа ──
        let found = self.store.all().into_iter().find(|f| f.id == id);
        let flow_method = found.as_ref().map(|f| f.method.clone()).unwrap_or_default();
        let flow_url = found.map(|f| f.url);
        let (targets, host_str, path_str) = match &flow_url {
            Some(u) => (
                vec![
                    format!("{}{}", u.host, u.path),
                    format!("{}:{}{}", u.host, u.port, u.path),
                ],
                u.host.clone(),
                u.path.clone(),
            ),
            None => (vec![], String::new(), String::new()),
        };
        let rules = self.matching(Phase::Response, &targets);
        let mut work_status = status;
        let mut work_headers = orig_headers.clone();
        let mut work_body = text_of(&display_body, is_text);
        let mut applied: Vec<String> = Vec::new();
        let mut script_error: Option<String> = None;
        let mut rule_break = false;
        let mut env = self.active_env();
        if !rules.is_empty() {
            let prelude = self.library.read().unwrap().clone();
            for rule in &rules {
                let input = json!({
                    "request": { "host": host_str },
                    "response": {
                        "status": work_status,
                        "headers": headers_to_json(&work_headers),
                        "body": work_body,
                    },
                    "env": env,
                })
                .to_string();
                let res = self.scripts.run(prelude.clone(), rule.script.clone(), input).await;
                self.emit_notifications(&res, &rule.name, id);
                if let Some(e) = res.env.clone() {
                    env = e;
                }
                match res.action.as_str() {
                    "continue" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "response", id, &flow_method, &host_str, &path_str, None,
                        );
                        if let Some(rv) = &res.response {
                            if let Some(s) = rv.get("status").and_then(|s| s.as_u64()) {
                                work_status = s as u16;
                            }
                            if let Some(h) = rv.get("headers") {
                                work_headers = json_to_headers(h);
                            }
                            if let Some(b) = rv.get("body").and_then(|b| b.as_str()) {
                                work_body = b.to_string();
                            }
                        }
                        applied.push(rule.name.clone());
                    }
                    "breakpoint" => {
                        self.emit_rule_event(
                            "rule-applied", &rule.name, "response", id, &flow_method, &host_str, &path_str, None,
                        );
                        if let Some(rv) = &res.response {
                            if let Some(s) = rv.get("status").and_then(|s| s.as_u64()) {
                                work_status = s as u16;
                            }
                            if let Some(h) = rv.get("headers") {
                                work_headers = json_to_headers(h);
                            }
                            if let Some(b) = rv.get("body").and_then(|b| b.as_str()) {
                                work_body = b.to_string();
                            }
                        }
                        rule_break = true;
                        applied.push(rule.name.clone());
                    }
                    _ => {
                        self.emit_rule_event(
                            "rule-error", &rule.name, "response", id, &flow_method, &host_str, &path_str,
                            Some(res.error.as_deref().unwrap_or("unknown error")),
                        );
                        script_error = res.error;
                    }
                }
            }
            self.apply_env(&env);
        }

        // Pause on a matched response breakpoint (UI-defined) or a response rule's
        // ctx.breakpoint(). The client keeps waiting until the UI resolves it.
        let mut sub_bytes: Option<Vec<u8>> = None;
        let want_break = rule_break || self.breakpoint_matches(BpPhase::Response, &targets, &flow_method);
        // Set when the pause was resolved by an explicit Resolution (Execute/
        // Respond) rather than a dropped sender or auto-continue timeout. The
        // "flow-resumed" event for this case is deferred until after the final
        // store write below (~"flow-updated" emit) so it carries the fully
        // edited response, mirroring the handler-synthetic path in
        // `break_response`/its caller.
        let mut explicit_resume = false;
        if want_break {
            self.store.update(id, |f| {
                f.state = FlowState::Paused;
                f.paused_phase = Some("response".into());
                f.response = Some(ResponseMessage {
                    status: work_status,
                    headers: work_headers.clone(),
                    body: if is_text { work_body.clone().into_bytes() } else { display_body.clone() },
                    body_is_text: is_text,
                });
            });
            if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-paused", &u);
            }
            match self.await_resolution(id, BpPhase::Response).await {
                Some(Resolution::Execute { status, headers, body, body_bytes, .. }) => {
                    if let Some(sc) = status {
                        work_status = sc;
                    }
                    work_headers = headers;
                    if let Some(bb) = body_bytes {
                        sub_bytes = Some(bb); // file substitution — raw bytes win
                    } else if is_text {
                        work_body = body;
                    }
                    explicit_resume = true;
                }
                Some(Resolution::Abort(reason)) => {
                    self.store.update(id, |f| {
                        f.state = FlowState::Error;
                        f.paused_phase = None;
                        f.error = Some(reason.clone());
                    });
                    if let Some(u) = self.store.all().into_iter().find(|f| f.id == id) {
                        (self.emit)("flow-updated", &u);
                        (self.emit)("flow-resumed", &u);
                        self.persist(&u);
                    }
                    return build_abort_response(&reason);
                }
                // Respond has no meaning in the response phase (value-wise treated
                // as continue), but it's still an explicit resolution — report it.
                Some(Resolution::Respond { .. }) => {
                    explicit_resume = true;
                }
                None => {}
            }
            self.store.update(id, |f| {
                f.state = FlowState::Pending;
                f.paused_phase = None;
            });
        }

        let substituted = sub_bytes.is_some();
        let stored_is_text = if substituted { looks_textual(&work_headers) } else { is_text };
        let out_body: Vec<u8> = if let Some(b) = &sub_bytes {
            b.clone()
        } else if is_text {
            work_body.clone().into_bytes()
        } else {
            bytes.clone()
        };
        let stored_body: Vec<u8> = if let Some(b) = sub_bytes {
            b
        } else if is_text {
            work_body.clone().into_bytes()
        } else {
            display_body
        };
        self.store.update(id, |f| {
            f.response = Some(ResponseMessage {
                status: work_status,
                headers: work_headers.clone(),
                body: stored_body,
                body_is_text: stored_is_text,
            });
            f.timings.ttfb = Some(done_ms);
            f.timings.done = Some(done_ms);
            f.state = FlowState::Completed;
            for n in &applied {
                if !f.applied_rules.contains(n) {
                    f.applied_rules.push(n.clone());
                }
            }
            if let Some(e) = &script_error {
                f.error = Some(e.clone());
            }
        });
        if let Some(updated) = self.store.all().into_iter().find(|f| f.id == id) {
            (self.emit)("flow-updated", &updated);
            if explicit_resume {
                (self.emit)("flow-resumed", &updated);
            }
            self.persist(&updated);
        }

        let mut new_parts = parts;
        new_parts.status = hudsucker::hyper::StatusCode::from_u16(work_status).unwrap_or(new_parts.status);
        new_parts.headers = build_header_map(&work_headers, out_body.len(), is_text || substituted);
        Response::from_parts(new_parts, Body::from(Full::new(Bytes::from(out_body))))
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
    app_event: AppEventFn,
    secret_fn: crate::scripting::SecretFn,
    ca_dir: PathBuf,
    scripts: ScriptClient,
    rules: SharedRules,
    library: SharedLibrary,
    active_project: SharedProject,
    global_env: SharedGlobalEnv,
    data_dir: PathBuf,
    db: Option<DbHandle>,
    breakpoints: SharedBreakpoints,
    intercept: SharedIntercept,
    pending: BreakpointRegistry,
    timeout_secs: SharedTimeout,
    pause_others: SharedPauseOthers,
) -> Result<ProxyHandle> {
    let ca = load_or_create_ca(&ca_dir)?;
    let authority = RcgenAuthority::new(ca.key_pair, ca.ca_cert, 1_000);

    // забиндиться заранее, чтобы узнать реальный порт при :0
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let handler = CaptureHandler {
        store,
        emit,
        app_event,
        secret_fn,
        current_id: None,
        ca_pem: ca.cert_pem,
        started: std::time::Instant::now(),
        scripts,
        rules,
        library,
        active_project,
        global_env,
        data_dir,
        db,
        breakpoints,
        intercept,
        pending,
        timeout_secs,
        pause_others,
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
    use crate::rules::{Phase, Rule};
    use crate::scripting::spawn_engine;
    use crate::store::FlowStore;
    use std::io::Write;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex, RwLock};
    use std::time::Duration;

    fn scripting(
        rules: Vec<Rule>,
    ) -> (
        ScriptClient,
        SharedRules,
        SharedLibrary,
        SharedProject,
        SharedBreakpoints,
        SharedIntercept,
        BreakpointRegistry,
    ) {
        (
            spawn_engine(Duration::from_millis(500), Arc::new(|_: &str| None)),
            Arc::new(RwLock::new(rules)),
            Arc::new(RwLock::new(String::new())),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(vec![])),
            Arc::new(RwLock::new(true)),
            Arc::new(Mutex::new(std::collections::HashMap::new())),
        )
    }

    fn breakpoint(pattern: &str, on_request: bool, on_response: bool) -> crate::breakpoints::Breakpoint {
        crate::breakpoints::Breakpoint {
            id: "b".into(),
            name: "b".into(),
            enabled: true,
            pattern: pattern.into(),
            method: None,
            on_request,
            on_response,
            project_id: None,
        }
    }

    fn temp_ca() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "httpcatch-ca-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

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
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        let handle = start(proxy_addr, store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
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
        assert!(flows[0].timings.sent.is_some(), "timings.sent должен заполниться");
        assert!(flows[0].timings.done.is_some(), "timings.done должен заполниться");
        assert!(flows[0].timestamp > 0, "timestamp должен заполниться");
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
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client.get("http://trawl/").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let text = resp.text().await.unwrap();
        assert!(text.contains("BEGIN CERTIFICATE"), "got: {text}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // http://trawl/ должен отдавать сертификат даже когда активен проект,
    // который этот хост не трекает (раньше запрос уходил в апстрим → 502).
    #[tokio::test]
    async fn serves_ca_pem_when_active_project_excludes_trawl() {
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-cert-proj-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *p.write().unwrap() = Some(crate::projects::Project {
            id: "p1".into(),
            name: "p1".into(),
            include_hosts: vec!["example.com".into()],
            exclude_hosts: vec![],
            env: vec![],
        });
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client.get("http://trawl/").send().await.unwrap();
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
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
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
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
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

    fn rule(name: &str, pattern: &str, phase: Phase, script: &str) -> Rule {
        Rule {
            id: name.into(),
            name: name.into(),
            enabled: true,
            pattern: pattern.into(),
            phase,
            script: script.into(),
            project_id: None,
        }
    }

    // Правило фазы запроса добавляет заголовок; upstream его возвращает, поток отражает изменение.
    #[tokio::test]
    async fn request_rule_adds_header_reaches_upstream() {
        // upstream эхо-сервер: возвращает значение X-Debug в теле
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let got = req
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("x-debug:"))
                        .map(|l| l.split_once(':').map(|(_, v)| v.trim()).unwrap_or(""))
                        .unwrap_or("<none>")
                        .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        got.len(),
                        got
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let host = format!("{}", upstream_addr);
        let rules = vec![rule(
            "add-debug",
            &format!("{host}/*"),
            Phase::Request,
            "request.headers['X-Debug'] = 'yes';",
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let body = client
            .get(format!("http://{upstream_addr}/api"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "yes", "upstream должен получить добавленный заголовок");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let flow = store.all().into_iter().next().unwrap();
        assert!(flow.applied_rules.contains(&"add-debug".to_string()));

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Правило-mock короткозамыкает запрос без обращения к upstream.
    #[tokio::test]
    async fn request_rule_mock_short_circuits() {
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        // upstream, которого нет: 127.0.0.1:1 — если бы не mock, был бы отказ
        let rules = vec![rule(
            "mocker",
            "127.0.0.1:1/*",
            Phase::Request,
            r#"ctx.mock({ status: 201, body: 'mocked!' });"#,
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client.get("http://127.0.0.1:1/x").send().await.unwrap();
        assert_eq!(resp.status(), 201);
        assert_eq!(resp.text().await.unwrap(), "mocked!");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    fn app_event_noop() -> AppEventFn {
        Arc::new(|_, _| {})
    }
    fn secret_none() -> crate::scripting::SecretFn {
        Arc::new(|_: &str| None)
    }

    // A rule's notify() call is forwarded to the app's AppEventFn as a
    // ("script-notify", payload) event.
    #[tokio::test]
    async fn rule_notify_reaches_notify_fn() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let rules = vec![rule(
            "notifier",
            "*",
            Phase::Request,
            "notify('hello', { channel: 'ops' });",
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();

        let (ntx, mut nrx) = tokio::sync::mpsc::unbounded_channel::<(String, serde_json::Value)>();
        let app_event: AppEventFn = Arc::new(move |event: &str, payload: serde_json::Value| {
            let _ = ntx.send((event.to_string(), payload));
        });

        let handle = start(
            "127.0.0.1:0".parse().unwrap(),
            store.clone(),
            emit,
            app_event,
            secret_none(),
            ca_dir.clone(),
            s,
            r,
            l,
            p,
            Arc::new(RwLock::new(vec![])),
            ca_dir.clone(),
            None,
            bps,
            icept,
            pending,
            Arc::new(RwLock::new(0)),
            Arc::new(RwLock::new(false)),
        )
        .await
        .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let _ = client
            .get(format!("http://{upstream_addr}/api"))
            .send()
            .await
            .unwrap();

        // The rule both notifies and applies cleanly, so two events arrive on
        // the channel; find the "script-notify" one.
        let mut p = None;
        for _ in 0..2 {
            let (event, payload) =
                tokio::time::timeout(std::time::Duration::from_secs(5), nrx.recv())
                    .await
                    .expect("notification not emitted")
                    .unwrap();
            if event == "script-notify" {
                p = Some(payload);
                break;
            }
        }
        let p = p.expect("script-notify not emitted");
        assert_eq!(p["text"], "hello");
        assert_eq!(p["channel"], "ops");
        assert_eq!(p["source"], "rule");
        assert_eq!(p["ruleName"], "notifier");
        assert!(p["flowId"].is_u64());

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Two request-phase rules: one applies cleanly ("ok"), the next throws
    // ("boom"). Both outcomes are reported to the app via AppEventFn as
    // ("rule-applied", ...) / ("rule-error", ...) events.
    #[tokio::test]
    async fn rule_apply_and_error_reach_app_event_fn() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let rules = vec![
            rule("ok", "*", Phase::Request, "setHeader(request,'X-A','1');"),
            rule("boom", "*", Phase::Request, "throw new Error('kaput');"),
        ];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();

        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<(String, serde_json::Value)>();
        let app_event: AppEventFn = Arc::new(move |event: &str, payload: serde_json::Value| {
            let _ = etx.send((event.to_string(), payload));
        });

        let handle = start(
            "127.0.0.1:0".parse().unwrap(),
            store.clone(),
            emit,
            app_event,
            secret_none(),
            ca_dir.clone(),
            s,
            r,
            l,
            p,
            Arc::new(RwLock::new(vec![])),
            ca_dir.clone(),
            None,
            bps,
            icept,
            pending,
            Arc::new(RwLock::new(0)),
            Arc::new(RwLock::new(false)),
        )
        .await
        .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let _ = client
            .get(format!("http://{upstream_addr}/api"))
            .send()
            .await
            .unwrap();

        let mut applied: Option<serde_json::Value> = None;
        let mut errored: Option<serde_json::Value> = None;
        for _ in 0..2 {
            let (event, payload) =
                tokio::time::timeout(std::time::Duration::from_secs(5), erx.recv())
                    .await
                    .expect("app event not emitted")
                    .unwrap();
            match event.as_str() {
                "rule-applied" => applied = Some(payload),
                "rule-error" => errored = Some(payload),
                other => panic!("unexpected app event {other}"),
            }
        }

        let applied = applied.expect("rule-applied not emitted");
        assert_eq!(applied["ruleName"], "ok");
        assert_eq!(applied["phase"], "request");
        assert_eq!(applied["method"], "GET");
        assert!(applied["flowId"].is_u64());
        assert!(applied["host"].is_string());
        assert!(applied["path"].is_string());

        let errored = errored.expect("rule-error not emitted");
        assert_eq!(errored["ruleName"], "boom");
        assert!(errored["error"].as_str().unwrap().contains("kaput"));

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A request breakpoint holds the flow; resolving Execute with an edited header
    // sends the edit to the upstream echo server.
    #[tokio::test]
    async fn request_breakpoint_execute_applies_edit() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let emit: EmitFn = Arc::new(move |e: &str, _f: &Flow| {
            let _ = etx.send(e.to_string());
        });
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        // Resolver task: wait until the flow is paused, then Execute with an edit.
        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                let paused = store2.all().into_iter().find(|f| f.state == FlowState::Paused);
                if let Some(f) = paused {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None,
                            path: None,
                            status: None,
                            headers: vec![("X-Debug".into(), "edited".into())],
                            body: String::new(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let echoed = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().text().await.unwrap();
        assert!(echoed.to_lowercase().contains("x-debug: edited"), "edit not applied: {echoed}");

        // Resolving the paused flow with an explicit Execute resolution must
        // surface a "flow-resumed" event to the app.
        let mut events = Vec::new();
        while let Ok(e) = erx.try_recv() {
            events.push(e);
        }
        assert!(events.contains(&"flow-resumed".to_string()), "events: {events:?}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A request breakpoint fires BEFORE a matching handler rule: the flow pauses,
    // the edit is applied, and the edited request then flows into the handler's send().
    #[tokio::test]
    async fn request_breakpoint_fires_before_handler_rule() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let rules = vec![rule("echo", &format!("{upstream_addr}/*"), Phase::Handler, "return send(request);")];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.state == FlowState::Paused) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None, path: None, status: None,
                            headers: vec![("X-Bp".into(), "hit".into())], body: String::new(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let echoed = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().text().await.unwrap();
        assert!(
            echoed.to_lowercase().contains("x-bp: hit"),
            "breakpoint did not fire before the handler rule: {echoed}"
        );

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Regression: a breakpoint whose pattern includes the scheme (a pasted full
    // URL) must still match — proxy targets are scheme-less host/path.
    #[tokio::test]
    async fn request_breakpoint_matches_scheme_prefixed_pattern() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("http://{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.state == FlowState::Paused) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None, path: None, status: None,
                            headers: vec![("X-Bp".into(), "hit".into())], body: String::new(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let echoed = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().text().await.unwrap();
        assert!(echoed.to_lowercase().contains("x-bp: hit"), "scheme-prefixed breakpoint did not fire: {echoed}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A request breakpoint can edit the path+query; the edited path reaches upstream.
    #[tokio::test]
    async fn request_breakpoint_execute_edits_query() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.state == FlowState::Paused) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None,
                            path: Some("/api?edited=1".into()),
                            status: None,
                            headers: f.request.headers.clone(),
                            body: String::new(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let echoed = client.get(format!("http://{upstream_addr}/api?edited=0"))
            .send().await.unwrap().text().await.unwrap();
        assert!(echoed.contains("/api?edited=1"), "edited query not sent upstream: {echoed}");
        assert!(!echoed.contains("edited=0"), "original query should be replaced: {echoed}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // With a timeout set and nobody resolving, a paused flow auto-continues.
    #[tokio::test]
    async fn breakpoint_auto_continues_on_timeout() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let emit: EmitFn = Arc::new(move |e: &str, _f: &Flow| {
            let _ = etx.send(e.to_string());
        });
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let timeout = Arc::new(RwLock::new(1u64)); // 1s auto-continue
        let pother = Arc::new(RwLock::new(false));
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, timeout, pother,
        ).await.unwrap();
        let bound = handle.local_addr();

        // No resolver — the flow should time out and continue unmodified.
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let status = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().status();
        assert_eq!(status, 200, "flow should auto-continue after the timeout");

        // The auto-continue path must surface "breakpoint-timeout" (not
        // "flow-resumed", which is reserved for explicit resolutions).
        let mut events = Vec::new();
        while let Ok(e) = erx.try_recv() {
            events.push(e);
        }
        assert!(events.contains(&"breakpoint-timeout".to_string()), "events: {events:?}");
        assert!(!events.contains(&"flow-resumed".to_string()), "events: {events:?}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Abort resolution short-circuits with 502.
    #[tokio::test]
    async fn request_breakpoint_abort_returns_502() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), true, false)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.state == FlowState::Paused) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Request)) {
                        let _ = tx.send(Resolution::Abort("nope".into()));
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let status = client.get(format!("http://{upstream_addr}/api"))
            .send().await.unwrap().status();
        assert_eq!(status, 502);

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A response breakpoint edits status + body before the client receives it.
    #[tokio::test]
    async fn response_breakpoint_execute_edits_response() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = sock.read(&mut b).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\norig").await;
                });
            }
        });

        let store = FlowStore::new(10);
        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<(String, Flow)>();
        let emit: EmitFn = Arc::new(move |e: &str, f: &Flow| {
            let _ = etx.send((e.to_string(), f.clone()));
        });
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), false, true)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None,
                            path: None,
                            status: Some(418),
                            headers: vec![("Content-Type".into(), "text/plain".into())],
                            body: "edited".into(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get(format!("http://{upstream_addr}/api")).send().await.unwrap();
        assert_eq!(resp.status(), 418);
        assert_eq!(resp.text().await.unwrap(), "edited");

        // The "flow-resumed" event for an explicit Execute resolution on the
        // response phase must carry the FINAL edited response (status 418 /
        // "edited"), not the pre-edit original — it's emitted after the final
        // store write, mirroring the handler-synthetic path.
        let mut resumed: Option<Flow> = None;
        let mut events = Vec::new();
        while let Ok((e, f)) = erx.try_recv() {
            events.push(e.clone());
            if e == "flow-resumed" {
                resumed = Some(f);
            }
        }
        let resumed = match resumed {
            Some(f) => f,
            None => panic!("flow-resumed not emitted; events: {events:?}"),
        };
        let resp_msg = resumed.response.expect("resumed flow has no response");
        assert_eq!(resp_msg.status, 418);
        assert_eq!(String::from_utf8_lossy(&resp_msg.body), "edited");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A response-phase breakpoint resolved with Resolution::Respond has no
    // effect on the outgoing bytes (treated as continue, per the existing
    // value-handling), but it IS an explicit resolution and must still emit
    // "flow-resumed" — distinguishing it from an auto-continue timeout.
    #[tokio::test]
    async fn response_breakpoint_respond_resolution_emits_flow_resumed() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = sock.read(&mut b).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\norig").await;
                });
            }
        });

        let store = FlowStore::new(10);
        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let emit: EmitFn = Arc::new(move |e: &str, _f: &Flow| {
            let _ = etx.send(e.to_string());
        });
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), false, true)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Respond {
                            status: 999, // ignored in the response phase
                            headers: vec![],
                            body: "ignored".into(),
                            body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get(format!("http://{upstream_addr}/api")).send().await.unwrap();
        // Respond has no meaning here — original upstream response passes through.
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "orig");

        let mut events = Vec::new();
        while let Ok(e) = erx.try_recv() {
            events.push(e);
        }
        assert!(events.contains(&"flow-resumed".to_string()), "events: {events:?}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A response breakpoint fires on a handler rule's synthetic response too —
    // even though it never passes through handle_response.
    #[tokio::test]
    async fn response_breakpoint_fires_on_handler_response() {
        let store = FlowStore::new(10);
        let (etx, mut erx) = tokio::sync::mpsc::unbounded_channel::<(String, Flow)>();
        let emit: EmitFn = Arc::new(move |e: &str, f: &Flow| {
            let _ = etx.send((e.to_string(), f.clone()));
        });
        // Handler returns a synthetic response (no upstream) on 1.2.3.4.
        let rules = vec![rule(
            "mock-handler",
            "handler.test/*",
            Phase::Handler,
            "return { status: 200, headers: {}, body: 'orig' };",
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        *bps.write().unwrap() = vec![breakpoint("handler.test/*", false, true)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None, path: None, status: Some(418),
                            headers: vec![("Content-Type".into(), "text/plain".into())],
                            body: "edited".into(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get("http://handler.test/x").send().await.unwrap();
        assert_eq!(resp.status(), 418, "response breakpoint did not fire on handler response");
        assert_eq!(resp.text().await.unwrap(), "edited");

        // The "flow-resumed" event must carry the FINAL resolved response
        // (status 418 / "edited"), not a stale pre-resolution snapshot.
        let mut resumed: Option<Flow> = None;
        let mut events = Vec::new();
        while let Ok((e, f)) = erx.try_recv() {
            events.push(e.clone());
            if e == "flow-resumed" {
                resumed = Some(f);
            }
        }
        let resumed = match resumed {
            Some(f) => f,
            None => panic!("flow-resumed not emitted; events: {events:?}"),
        };
        let resp_msg = resumed.response.expect("resumed flow has no response");
        assert_eq!(resp_msg.status, 418);
        assert_eq!(String::from_utf8_lossy(&resp_msg.body), "edited");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A response breakpoint with a method filter (e.g. GET) must still match on
    // the response phase — the request's method is matched, not an empty string.
    #[tokio::test]
    async fn response_breakpoint_matches_method_filter() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = sock.read(&mut b).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 4\r\n\r\norig").await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        let mut bp = breakpoint(&format!("{upstream_addr}/*"), false, true);
        bp.method = Some("GET".into());
        *bps.write().unwrap() = vec![bp];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None, path: None, status: Some(418),
                            headers: vec![("Content-Type".into(), "text/plain".into())],
                            body: "edited".into(), body_bytes: None,
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get(format!("http://{upstream_addr}/api")).send().await.unwrap();
        assert_eq!(resp.status(), 418, "method-filtered response breakpoint did not fire");
        assert_eq!(resp.text().await.unwrap(), "edited");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // A response breakpoint can replace the body with raw bytes (an uploaded file);
    // the client receives exactly those bytes.
    #[tokio::test]
    async fn response_breakpoint_substitutes_binary_body() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = sock.read(&mut b).await;
                    let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}").await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *bps.write().unwrap() = vec![breakpoint(&format!("{upstream_addr}/*"), false, true)];
        let ca_dir = temp_ca();
        let handle = start(
            "127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(),
            s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending.clone(), Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)),
        ).await.unwrap();
        let bound = handle.local_addr();

        // A tiny PNG-like binary payload (non-UTF8 bytes).
        let file: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF];
        let file_task = file.clone();
        let pending2 = pending.clone();
        let store2 = store.clone();
        tokio::spawn(async move {
            loop {
                if let Some(f) = store2.all().into_iter().find(|f| f.paused_phase.as_deref() == Some("response")) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&(f.id, BpPhase::Response)) {
                        let _ = tx.send(Resolution::Execute {
                            method: None, path: None, status: None,
                            headers: vec![("Content-Type".into(), "image/png".into())],
                            body: String::new(),
                            body_bytes: Some(file_task.clone()),
                        });
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build().unwrap();
        let resp = client.get(format!("http://{upstream_addr}/img")).send().await.unwrap();
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        let got = resp.bytes().await.unwrap().to_vec();
        assert_eq!(got, file, "client should receive the substituted file bytes");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Handler-правило само выполняет запрос через send() и преобразует ответ.
    #[tokio::test]
    async fn handler_rule_sends_and_transforms_response() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello")
                        .await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let rules = vec![rule(
            "upper",
            &format!("{upstream_addr}/*"),
            Phase::Handler,
            "const r = send(request); r.body = r.body.toUpperCase(); return r;",
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let body = client
            .get(format!("http://{upstream_addr}/data"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "HELLO", "handler должен вернуть преобразованный ответ");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let flow = store.all().into_iter().next().unwrap();
        assert!(flow.applied_rules.contains(&"upper".to_string()));

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Диагностика: handler send(request) должен сохранять путь, query и заголовки.
    #[tokio::test]
    async fn handler_send_preserves_path_query_headers() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    // эхо: возвращаем сырой запрос как тело
                    let echo = String::from_utf8_lossy(&buf[..n]).to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                        echo.len(),
                        echo
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });

        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let rules = vec![rule(
            "echo",
            &format!("{upstream_addr}/*"),
            Phase::Handler,
            "return send(request);",
        )];
        let (s, r, l, p, bps, icept, pending) = scripting(rules);
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let echoed = client
            .get(format!("http://{upstream_addr}/a/b?x=1&y=2"))
            .header("X-Custom", "hello")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        assert!(echoed.contains("/a/b?x=1&y=2"), "путь+query потеряны: {echoed}");
        assert!(
            echoed.to_lowercase().contains("x-custom: hello"),
            "заголовок потерян: {echoed}"
        );

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    fn echo_upstream() -> impl std::future::Future<Output = SocketAddr> {
        async {
            let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = upstream.local_addr().unwrap();
            tokio::spawn(async move {
                loop {
                    let (mut sock, _) = upstream.accept().await.unwrap();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut buf = vec![0u8; 4096];
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        let echo = String::from_utf8_lossy(&buf[..n]).to_string();
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                            echo.len(),
                            echo
                        );
                        let _ = sock.write_all(resp.as_bytes()).await;
                    });
                }
            });
            addr
        }
    }

    fn project(id: &str, include: &[&str]) -> crate::projects::Project {
        crate::projects::Project {
            id: id.into(),
            name: id.into(),
            include_hosts: include.iter().map(|s| s.to_string()).collect(),
            exclude_hosts: vec![],
            env: vec![],
        }
    }

    // Активный проект: запрос к нетрекаемому хосту проксируется, но не сохраняется.
    #[tokio::test]
    async fn untracked_host_not_stored() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]);
        *p.write().unwrap() = Some(project("proj", &["tracked.example"]));
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let ok = client
            .get(format!("http://{upstream_addr}/x"))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(ok, 200, "нетрекаемый хост всё равно проксируется");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(store.all().len(), 0, "нетрекаемый хост не должен сохраняться");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Активный проект: трекаемый хост пишется, применяются только правила проекта.
    #[tokio::test]
    async fn tracked_host_uses_only_project_rules() {
        let upstream_addr = echo_upstream().await;
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});

        let mut proj_rule = rule("proj-rule", "127.0.0.1/*", Phase::Request, "request.headers['X-Proj'] = '1';");
        proj_rule.project_id = Some("proj".into());
        let global_rule = rule("global-rule", "127.0.0.1/*", Phase::Request, "request.headers['X-Global'] = '1';");
        let (s, r, l, p, bps, icept, pending) = scripting(vec![proj_rule, global_rule]);
        *p.write().unwrap() = Some(project("proj", &["127.0.0.1"]));
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store.clone(), emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let echoed = client
            .get(format!("http://{upstream_addr}/api"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let low = echoed.to_lowercase();
        assert!(low.contains("x-proj"), "проектное правило должно примениться: {echoed}");
        assert!(!low.contains("x-global"), "глобальное правило не должно применяться при активном проекте");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let flow = store.all().into_iter().next().unwrap();
        assert!(flow.applied_rules.contains(&"proj-rule".to_string()));
        assert!(!flow.applied_rules.contains(&"global-rule".to_string()));

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }

    // Регрессия: gzip-ответ должен дойти до клиента декодируемым (не оставляем
    // content-encoding при распакованном теле).
    #[tokio::test]
    async fn gzip_response_reaches_client_decodable() {
        let payload = b"{\"gzipped\": true, \"msg\": \"hello world\"}";
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(payload).unwrap();
        let gz = enc.finish().unwrap();

        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let gz_task = gz.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = upstream.accept().await.unwrap();
                let gz = gz_task.clone();
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
        let (s, r, l, p, bps, icept, pending) = scripting(vec![]); // без правил — обычный путь
        let ca_dir = temp_ca();
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, app_event_noop(), secret_none(), ca_dir.clone(), s, r, l, p, Arc::new(RwLock::new(vec![])), ca_dir.clone(), None, bps, icept, pending, Arc::new(RwLock::new(0)), Arc::new(RwLock::new(false)))
            .await
            .unwrap();
        let bound = handle.local_addr();

        // клиент с gzip auto-decode — если прокси оставит content-encoding при
        // распакованном теле, декодирование сломается.
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let text = client
            .get(format!("http://{upstream_addr}/api"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(text, String::from_utf8_lossy(payload));

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }
}
