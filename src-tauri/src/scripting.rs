use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{Context, Ctx, Function, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

/// Resolves a named secret for scripts. Injected so tests avoid the real Keychain.
pub type SecretFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResult {
    /// continue | mock | abort | error
    pub action: String,
    #[serde(default)]
    pub request: Option<serde_json::Value>,
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    #[serde(default)]
    pub mock: Option<serde_json::Value>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// env as modified by the script (written back to the project).
    #[serde(default)]
    pub env: Option<serde_json::Value>,
    /// notify(...) calls collected during the run.
    #[serde(default)]
    pub notifications: Vec<serde_json::Value>,
    /// Trace of stdlib/send operations for the rule run.
    #[serde(default)]
    pub trace: Vec<serde_json::Value>,
}

impl ScriptResult {
    pub fn error(msg: impl Into<String>) -> Self {
        ScriptResult {
            action: "error".into(),
            request: None,
            response: None,
            mock: None,
            reason: None,
            error: Some(msg.into()),
            env: None,
            notifications: Vec::new(),
            trace: Vec::new(),
        }
    }
}

struct ScriptJob {
    prelude: String,
    script: String,
    input_json: String,
    reply: oneshot::Sender<ScriptResult>,
}

/// Client to the script engine. Cloneable, Send+Sync — fits the proxy handler.
#[derive(Clone)]
pub struct ScriptClient {
    tx: mpsc::UnboundedSender<ScriptJob>,
}

impl ScriptClient {
    /// Runs `script` (with `prelude` prepended) against the `input_json` context.
    pub async fn run(&self, prelude: String, script: String, input_json: String) -> ScriptResult {
        let (reply, rx) = oneshot::channel();
        let job = ScriptJob { prelude, script, input_json, reply };
        if self.tx.send(job).is_err() {
            return ScriptResult::error("script engine unavailable");
        }
        rx.await
            .unwrap_or_else(|_| ScriptResult::error("script engine dropped"))
    }
}

/// Registers the natives shared by every engine flavor: secrets, JSONPath,
/// digest/base64 and the counter store. `state` is the app-wide store for real
/// traffic and a fresh instance for dry-run — the closure must capture it, not
/// reach for a global, or dry-run isolation silently breaks.
fn register_common_natives(
    c: &Ctx<'_>,
    secrets: &SecretFn,
    state: &Arc<crate::script_state::ScriptState>,
) -> Result<(), rquickjs::Error> {
    let g = c.globals();
    let sfn = secrets.clone();
    g.set(
        "__native_secret",
        Function::new(c.clone(), move |name: String| -> Option<String> { sfn(&name) })?,
    )?;
    g.set(
        "__native_jsonpath_locate",
        Function::new(c.clone(), |doc: String, path: String| -> String {
            crate::jsonpath::locate(&doc, &path)
        })?,
    )?;
    g.set(
        "__native_digest",
        Function::new(c.clone(), |kind: String, key: String, data: String| -> Option<String> {
            crate::hashing::digest(&kind, &key, &data)
        })?,
    )?;
    g.set(
        "__native_base64",
        Function::new(c.clone(), |op: String, data: String| -> Option<String> {
            crate::hashing::base64(&op, &data)
        })?,
    )?;
    let st = state.clone();
    g.set(
        "__native_counter",
        Function::new(c.clone(), move |op: String, name: String| -> f64 {
            match op.as_str() {
                "bump" => st.bump(&name) as f64,
                _ => {
                    st.reset(&name);
                    0.0
                }
            }
        })?,
    )?;
    Ok(())
}

/// Spins up the engine on a dedicated thread (the QuickJS runtime isn't Send across await).
pub fn spawn_engine(timeout: Duration, secrets: SecretFn) -> ScriptClient {
    let (tx, mut rx) = mpsc::unbounded_channel::<ScriptJob>();

    std::thread::Builder::new()
        .name("script-engine".into())
        .spawn(move || {
            let rt = Runtime::new().expect("create quickjs runtime");
            let deadline = Arc::new(Mutex::new(Instant::now()));
            {
                let d = deadline.clone();
                rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= *d.lock().unwrap())));
            }
            let ctx = Context::full(&rt).expect("create quickjs context");
            ctx.with(|c| {
                register_common_natives(&c, &secrets, &crate::script_state::global())
                    .expect("register natives");
            });

            while let Some(job) = rx.blocking_recv() {
                *deadline.lock().unwrap() = Instant::now() + timeout;
                let res = ctx.with(|c| eval_job(&c, &job));
                let _ = job.reply.send(res);
            }
        })
        .expect("spawn script engine thread");

    ScriptClient { tx }
}

fn eval_job(c: &Ctx<'_>, job: &ScriptJob) -> ScriptResult {
    if let Err(e) = c.globals().set("__input", job.input_json.clone()) {
        return ScriptResult::error(format!("set input: {e}"));
    }
    let src = build_source(&job.prelude, &job.script);
    match c.eval::<String, _>(src) {
        Ok(json) => serde_json::from_str(&json)
            .unwrap_or_else(|e| ScriptResult::error(format!("bad result json: {e}"))),
        Err(_) => {
            let caught = c.catch();
            let msg = match caught.into_exception() {
                Some(ex) => ex.message().unwrap_or_else(|| ex.to_string()),
                None => "script compile error or timeout".to_string(),
            };
            ScriptResult::error(msg)
        }
    }
}

/// Built-in helper functions injected before every rule (both phases). Source:
/// src-tauri/js/stdlib.js. Kept in sync with the declarations shown for
/// autocomplete in `src/scripting/stdlib.ts`.
const STD_LIB: &str = include_str!("../js/stdlib.js");

