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
    /// Изменённый скриптом env (пишется обратно в проект).
    #[serde(default)]
    pub env: Option<serde_json::Value>,
    /// notify(...) calls collected during the run.
    #[serde(default)]
    pub notifications: Vec<serde_json::Value>,
    /// Трасса операций stdlib/send за прогон правила.
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

/// Клиент к движку скриптов. Клонируемый, Send+Sync — годится для прокси-хендлера.
#[derive(Clone)]
pub struct ScriptClient {
    tx: mpsc::UnboundedSender<ScriptJob>,
}

impl ScriptClient {
    /// Прогоняет `script` (с приложенным `prelude`) над контекстом `input_json`.
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

/// Поднимает движок на выделенном потоке (QuickJS-рантайм не Send через await).
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
                let sfn = secrets.clone();
                let f = Function::new(c.clone(), move |name: String| -> Option<String> {
                    sfn(&name)
                })
                .expect("bind secret fn");
                c.globals().set("__native_secret", f).expect("set secret fn");
                let jp = Function::new(c.clone(), |doc: String, path: String| -> String {
                    crate::jsonpath::locate(&doc, &path)
                })
                .expect("bind jsonpath fn");
                c.globals().set("__native_jsonpath_locate", jp).expect("set jsonpath fn");
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
    if (__ln && (__ln - {offset}) > 0) {{ __m += " (строка " + (__ln - {offset}) + ")"; }}
    return JSON.stringify({{ action: "error", error: __m, trace: (typeof ctx !== "undefined" && ctx.__trace) || [] }});
  }}
}})()
"#
    )
}

// ── Handler-режим: скрипт сам выполняет запрос через блокирующий send() ──

/// Ленивая инициализация блокирующего клиента. Создаётся при первом вызове
/// внутри blocking-контекста (не в async-рантайме) — иначе reqwest паникует.
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

/// Выполняет реальный HTTP-запрос (блокирующе) и возвращает {status,headers,body} как JSON.
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
                // тело уже распаковано reqwest — не тащим согласованность-ломающие заголовки
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
      return JSON.stringify({{ action: "error", error: "handler не вернул ответ (нужен return response)", env: ctx.env, notifications: ctx.__notifications, trace: ctx.__trace }});
    }}
    return JSON.stringify({{ action: "respond", response: __out, env: ctx.env, notifications: ctx.__notifications, trace: ctx.__trace }});
  }} catch (e) {{
    var __m = String((e && e.message) || e);
    var __ln = e && e.lineNumber;
    if (!__ln && e && e.stack) {{ var __sm = String(e.stack).match(/:(\d+)/); if (__sm) __ln = Number(__sm[1]); }}
    if (__ln && (__ln - {offset}) > 0) {{ __m += " (строка " + (__ln - {offset}) + ")"; }}
    return JSON.stringify({{ action: "error", error: __m, trace: (typeof ctx !== "undefined" && ctx.__trace) || [] }});
  }}
}})()
"#
    )
}

/// Реализация send() для handler-движка (подменяется в dry-run на реплей).
pub type SendFn = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// Синхронно исполняет handler-скрипт: он сам делает send()/sleep() и возвращает ответ.
/// Вызывать вне tokio-рантайма (через spawn_blocking).
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
    )
}

/// Как `execute_handler`, но с подменяемой реализацией send() — используется
/// dry-run'ом, чтобы реплеить захваченный ответ вместо реального похода в сеть.
pub fn execute_handler_with_send(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
    send_impl: SendFn,
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
        let sfn = secrets.clone();
        let secret_fn = match Function::new(c.clone(), move |name: String| -> Option<String> {
            sfn(&name)
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind secret: {e}")),
        };
        let _ = g.set("__native_secret", secret_fn);
        let jp_fn = match Function::new(c.clone(), |doc: String, path: String| -> String {
            crate::jsonpath::locate(&doc, &path)
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind jsonpath: {e}")),
        };
        let _ = g.set("__native_jsonpath_locate", jp_fn);

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

/// Одноразовый прогон request/response-скрипта в свежем рантайме (dry-run,
/// без общего потока движка). Сеть не используется.
pub fn execute_once(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
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
        let sfn = secrets.clone();
        if let Ok(f) = Function::new(c.clone(), move |name: String| -> Option<String> { sfn(&name) }) {
            let _ = g.set("__native_secret", f);
        }
        if let Ok(f) = Function::new(c.clone(), |doc: String, path: String| -> String {
            crate::jsonpath::locate(&doc, &path)
        }) {
            let _ = g.set("__native_jsonpath_locate", f);
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

/// Литеральные JSONPath-аргументы path-функций (2-й аргумент — строка в '…' или "…").
/// Список функций синхронизирован с src/scripting/pathContext.ts.
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

/// Проверка правила перед сохранением: синтаксис JS + литеральные JSONPath.
/// `return` в скрипте валиден (handler-фаза), поэтому оборачиваем в функцию.
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
                    None => "синтаксическая ошибка".to_string(),
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
        assert!(msg.contains("(строка 3)"), "msg: {msg}");
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
        assert_eq!(env["SEED"], "hi", "env читается");
        assert_eq!(env["NEW"], "hi!", "env пишется и возвращается");
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
        assert!(msg.contains("0 узлов"), "msg: {msg}");
        assert!(msg.contains("items[2]"), "msg должен содержать структуру тела: {msg}");
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
        // __native_jsonpath_locate должен быть и в handler-движке; send мокается нативно нельзя,
        // поэтому патчим синтетический ответ.
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
        assert_eq!(body["items"][0]["flags"]["hot"], true, "deep-merge не затирает соседей");
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
        assert!(res.error.unwrap().contains("0 узлов"));
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
        assert!(p.ends_with("+05:00"), "tz-суффикс: {p}");
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
        assert_eq!(req["__orig"], 3, "sortBy не мутирует исходный массив");
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
            patch(res, dynamicPath, 1); // не литерал — пропускаем
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
            )
        })
        .await
        .unwrap();
        assert_eq!(res.action, "respond", "err: {:?}", res.error);
        let body: Value =
            serde_json::from_str(res.response.unwrap()["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["x"], 9, "send() отдал реплей, patch применился");
    }
}
