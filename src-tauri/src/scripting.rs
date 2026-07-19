use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::{Context, Ctx, Runtime};
use serde::{Deserialize, Serialize};
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
    const request = ctx.request;
    const response = ctx.response;
    /* ── library ── */
    {prelude}
    /* ── rule script ── */
    {script}
    return JSON.stringify({{
      action: ctx.__action,
      request: ctx.request,
      response: ctx.response,
      mock: ctx.__mock || null,
      reason: ctx.__reason || null
    }});
  }} catch (e) {{
    return JSON.stringify({{ action: "error", error: String((e && e.message) || e) }});
  }}
}})()
"#
    )
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
}