fn build_source(prelude: &str, script: &str) -> String {
    let full_prelude = format!("{STD_LIB}\n{prelude}");
    let prefix = format!(
        r#"
(function() {{
  try {{
    const ctx = JSON.parse(__input);
    ctx.__action = "continue";
    ctx.mock = function(resp) {{ ctx.__action = "mock"; ctx.__mock = resp; }};
    ctx.abort = function(reason) {{ ctx.__action = "abort"; ctx.__reason = reason || "aborted"; }};
    ctx.breakpoint = function() {{ ctx.__action = "breakpoint"; }};
    ctx.__notifications = [];
    ctx.__trace = [];
    if (!ctx.env) ctx.env = {{}};
    const request = ctx.request;
    const response = ctx.response;
    const env = ctx.env;
    /* ── library ── */
    {full_prelude}
    /* ── rule script ── */
"#
    );
    let offset = prefix.lines().count();
    format!(
        r#"{prefix}{script}
    return JSON.stringify({{
      action: ctx.__action,
      request: ctx.request,
      response: ctx.response,
      mock: ctx.__mock || null,
      reason: ctx.__reason || null,
      env: ctx.env,
      notifications: ctx.__notifications,
      trace: ctx.__trace
    }});
  }} catch (e) {{
    var __m = String((e && e.message) || e);
    var __ln = e && e.lineNumber;
    if (!__ln && e && e.stack) {{ var __sm = String(e.stack).match(/:(\d+)/); if (__sm) __ln = Number(__sm[1]); }}
    if (__ln && (__ln - {offset}) > 0) {{ __m += " (line " + (__ln - {offset}) + ")"; }}
    return JSON.stringify({{ action: "error", error: __m, trace: (typeof ctx !== "undefined" && ctx.__trace) || [] }});
  }}
}})()
"#
    )
}

// ── Handler mode: the script performs the request itself via blocking send() ──

/// Lazy init of the blocking client. Created on first call
/// inside a blocking context (not in the async runtime) — otherwise reqwest panics.
fn http_client() -> &'static reqwest::blocking::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

/// Performs a real HTTP request (blocking) and returns {status,headers,body} as JSON.
fn native_send(req_json: &str) -> String {
    let client = http_client();
    let v: Value = match serde_json::from_str(req_json) {
        Ok(v) => v,
        Err(e) => return json!({"status":0,"headers":{},"body":"","error":format!("bad request: {e}")}).to_string(),
    };
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("GET");
    let url = v.get("url").and_then(|u| u.as_str()).unwrap_or("");
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut rb = client.request(m, url);
    if let Some(h) = v.get("headers").and_then(|h| h.as_object()) {
        for (k, val) in h {
            let lk = k.to_ascii_lowercase();
            if matches!(
                lk.as_str(),
                "host" | "content-length" | "connection" | "transfer-encoding" | "accept-encoding"
            ) {
                continue;
            }
            if let Some(vs) = val.as_str() {
                rb = rb.header(k, vs);
            }
        }
    }
    if let Some(b) = v.get("body").and_then(|b| b.as_str()) {
        if !b.is_empty() {
            rb = rb.body(b.to_string());
        }
    }
    match rb.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut headers = serde_json::Map::new();
            for (k, val) in resp.headers().iter() {
                // body is already decoded by reqwest — don't carry over consistency-breaking headers
                if matches!(
                    k.as_str().to_ascii_lowercase().as_str(),
                    "content-encoding" | "content-length" | "transfer-encoding"
                ) {
                    continue;
                }
                headers.insert(
                    k.as_str().to_string(),
                    Value::String(String::from_utf8_lossy(val.as_bytes()).to_string()),
                );
            }
            let body = resp.text().unwrap_or_default();
            json!({ "status": status, "headers": headers, "body": body }).to_string()
        }
        Err(e) => json!({"status":0,"headers":{},"body":"","error":e.to_string()}).to_string(),
    }
}

fn build_handler_source(prelude: &str, script: &str) -> String {
    let full_prelude = format!("{STD_LIB}\n{prelude}");
    let prefix = format!(
        r#"
(function() {{
  try {{
    const ctx = JSON.parse(__input);
    if (!ctx.env) ctx.env = {{}};
    ctx.__notifications = [];
    ctx.__trace = [];
    const request = ctx.request;
    const env = ctx.env;
    function send(req) {{ var __t0 = Date.now(); var __r = JSON.parse(__native_send(JSON.stringify(req || request))); ctx.__trace.push({{ op: "send", status: __r.status, ms: Date.now() - __t0 }}); return __r; }}
    function sleep(ms) {{ __native_sleep(ms); }}
    {full_prelude}
    const __out = (function() {{
"#
    );
    let offset = prefix.lines().count();
    format!(
        r#"{prefix}{script}
    }})();
    if (__out === undefined || __out === null) {{
      return JSON.stringify({{ action: "error", error: "handler did not return a response (needs return response)", env: ctx.env, notifications: ctx.__notifications, trace: ctx.__trace }});
    }}
    return JSON.stringify({{ action: "respond", response: __out, env: ctx.env, notifications: ctx.__notifications, trace: ctx.__trace }});
  }} catch (e) {{
    var __m = String((e && e.message) || e);
    var __ln = e && e.lineNumber;
    if (!__ln && e && e.stack) {{ var __sm = String(e.stack).match(/:(\d+)/); if (__sm) __ln = Number(__sm[1]); }}
    if (__ln && (__ln - {offset}) > 0) {{ __m += " (line " + (__ln - {offset}) + ")"; }}
    return JSON.stringify({{ action: "error", error: __m, trace: (typeof ctx !== "undefined" && ctx.__trace) || [] }});
  }}
}})()
"#
    )
}

/// send() implementation for the handler engine (swapped out for a replay in dry-run).
pub type SendFn = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Synchronously executes a handler script: it does its own send()/sleep() and returns a response.
/// Call outside the tokio runtime (via spawn_blocking).
pub fn execute_handler(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
) -> ScriptResult {
    execute_handler_with_send(
        prelude,
        script,
        input_json,
        js_timeout,
        secrets,
        Arc::new(|req: &str| native_send(req)),
        crate::script_state::global(),
    )
}

