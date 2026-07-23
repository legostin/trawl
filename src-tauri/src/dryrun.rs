//! Dry-run a rule against a captured flow: no network access (send replays the
//! captured response), no persistence. Used by the Tauri command test_rule and MCP.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::model::Flow;
use crate::scripting::{self, ScriptResult};

fn headers_json(headers: &[(String, String)]) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in headers {
        m.insert(k.clone(), Value::String(v.clone()));
    }
    Value::Object(m)
}

fn body_text(body: &[u8]) -> String {
    String::from_utf8_lossy(body).to_string()
}

/// Runs a script (phase: request|response|handler) against a captured flow.
pub fn run(
    flow: &Flow,
    script: &str,
    phase: &str,
    prelude: &str,
    env: Value,
    timeout: Duration,
) -> Value {
    let req = json!({
        "method": flow.method,
        "url": format!("{}://{}{}", flow.url.scheme, flow.url.host, flow.url.path),
        "host": flow.url.host,
        "path": flow.url.path,
        "headers": headers_json(&flow.request.headers),
        "body": body_text(&flow.request.body),
    });
    let resp = flow.response.as_ref().map(|r| {
        json!({ "status": r.status, "headers": headers_json(&r.headers), "body": body_text(&r.body) })
    });
    let before = resp.clone().unwrap_or(Value::Null);

    let res: ScriptResult = match phase {
        "handler" => {
            let canned = resp.clone().unwrap_or(json!({"status":0,"headers":{},"body":""})).to_string();
            scripting::execute_handler_with_send(
                prelude,
                script,
                &json!({ "request": req, "env": env }).to_string(),
                timeout,
                Arc::new(|_| None),
                Arc::new(move |_req: &str| canned.clone()),
            )
        }
        "response" => scripting::execute_once(
            prelude,
            script,
            &json!({ "request": req, "response": resp, "env": env }).to_string(),
            timeout,
            Arc::new(|_| None),
        ),
        _ => scripting::execute_once(
            prelude,
            script,
            &json!({ "request": req, "env": env }).to_string(),
            timeout,
            Arc::new(|_| None),
        ),
    };

    // after: what will go to the client / server after the rule runs.
    let after = match res.action.as_str() {
        "respond" => res.response.clone(),
        "mock" => res.mock.clone(),
        "continue" if phase == "response" => res.response.clone(),
        "continue" => res.request.clone(),
        _ => None,
    };

    json!({
        "flowId": flow.id,
        "action": res.action,
        "error": res.error,
        "trace": res.trace,
        "before": before,
        "after": after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Flow, HttpMessage, ResponseMessage, UrlParts};

    fn sample_flow() -> Flow {
        let mut f = Flow::new_request(
            1,
            "GET".into(),
            UrlParts { scheme: "https".into(), host: "api.test".into(), port: 443, path: "/v1/items".into() },
            HttpMessage { headers: vec![], body: Vec::new(), body_is_text: true },
        );
        f.response = Some(ResponseMessage {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: br#"{"items":[{"x":1}]}"#.to_vec(),
            body_is_text: true,
        });
        f
    }

    #[test]
    fn dry_run_handler_replays_and_patches() {
        let flow = sample_flow();
        let out = run(
            &flow,
            "var r = send(request); patch(r, 'items[*].x', 9); return r;",
            "handler",
            "",
            serde_json::json!({}),
            Duration::from_secs(5),
        );
        assert_eq!(out["action"], "respond", "err: {:?}", out["error"]);
        let body: serde_json::Value =
            serde_json::from_str(out["after"]["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["x"], 9);
        assert_eq!(out["trace"][1]["op"], "patch");
    }

    #[test]
    fn dry_run_response_phase_continue() {
        let flow = sample_flow();
        let out = run(
            &flow,
            "patch(response, 'items[*].x', 5);",
            "response",
            "",
            serde_json::json!({}),
            Duration::from_secs(5),
        );
        assert_eq!(out["action"], "continue", "err: {:?}", out["error"]);
        let body: serde_json::Value =
            serde_json::from_str(out["after"]["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["items"][0]["x"], 5);
    }
}
