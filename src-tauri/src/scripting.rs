use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{Context, Ctx, Function, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

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
pub fn spawn_engine(timeout: Duration) -> ScriptClient {
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

fn build_source(prelude: &str, script: &str) -> String {
    format!(
        r#"
(function() {{
  try {{
    const ctx = JSON.parse(__input);
    ctx.__action = "continue";
    ctx.mock = function(resp) {{ ctx.__action = "mock"; ctx.__mock = resp; }};
    ctx.abort = function(reason) {{ ctx.__action = "abort"; ctx.__reason = reason || "aborted"; }};
    if (!ctx.env) ctx.env = {{}};
    const request = ctx.request;
    const response = ctx.response;
    const env = ctx.env;
    /* ── library ── */
    {prelude}
    /* ── rule script ── */
    {script}
    return JSON.stringify({{
      action: ctx.__action,
      request: ctx.request,
      response: ctx.response,
      mock: ctx.__mock || null,
      reason: ctx.__reason || null,
      env: ctx.env
    }});
  }} catch (e) {{
    return JSON.stringify({{ action: "error", error: String((e && e.message) || e) }});
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
    format!(
        r#"
(function() {{
  try {{
    const ctx = JSON.parse(__input);
    if (!ctx.env) ctx.env = {{}};
    const request = ctx.request;
    const env = ctx.env;
    function send(req) {{ return JSON.parse(__native_send(JSON.stringify(req || request))); }}
    function sleep(ms) {{ __native_sleep(ms); }}
    {prelude}
    const __out = (function() {{ {script} }})();
    if (__out === undefined || __out === null) {{
      return JSON.stringify({{ action: "error", error: "handler не вернул ответ (нужен return response)", env: ctx.env }});
    }}
    return JSON.stringify({{ action: "respond", response: __out, env: ctx.env }});
  }} catch (e) {{
    return JSON.stringify({{ action: "error", error: String((e && e.message) || e) }});
  }}
}})()
"#
    )
}

/// Синхронно исполняет handler-скрипт: он сам делает send()/sleep() и возвращает ответ.
/// Вызывать вне tokio-рантайма (через spawn_blocking).
pub fn execute_handler(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
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
        let send_fn = match Function::new(c.clone(), move |req: String| -> String {
            native_send(&req)
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(script: &str, input: &str) -> ScriptResult {
        let client = spawn_engine(Duration::from_millis(500));
        client.run(String::new(), script.to_string(), input.to_string()).await
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
    async fn syntax_error_becomes_error_result() {
        let res = run("this is not valid )(", r#"{"request":{}}"#).await;
        assert_eq!(res.action, "error");
    }

    #[tokio::test]
    async fn infinite_loop_times_out() {
        let client = spawn_engine(Duration::from_millis(150));
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
            execute_handler("", "return send(request);", &input, Duration::from_secs(5))
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
            execute_handler("", "const x = 1;", r#"{"request":{}}"#, Duration::from_secs(5))
        })
        .await
        .unwrap();
        assert_eq!(res.action, "error");
    }
}