/// Like `execute_handler`, but with a swappable send() implementation and state
/// store — used by dry-run to replay a captured response instead of making a
/// real network call, with counters isolated from live traffic.
#[allow(clippy::too_many_arguments)]
pub fn execute_handler_with_send(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
    send_impl: SendFn,
    state: Arc<crate::script_state::ScriptState>,
) -> ScriptResult {
    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => return ScriptResult::error(format!("runtime: {e}")),
    };
    let deadline = Arc::new(Mutex::new(Instant::now() + js_timeout));
    {
        let d = deadline.clone();
        rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= *d.lock().unwrap())));
    }
    let ctx = match Context::full(&rt) {
        Ok(c) => c,
        Err(e) => return ScriptResult::error(format!("context: {e}")),
    };
    ctx.with(|c| {
        let g = c.globals();
        if g.set("__input", input_json.to_string()).is_err() {
            return ScriptResult::error("set input failed");
        }
        let si = send_impl.clone();
        let send_fn = match Function::new(c.clone(), move |req: String| -> String {
            si(&req)
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind send: {e}")),
        };
        let _ = g.set("__native_send", send_fn);
        let sleep_fn = match Function::new(c.clone(), move |ms: f64| {
            let ms = ms.clamp(0.0, 10_000.0) as u64;
            std::thread::sleep(Duration::from_millis(ms));
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind sleep: {e}")),
        };
        let _ = g.set("__native_sleep", sleep_fn);
        if let Err(e) = register_common_natives(&c, &secrets, &state) {
            return ScriptResult::error(format!("bind natives: {e}"));
        }

        let src = build_handler_source(prelude, script);
        match c.eval::<String, _>(src) {
            Ok(json) => serde_json::from_str(&json)
                .unwrap_or_else(|e| ScriptResult::error(format!("bad result json: {e}"))),
            Err(_) => {
                let caught = c.catch();
                let msg = match caught.into_exception() {
                    Some(ex) => ex.message().unwrap_or_else(|| ex.to_string()),
                    None => "handler error or timeout".to_string(),
                };
                ScriptResult::error(msg)
            }
        }
    })
}

/// One-off run of a request/response script in a fresh runtime (dry-run,
/// without the engine's shared thread). No network access.
pub fn execute_once(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
    state: Arc<crate::script_state::ScriptState>,
) -> ScriptResult {
    let rt = match Runtime::new() {
        Ok(r) => r,
        Err(e) => return ScriptResult::error(format!("runtime: {e}")),
    };
    let deadline = Arc::new(Mutex::new(Instant::now() + js_timeout));
    {
        let d = deadline.clone();
        rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= *d.lock().unwrap())));
    }
    let ctx = match Context::full(&rt) {
        Ok(c) => c,
        Err(e) => return ScriptResult::error(format!("context: {e}")),
    };
    ctx.with(|c| {
        let g = c.globals();
        if g.set("__input", input_json.to_string()).is_err() {
            return ScriptResult::error("set input failed");
        }
        if let Err(e) = register_common_natives(&c, &secrets, &state) {
            return ScriptResult::error(format!("bind natives: {e}"));
        }
        let src = build_source(prelude, script);
        match c.eval::<String, _>(src) {
            Ok(json) => serde_json::from_str(&json)
                .unwrap_or_else(|e| ScriptResult::error(format!("bad result json: {e}"))),
            Err(_) => {
                let caught = c.catch();
                let msg = match caught.into_exception() {
                    Some(ex) => ex.message().unwrap_or_else(|| ex.to_string()),
                    None => "script error or timeout".to_string(),
                };
                ScriptResult::error(msg)
            }
        }
    })
}

/// Literal JSONPath arguments of path functions (2nd argument — a string in '…' or "…").
/// The function list is kept in sync with src/scripting/pathContext.ts.
pub fn extract_path_literals(script: &str) -> Vec<String> {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r#"\b(?:patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(?:'([^'\\]*(?:\\.[^'\\]*)*)'|"([^"\\]*(?:\\.[^"\\]*)*)")"#,
        )
        .expect("path literal regex")
    });
    re.captures_iter(script)
        .filter_map(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string()))
        .collect()
}

/// Validates a rule before saving: JS syntax + literal JSONPaths.
/// `return` is valid in the script (handler phase), so we wrap it in a function.
pub fn validate_rule_script(script: &str) -> Result<(), String> {
    let rt = Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    let ctx = Context::full(&rt).map_err(|e| format!("context: {e}"))?;
    ctx.with(|c| {
        let src = format!("(function() {{\n{script}\n}})");
        match c.eval::<rquickjs::Value, _>(src) {
            Ok(_) => Ok(()),
            Err(_) => {
                let caught = c.catch();
                let msg = match caught.into_exception() {
                    Some(ex) => ex.message().unwrap_or_else(|| ex.to_string()),
                    None => "syntax error".to_string(),
                };
                Err(format!("JS: {msg}"))
            }
        }
    })?;
    for path in extract_path_literals(script) {
        if let Some(err) = crate::jsonpath::validate(&path) {
            return Err(format!("JSONPath \"{path}\": {err}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(script: &str, input: &str) -> ScriptResult {
        let client = spawn_engine(Duration::from_millis(500), Arc::new(|_: &str| None));
        client.run(String::new(), script.to_string(), input.to_string()).await
    }

    #[tokio::test]
    async fn secret_reads_from_resolver_and_missing_is_null() {
        let secrets: SecretFn =
            Arc::new(|name| (name == "TOKEN").then(|| "s3cr3t".to_string()));
        let client = spawn_engine(Duration::from_millis(500), secrets);
        let res = client
            .run(
                String::new(),
                "request.tok = secret('TOKEN'); request.miss = secret('NOPE');".into(),
                r#"{"request":{}}"#.into(),
            )
            .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["tok"], "s3cr3t");
        assert!(req["miss"].is_null());
    }

    #[tokio::test]
    async fn notify_collects_notifications() {
        let res = run(
            "notify('hello', { channel: 'ops', title: 'T' }); notify('plain');",
            r#"{"request":{}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.notifications.len(), 2);
        assert_eq!(res.notifications[0]["text"], "hello");
        assert_eq!(res.notifications[0]["channel"], "ops");
        assert_eq!(res.notifications[0]["title"], "T");
        assert_eq!(res.notifications[1]["text"], "plain");
    }

    #[tokio::test]
    async fn handler_supports_secret_and_notify() {
        let secrets: SecretFn = Arc::new(|_| Some("tok".to_string()));
        let res = tokio::task::spawn_blocking(move || {
            execute_handler(
                "",
                "notify('from handler'); return { status: 200, headers: {}, body: secret('X') };",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                secrets,
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        assert_eq!(res.response.unwrap()["body"], "tok");
        assert_eq!(res.notifications.len(), 1);
        assert_eq!(res.notifications[0]["text"], "from handler");
    }

    #[tokio::test]
    async fn trace_records_patch_ops() {
        let res = run(
            "patch(request, 'a', 1); tryPatch(request, 'zzz', 2);",
            r#"{"request":{"headers":{},"body":"{\"a\":0}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.trace.len(), 2);
        assert_eq!(res.trace[0]["op"], "patch");
        assert_eq!(res.trace[0]["nodes"], 1);
        assert_eq!(res.trace[1]["op"], "tryPatch");
        assert_eq!(res.trace[1]["nodes"], 0);
    }

    #[tokio::test]
    async fn handler_trace_records_send() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = upstream.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b).await;
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
        });
        let input = format!(
            r#"{{"request":{{"method":"GET","url":"http://{addr}/","headers":{{}},"body":""}}}}"#
        );
        let res = tokio::task::spawn_blocking(move || {
            execute_handler("", "return send(request);", &input, Duration::from_secs(5), Arc::new(|_: &str| None))
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        assert_eq!(res.trace.len(), 1);
        assert_eq!(res.trace[0]["op"], "send");
        assert_eq!(res.trace[0]["status"], 200);
        assert!(res.trace[0]["ms"].is_number());
    }

    #[tokio::test]
    async fn script_mutates_request_header() {
        let res = run(
            "request.headers['X-Debug'] = '1';",
            r#"{"request":{"headers":{},"url":"http://x/"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue");
        let req = res.request.unwrap();
        assert_eq!(req["headers"]["X-Debug"], "1");
    }

    #[tokio::test]
    async fn stdlib_helpers_are_available() {
        // Guards against a syntax error in STD_LIB (which would break every rule).
        let res = run(
            "setJsonBody(request, { a: (jsonBody(request) || {}).a, injected: true });",
            r#"{"request":{"headers":{},"body":"{\"a\":1}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        let body: serde_json::Value = serde_json::from_str(req["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["a"], 1);
        assert_eq!(body["injected"], true);
    }

    #[tokio::test]
    async fn stdlib_header_helpers_case_insensitive() {
        let res = run(
            "request.__found = header(request, 'CONTENT-TYPE');",
            r#"{"request":{"headers":{"Content-Type":"application/json"}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue");
        assert_eq!(res.request.unwrap()["__found"], "application/json");
    }

    #[tokio::test]
    async fn stdlib_base64_and_hashes() {
        let res = run(
            "request.b64 = base64Encode('hello');\n\
             request.plain = base64Decode('aGVsbG8=');\n\
             request.url64 = base64Decode('Pj4-Pz8_');\n\
             request.sha = sha256('abc');\n\
             request.md = md5('abc');\n\
             request.mac = hmacSha256('Jefe', 'what do ya want for nothing?');",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["b64"], "aGVsbG8=");
        assert_eq!(req["plain"], "hello");
        assert_eq!(req["url64"], ">>>???");
        assert_eq!(req["sha"], "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(req["md"], "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(req["mac"], "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[tokio::test]
    async fn stdlib_base64_decode_invalid_throws() {
        let res = run("base64Decode('!!!');", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("base64Decode"));
    }

    #[tokio::test]
    async fn stdlib_jwt_decode() {
        // {"alg":"HS256","typ":"JWT"} . {"sub":"42","name":"Ann"} . <fake sig>
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI0MiIsIm5hbWUiOiJBbm4ifQ.sig";
        let script = format!(
            "var t = jwtDecode('Bearer {jwt}');\n\
             request.alg = t.header.alg; request.sub = t.payload.sub; request.nm = t.payload.name;"
        );
        let res = run(&script, r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["alg"], "HS256");
        assert_eq!(req["sub"], "42");
        assert_eq!(req["nm"], "Ann");
    }

    #[tokio::test]
    async fn stdlib_jwt_decode_malformed_throws() {
        let res = run("jwtDecode('not-a-jwt');", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("jwtDecode"));
    }

    #[tokio::test]
    async fn stdlib_request_cookies_read_write_remove() {
        let res = run(
            "request.all = cookies(request);\n\
             request.one = cookie(request, 'b');\n\
             setCookie(request, 'c', 'x y');\n\
             removeCookie(request, 'a');",
            r#"{"request":{"headers":{"Cookie":"a=1; b=two"}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["all"]["a"], "1");
        assert_eq!(req["all"]["b"], "two");
        assert_eq!(req["one"], "two");
        let header = req["headers"]["Cookie"].as_str().unwrap();
        assert!(!header.contains("a=1"), "cookie a removed: {header}");
        assert!(header.contains("b=two"));
        assert!(header.contains("c=x%20y"));
    }

    #[tokio::test]
    async fn stdlib_request_remove_last_cookie_drops_header() {
        let res = run(
            "removeCookie(request, 'only');",
            r#"{"request":{"headers":{"Cookie":"only=1"}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert!(res.request.unwrap()["headers"]["Cookie"].is_null());
    }

    #[tokio::test]
    async fn stdlib_response_set_cookie_with_attrs() {
        let res = run(
            "response.before = cookies(response);\n\
             setCookie(response, 'sid', 'abc', { path: '/', maxAge: 60, httpOnly: true, secure: true, sameSite: 'Lax' });",
            r#"{"request":{"headers":{}},"response":{"status":200,"headers":{"Set-Cookie":"old=v; Path=/"},"body":""}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let resp = res.response.unwrap();
        assert_eq!(resp["before"]["old"], "v");
        let sc = resp["headers"]["Set-Cookie"].as_str().unwrap();
        assert!(sc.starts_with("sid=abc"), "sc: {sc}");
        assert!(sc.contains("Path=/"));
        assert!(sc.contains("Max-Age=60"));
        assert!(sc.contains("HttpOnly"));
        assert!(sc.contains("Secure"));
        assert!(sc.contains("SameSite=Lax"));
    }

    #[tokio::test]
    async fn stdlib_response_remove_cookie_expires_it() {
        let res = run(
            "removeCookie(response, 'sid');",
            r#"{"request":{"headers":{}},"response":{"status":200,"headers":{},"body":""}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let sc = res.response.unwrap()["headers"]["Set-Cookie"].as_str().unwrap().to_string();
        assert!(sc.starts_with("sid=;"), "sc: {sc}");
        assert!(sc.contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn stdlib_form_helpers_roundtrip() {
        let res = run(
            "request.form = formBody(request);\n\
             request.q = formParam(request, 'q');\n\
             setFormParam(request, 'page', '2');\n\
             request.after = formBody(request);",
            r#"{"request":{"headers":{"Content-Type":"application/x-www-form-urlencoded"},"body":"q=a+b&tag=x%26y"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["form"]["q"], "a b");
        assert_eq!(req["form"]["tag"], "x&y");
        assert_eq!(req["q"], "a b");
        assert_eq!(req["after"]["page"], "2");
        assert_eq!(req["after"]["q"], "a b");
    }

    #[tokio::test]
    async fn stdlib_set_form_body_encodes_and_sets_content_type() {
        let res = run(
            "setFormBody(request, { user: 'ann smith', n: 2 });",
            r#"{"request":{"headers":{},"body":""}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["body"], "user=ann%20smith&n=2");
        assert_eq!(req["headers"]["Content-Type"], "application/x-www-form-urlencoded");
    }

    #[tokio::test]
    async fn stdlib_fake_data_generators() {
        let res = run(
            "request.f = randomFloat(1, 2);\n\
             request.b = randomBool(1);\n\
             request.nb = randomBool(0);\n\
             request.name = fakeName();\n\
             request.mail = fakeEmail();\n\
             request.phone = fakePhone();\n\
             request.text = lorem(5);\n\
             request.list = fakeList(3, function(i) { return { n: i }; });",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        let f = req["f"].as_f64().unwrap();
        assert!((1.0..2.0).contains(&f));
        assert_eq!(req["b"], true);
        assert_eq!(req["nb"], false);
        assert!(req["name"].as_str().unwrap().contains(' '));
        let mail = req["mail"].as_str().unwrap();
        assert!(mail.contains('@') && mail.contains('.'), "mail: {mail}");
        let phone = req["phone"].as_str().unwrap();
        assert!(phone.chars().filter(|c| c.is_ascii_digit()).count() >= 7, "phone: {phone}");
        assert_eq!(req["text"].as_str().unwrap().split(' ').count(), 5);
        assert_eq!(req["list"].as_array().unwrap().len(), 3);
        assert_eq!(req["list"][2]["n"], 2);
    }

    #[tokio::test]
    async fn stdlib_counter_persists_across_runs_and_resets() {
        let client = spawn_engine(Duration::from_millis(500), Arc::new(|_: &str| None));
        let script = "request.n = counter('__t_counter_runs');".to_string();
        let r1 = client.run(String::new(), script.clone(), r#"{"request":{}}"#.into()).await;
        let r2 = client.run(String::new(), script, r#"{"request":{}}"#.into()).await;
        assert_eq!(r1.request.unwrap()["n"], 1, "err: {:?}", r1.error);
        assert_eq!(r2.request.unwrap()["n"], 2);
        let r3 = client
            .run(
                String::new(),
                "resetCounter('__t_counter_runs'); request.n = counter('__t_counter_runs');".into(),
                r#"{"request":{}}"#.into(),
            )
            .await;
        assert_eq!(r3.request.unwrap()["n"], 1);
    }

    #[tokio::test]
    async fn stdlib_counter_shared_between_handler_runs() {
        for expected in 1..=2 {
            let res = tokio::task::spawn_blocking(move || {
                execute_handler(
                    "",
                    "return { status: 200, headers: {}, body: String(counter('__t_counter_handler')) };",
                    r#"{"request":{}}"#,
                    Duration::from_secs(5),
                    Arc::new(|_: &str| None),
                )
            })
            .await
            .unwrap();
            assert_eq!(res.action, "respond", "err: {:?}", res.error);
            assert_eq!(res.response.unwrap()["body"], expected.to_string());
        }
        crate::script_state::global().reset("__t_counter_handler");
    }

    #[tokio::test]
    async fn stdlib_once_and_every_nth() {
        let client = spawn_engine(Duration::from_millis(500), Arc::new(|_: &str| None));
        let script = "request.o = once('__t_once'); request.e = everyNth('__t_nth', 3);".to_string();
        let mut seen = Vec::new();
        for _ in 0..3 {
            let r = client.run(String::new(), script.clone(), r#"{"request":{}}"#.into()).await;
            let req = r.request.unwrap();
            seen.push((req["o"].as_bool().unwrap(), req["e"].as_bool().unwrap()));
        }
        assert_eq!(seen, vec![(true, false), (false, false), (false, true)]);
    }

    #[tokio::test]
    async fn stdlib_variables_wrap_env() {
        let res = run(
            "request.v = getVariable('token');\n\
             request.missing = getVariable('nope');\n\
             request.fb = getVariable('nope', 'dflt');\n\
             setVariable('fresh', 'val');\n\
             deleteVariable('token');",
            r#"{"request":{},"env":{"token":"abc"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["v"], "abc");
        assert!(req["missing"].is_null());
        assert_eq!(req["fb"], "dflt");
        let env = res.env.unwrap();
        assert_eq!(env["fresh"], "val");
        assert!(env.get("token").is_none() || env["token"].is_null());
    }

    #[tokio::test]
    async fn script_can_request_breakpoint() {
        let res = run("ctx.breakpoint();", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "breakpoint");
    }

    #[tokio::test]
    async fn script_can_mock_response() {
        let res = run(
            r#"ctx.mock({ status: 200, body: '{"ok":true}' });"#,
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "mock");
        assert_eq!(res.mock.unwrap()["status"], 200);
    }

    #[tokio::test]
    async fn thrown_error_becomes_error_result() {
        let res = run("throw new Error('boom');", r#"{"request":{}}"#).await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("boom"));
    }

    #[tokio::test]
    async fn error_message_contains_user_script_line() {
        let res = run(
            "var a = 1;\nvar b = 2;\nthrow new Error('boom');",
            r#"{"request":{}}"#,
        )
        .await;
        assert_eq!(res.action, "error");
        let msg = res.error.unwrap();
        assert!(msg.contains("boom"));
        assert!(msg.contains("(line 3)"), "msg: {msg}");
    }

    #[tokio::test]
    async fn syntax_error_becomes_error_result() {
        let res = run("this is not valid )(", r#"{"request":{}}"#).await;
        assert_eq!(res.action, "error");
    }

    #[tokio::test]
    async fn infinite_loop_times_out() {
        let client = spawn_engine(Duration::from_millis(150), Arc::new(|_: &str| None));
        let res = client
            .run(String::new(), "while(true){}".into(), r#"{"request":{}}"#.into())
            .await;
        assert_eq!(res.action, "error");
    }

    #[tokio::test]
    async fn handler_send_returns_upstream_response() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = upstream.accept().await.unwrap();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut b = [0u8; 1024];
                    let _ = s.read(&mut b).await;
                    let _ = s
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\npong")
                        .await;
                });
            }
        });

        let input = format!(
            r#"{{"request":{{"method":"GET","url":"http://{addr}/","headers":{{}},"body":""}}}}"#
        );
        let res = tokio::task::spawn_blocking(move || {
            execute_handler(
                "",
                "return send(request);",
                &input,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();

        assert_eq!(res.action, "respond");
        let resp = res.response.unwrap();
        assert_eq!(resp["status"], 200);
        assert_eq!(resp["body"], "pong");
    }

    #[tokio::test]
    async fn handler_can_return_synthetic_response() {
        let res = tokio::task::spawn_blocking(|| {
            execute_handler(
                "",
                "return { status: 201, headers: {}, body: 'hi' };",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond");
        assert_eq!(res.response.unwrap()["status"], 201);
    }

    #[tokio::test]
    async fn script_reads_and_writes_env() {
        let res = run(
            "env.NEW = env.SEED + '!';",
            r#"{"request":{"headers":{}},"env":{"SEED":"hi"}}"#,
        )
        .await;
        let env = res.env.unwrap();
        assert_eq!(env["SEED"], "hi", "env is read");
        assert_eq!(env["NEW"], "hi!", "env is written and returned");
    }

    #[tokio::test]
    async fn handler_without_return_is_error() {
        let res = tokio::task::spawn_blocking(|| {
            execute_handler(
                "",
                "const x = 1;",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "error");
    }

    #[tokio::test]
    async fn patch_sets_value_in_all_array_elements() {
        let res = run(
            "patch(request, 'items[*].price', 0);",
            r#"{"request":{"headers":{},"body":"{\"items\":[{\"price\":1},{\"price\":2}]}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["price"], 0);
        assert_eq!(body["items"][1]["price"], 0);
    }

    #[tokio::test]
    async fn patch_applies_modifier_function() {
        let res = run(
            "patch(request, 'items[*].price', function(p) { return p * 2; });",
            r#"{"request":{"headers":{},"body":"{\"items\":[{\"price\":10},{\"price\":20}]}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["price"], 20);
        assert_eq!(body["items"][1]["price"], 40);
    }

    #[tokio::test]
    async fn patch_zero_matches_is_error_with_shape() {
        let res = run(
            "patch(request, 'nope[*].x', 1);",
            r#"{"request":{"headers":{},"body":"{\"items\":[1,2],\"total\":2}"}}"#,
        )
        .await;
        assert_eq!(res.action, "error");
        let msg = res.error.unwrap();
        assert!(msg.contains("0 nodes"), "msg: {msg}");
        assert!(msg.contains("items[2]"), "msg should contain the body structure: {msg}");
    }

    #[tokio::test]
    async fn try_patch_zero_matches_is_ok() {
        let res = run(
            "request.__n = tryPatch(request, 'nope', 1);",
            r#"{"request":{"headers":{},"body":"{\"a\":1}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.request.unwrap()["__n"], 0);
    }

    #[tokio::test]
    async fn patch_works_on_plain_object_and_filter() {
        let res = run(
            "var doc = { items: [ { t: 'a', p: 1 }, { t: 'b', p: 2 } ] };\
             patch(doc, \"items[?@.t=='b'].p\", 99); request.__p = doc.items[1].p;",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.request.unwrap()["__p"], 99);
    }

    #[tokio::test]
    async fn patch_at_root_replaces_whole_body() {
        let res = run(
            "patch(request, '$', { replaced: true });",
            r#"{"request":{"headers":{},"body":"{\"a\":1}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["replaced"], true);
    }

    #[tokio::test]
    async fn patch_handles_unicode_keys_and_deep_scan() {
        let res = run(
            "patch(request, \"$..['цена']\", 5);",
            r#"{"request":{"headers":{},"body":"{\"товар\":{\"цена\":1},\"вложено\":{\"товар\":{\"цена\":2}}}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["товар"]["цена"], 5);
        assert_eq!(body["вложено"]["товар"]["цена"], 5);
    }

    #[tokio::test]
    async fn handler_patch_on_send_result() {
        // __native_jsonpath_locate must also exist in the handler engine; send can't be natively
        // mocked, so we patch a synthetic response instead.
        let res = tokio::task::spawn_blocking(|| {
            execute_handler(
                "",
                "var r = { status: 200, headers: {}, body: JSON.stringify({ items: [{ x: 1 }] }) };\
                 patch(r, 'items[*].x', 7); return r;",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.response.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["x"], 7);
    }

    #[tokio::test]
    async fn patch_at_root_on_plain_object_replaces_in_place() {
        let res = run(
            "var doc = { a: 1 }; patch(doc, '$', { b: 2 }); request.__b = doc.b; request.__a = doc.a;",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["__b"], 2);
        assert!(req["__a"].is_null());
    }

    #[tokio::test]
    async fn pick_collects_values_pick_one_first() {
        let res = run(
            "request.__ids = pick(request, 'items[*].id'); request.__first = pickOne(request, 'items[*].id'); request.__none = pickOne(request, 'nope');",
            r#"{"request":{"headers":{},"body":"{\"items\":[{\"id\":10},{\"id\":20}]}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["__ids"], json!([10, 20]));
        assert_eq!(req["__first"], 10);
        assert!(req["__none"].is_null());
    }

    #[tokio::test]
    async fn remove_at_deletes_from_arrays_and_objects() {
        let res = run(
            "request.__n = removeAt(request, 'items[?@.drop==true]'); removeAt(request, 'meta.secret');",
            r#"{"request":{"headers":{},"body":"{\"items\":[{\"drop\":true},{\"drop\":false},{\"drop\":true}],\"meta\":{\"secret\":1,\"keep\":2}}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        let body: Value = serde_json::from_str(req["body"].as_str().unwrap()).unwrap();
        assert_eq!(req["__n"], 2);
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["drop"], false);
        assert!(body["meta"].get("secret").is_none());
        assert_eq!(body["meta"]["keep"], 2);
    }

    #[tokio::test]
    async fn merge_at_deep_merges_each_node() {
        let res = run(
            "mergeAt(request, 'items[*]', { flags: { vip: true } });",
            r#"{"request":{"headers":{},"body":"{\"items\":[{\"id\":1,\"flags\":{\"hot\":true}},{\"id\":2}]}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["flags"]["hot"], true, "deep-merge doesn't clobber siblings");
        assert_eq!(body["items"][0]["flags"]["vip"], true);
        assert_eq!(body["items"][1]["flags"]["vip"], true);
    }

    #[tokio::test]
    async fn merge_at_zero_matches_is_error() {
        let res = run(
            "mergeAt(request, 'nope[*]', { a: 1 });",
            r#"{"request":{"headers":{},"body":"{\"x\":1}"}}"#,
        )
        .await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("0 nodes"));
    }

    #[tokio::test]
    async fn set_query_param_adds_and_replaces() {
        let res = run(
            "setQueryParam(request, 'limit', 5); setQueryParam(request, 'q', 'а б');",
            r#"{"request":{"headers":{},"url":"https://h.kz/v1/list?limit=20","host":"h.kz","path":"/v1/list?limit=20"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["path"], "/v1/list?limit=5&q=%D0%B0%20%D0%B1");
        assert_eq!(req["url"], "https://h.kz/v1/list?limit=5&q=%D0%B0%20%D0%B1");
    }

    #[tokio::test]
    async fn remove_query_param_and_last_one_drops_question_mark() {
        let res = run(
            "removeQueryParam(request, 'a'); removeQueryParam(request, 'b');",
            r#"{"request":{"headers":{},"url":"https://h.kz/p?a=1&b=2","host":"h.kz","path":"/p?a=1&b=2"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["path"], "/p");
        assert_eq!(req["url"], "https://h.kz/p");
    }

    #[tokio::test]
    async fn rewrite_host_updates_host_and_url() {
        let res = run(
            "rewriteHost(request, 'staging.h.kz');",
            r#"{"request":{"headers":{},"url":"https://h.kz/p?x=1","host":"h.kz","path":"/p?x=1"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["host"], "staging.h.kz");
        assert_eq!(req["url"], "https://staging.h.kz/p?x=1");
    }

    #[tokio::test]
    async fn rewrite_path_string_and_regex_keep_query() {
        let res = run(
            "rewritePath(request, '/v3/', '/v4/'); rewritePath(request, /adverts/, 'ads');",
            r#"{"request":{"headers":{},"url":"https://h.kz/v3/adverts/rec?limit=1","host":"h.kz","path":"/v3/adverts/rec?limit=1"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["path"], "/v4/ads/rec?limit=1");
        assert_eq!(req["url"], "https://h.kz/v4/ads/rec?limit=1");
    }

    #[tokio::test]
    async fn path_segments_decode_and_skip_query() {
        let res = run(
            "request.__s = pathSegments(request);",
            r#"{"request":{"headers":{},"path":"/v3/adverts/%D0%B0?x=1"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.request.unwrap()["__s"], json!(["v3", "adverts", "а"]));
    }

    #[tokio::test]
    async fn json_mocks_in_request_phase() {
        let res = run("json(404, { err: 'nope' });", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "mock", "err: {:?}", res.error);
        let m = res.mock.unwrap();
        assert_eq!(m["status"], 404);
        assert_eq!(m["headers"]["content-type"], "application/json");
        assert_eq!(m["body"], "{\"err\":\"nope\"}");
    }

    #[tokio::test]
    async fn json_default_status_200() {
        let res = run("json({ ok: true });", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "mock");
        assert_eq!(res.mock.unwrap()["status"], 200);
    }

    #[tokio::test]
    async fn json_returns_response_in_handler_phase() {
        let res = tokio::task::spawn_blocking(|| {
            execute_handler(
                "",
                "return json(201, { created: true });",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        let r = res.response.unwrap();
        assert_eq!(r["status"], 201);
        assert_eq!(r["body"], "{\"created\":true}");
    }

    #[tokio::test]
    async fn text_response_and_http_error() {
        let res = run("textResponse(503, 'busy');", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "mock");
        let m = res.mock.unwrap();
        assert_eq!(m["status"], 503);
        assert_eq!(m["body"], "busy");
        assert_eq!(m["headers"]["content-type"], "text/plain; charset=utf-8");

        let res = run("httpError(500, 'взрыв');", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "mock");
        let m = res.mock.unwrap();
        assert_eq!(m["status"], 500);
        assert!(m["body"].as_str().unwrap().contains("взрыв"));
    }

    #[tokio::test]
    async fn delay_outside_handler_throws_clear_error() {
        let res = run("delay(10);", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("handler"));
    }

    #[tokio::test]
    async fn delay_works_in_handler() {
        let res = tokio::task::spawn_blocking(|| {
            execute_handler(
                "",
                "delay(5); return json({ ok: 1 });",
                r#"{"request":{}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
    }

    #[tokio::test]
    async fn uuid_and_random_helpers() {
        let res = run(
            "request.__u = uuid(); request.__r = randomInt(3, 5); request.__f = randomFrom(['a']);",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        let u = req["__u"].as_str().unwrap();
        assert_eq!(u.len(), 36);
        assert_eq!(u.as_bytes()[14], b'4', "uuid v4: {u}");
        let r = req["__r"].as_i64().unwrap();
        assert!((3..=5).contains(&r));
        assert_eq!(req["__f"], "a");
    }

    #[tokio::test]
    async fn now_iso_formats_shift_and_tz() {
        let res = run(
            "request.__z = nowISO(); request.__p = nowISO('+2d', '+05:00'); request.__m = nowISO('-30m');",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        let z = req["__z"].as_str().unwrap();
        assert!(z.ends_with('Z') && z.len() == 20, "UTC ISO: {z}");
        let p = req["__p"].as_str().unwrap();
        assert!(p.ends_with("+05:00"), "tz suffix: {p}");
        let m = req["__m"].as_str().unwrap();
        assert!(m.ends_with('Z'));
    }

    #[tokio::test]
    async fn now_iso_rejects_bad_shift() {
        let res = run("nowISO('через день');", r#"{"request":{"headers":{}}}"#).await;
        assert_eq!(res.action, "error");
        assert!(res.error.unwrap().contains("nowISO"));
    }

    #[tokio::test]
    async fn collection_helpers() {
        let res = run(
            "var xs = [ { t: 'a', v: 3 }, { t: 'b', v: 1 }, { t: 'a', v: 2 } ];\
             request.__g = groupBy(xs, 't');\
             request.__s = sortBy(xs, 'v').map(function(x){ return x.v; });\
             request.__u = uniqBy(xs, 't').length;\
             request.__c = chunk([1,2,3,4,5], 2);\
             request.__sm = sample([1,2,3], 2).length;\
             request.__orig = xs[0].v;",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let req = res.request.unwrap();
        assert_eq!(req["__g"]["a"].as_array().unwrap().len(), 2);
        assert_eq!(req["__g"]["b"].as_array().unwrap().len(), 1);
        assert_eq!(req["__s"], json!([1, 2, 3]));
        assert_eq!(req["__u"], 2);
        assert_eq!(req["__c"], json!([[1, 2], [3, 4], [5]]));
        assert_eq!(req["__sm"], 2);
        assert_eq!(req["__orig"], 3, "sortBy doesn't mutate the original array");
    }

    #[tokio::test]
    async fn collection_helpers_accept_fn_key() {
        let res = run(
            "request.__g = groupBy([1, 2, 3, 4], function(x){ return x % 2 ? 'odd' : 'even'; });",
            r#"{"request":{"headers":{}}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        assert_eq!(res.request.unwrap()["__g"]["odd"], json!([1, 3]));
    }

    #[test]
    fn extract_path_literals_finds_second_string_arg() {
        let script = r#"
            patch(res, 'items[*].price', 0);
            tryPatch(res, "a.b", 1);
            pick(res, 'x');
            other('not.a.path');
            patch(res, dynamicPath, 1); // not a literal — skipped
        "#;
        assert_eq!(extract_path_literals(script), vec!["items[*].price", "a.b", "x"]);
    }

    #[test]
    fn validate_rule_script_ok() {
        assert!(validate_rule_script("const r = send(request);\npatch(r, 'items[*].x', 1);\nreturn r;").is_ok());
    }

    #[test]
    fn validate_rule_script_js_syntax_error() {
        let err = validate_rule_script("this is not ) valid").unwrap_err();
        assert!(err.contains("JS"), "err: {err}");
    }

    #[test]
    fn validate_rule_script_bad_jsonpath() {
        let err = validate_rule_script("patch(request, '$[', 1);").unwrap_err();
        assert!(err.contains("JSONPath"), "err: {err}");
    }

    #[tokio::test]
    async fn set_json_body_invalidates_doc_cache() {
        let res = run(
            "pickOne(request, 'a'); setJsonBody(request, { a: 2, b: 9 }); patch(request, 'a', 5);",
            r#"{"request":{"headers":{},"body":"{\"a\":1}"}}"#,
        )
        .await;
        assert_eq!(res.action, "continue", "err: {:?}", res.error);
        let body: Value = serde_json::from_str(res.request.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["a"], 5, "patch applied on top of the manual setJsonBody, not a stale cache");
        assert_eq!(body["b"], 9, "manual setJsonBody's sibling key survived (not clobbered by stale cache)");
    }

    #[tokio::test]
    async fn handler_with_send_replays_canned_response() {
        let canned = r#"{"status":200,"headers":{"content-type":"application/json"},"body":"{\"items\":[{\"x\":1}]}"}"#;
        let canned = canned.to_string();
        let res = tokio::task::spawn_blocking(move || {
            execute_handler_with_send(
                "",
                "var r = send(request); patch(r, 'items[*].x', 9); return r;",
                r#"{"request":{"method":"GET","url":"https://real.example/api","headers":{},"body":""}}"#,
                Duration::from_secs(5),
                Arc::new(|_: &str| None),
                Arc::new(move |_req: &str| canned.clone()),
                Arc::new(crate::script_state::ScriptState::default()),
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.response.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["x"], 9, "send() returned the replay, patch was applied");
    }
}
