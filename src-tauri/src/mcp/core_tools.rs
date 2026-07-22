//! Кор-тулы MCP: определения (имя/описание/схема) и синхронный диспатч.
//! Deps конструируется из AppHandle в сервере и вручную в тестах —
//! поэтому всё здесь тестируется без Tauri.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::commands::AppState;
use crate::db::FlowQuery;
use crate::model::Flow;

pub struct Deps<'a> {
    pub state: &'a AppState,
    pub data_dir: PathBuf,
    pub rules_dir: PathBuf,
}

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn filter_prop() -> Value {
    json!({
        "type": "object",
        "description": "Filter over captured traffic history",
        "properties": {
            "query": { "type": "string", "description": "substring of host+path" },
            "method": { "type": "string" },
            "statusClass": { "type": "string", "description": "2xx | 3xx | 4xx | 5xx | empty" },
            "host": { "type": "string", "description": "exact host" },
            "projectId": { "type": "string" },
            "startTs": { "type": "integer", "description": "unix ms" },
            "endTs": { "type": "integer", "description": "unix ms" }
        }
    })
}

pub fn core_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_status",
            description: "Trawl status: proxy running/address, active project, intercept flag, flow counts.",
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "query_flows",
            description: "Query captured traffic history (SQLite). Returns metadata rows without bodies; use get_flow for full request/response.",
            schema: obj(
                json!({
                    "filter": filter_prop(),
                    "limit": { "type": "integer", "description": "max rows, default 50, cap 500" },
                    "offset": { "type": "integer" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "get_flow",
            description: "Full flow by id from the in-memory capture (recent traffic): headers, bodies, applied rules, timings. Text bodies are truncated to maxBodyBytes.",
            schema: obj(
                json!({
                    "id": { "type": "integer" },
                    "maxBodyBytes": { "type": "integer", "description": "default 50000" }
                }),
                &["id"],
            ),
        },
        ToolDef {
            name: "flow_count",
            description: "Count flows in history matching a filter.",
            schema: obj(json!({ "filter": filter_prop() }), &[]),
        },
        ToolDef {
            name: "aggregate_flows",
            description: "Aggregate history: groupBy host | status | time | duration. bucket = ms for time/duration grouping.",
            schema: obj(
                json!({
                    "filter": filter_prop(),
                    "groupBy": { "type": "string", "enum": ["host", "status", "time", "duration"] },
                    "bucket": { "type": "integer", "description": "bucket size, default 60000" },
                    "limit": { "type": "integer", "description": "default 50" }
                }),
                &[],
            ),
        },
        ToolDef {
            name: "list_rules",
            description: "List rewrite rules (glob pattern over host+path, phase, JS script). Optional projectId filter.",
            schema: obj(json!({ "projectId": { "type": "string" } }), &[]),
        },
        ToolDef {
            name: "save_rule",
            description: "Create or update a rule. Omit rule.id to create (id is generated). phase: request | response | both | handler. Script API: call get_scripting_reference first. Fails if an enabled rule with the same pattern+phase exists.",
            schema: obj(
                json!({
                    "rule": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "name": { "type": "string" },
                            "enabled": { "type": "boolean", "description": "default true" },
                            "pattern": { "type": "string", "description": "glob over host+path, e.g. api.example.com/*" },
                            "phase": { "type": "string", "enum": ["request", "response", "both", "handler"] },
                            "script": { "type": "string" },
                            "projectId": { "type": ["string", "null"] }
                        },
                        "required": ["name", "pattern", "phase", "script"]
                    }
                }),
                &["rule"],
            ),
        },
        ToolDef {
            name: "delete_rule",
            description: "Delete a rule by id.",
            schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
        },
        ToolDef {
            name: "get_scripting_reference",
            description: "Rule scripting reference: ctx API typings, stdlib typings and the shared library source. Read before writing rule scripts.",
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "list_projects",
            description: "List projects (host include/exclude globs, env vars) and the active project id.",
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "save_project",
            description: "Create or update a project. Omit project.id to create.",
            schema: obj(
                json!({
                    "project": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "name": { "type": "string" },
                            "includeHosts": { "type": "array", "items": { "type": "string" } },
                            "excludeHosts": { "type": "array", "items": { "type": "string" } },
                            "env": { "type": "array", "items": { "type": "object", "properties": { "key": { "type": "string" }, "value": { "type": "string" } }, "required": ["key", "value"] } }
                        },
                        "required": ["name"]
                    }
                }),
                &["project"],
            ),
        },
        ToolDef {
            name: "delete_project",
            description: "Delete a project by id.",
            schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
        },
        ToolDef {
            name: "set_active_project",
            description: "Set the active project (null id clears it). Capture and rules are scoped by the active project.",
            schema: obj(json!({ "id": { "type": ["string", "null"] } }), &[]),
        },
        ToolDef {
            name: "list_breakpoints",
            description: "List breakpoint definitions (glob pattern, method, request/response phase).",
            schema: obj(json!({}), &[]),
        },
        ToolDef {
            name: "save_breakpoint",
            description: "Create or update a breakpoint definition. Omit breakpoint.id to create. Fails on a conflicting enabled breakpoint (same pattern+method+phase).",
            schema: obj(
                json!({
                    "breakpoint": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "name": { "type": "string" },
                            "enabled": { "type": "boolean", "description": "default true" },
                            "pattern": { "type": "string", "description": "glob over host+path" },
                            "method": { "type": ["string", "null"], "description": "HTTP method filter, null = any" },
                            "onRequest": { "type": "boolean" },
                            "onResponse": { "type": "boolean" },
                            "projectId": { "type": ["string", "null"] }
                        },
                        "required": ["name", "pattern", "onRequest", "onResponse"]
                    }
                }),
                &["breakpoint"],
            ),
        },
        ToolDef {
            name: "delete_breakpoint",
            description: "Delete a breakpoint definition by id.",
            schema: obj(json!({ "id": { "type": "string" } }), &["id"]),
        },
        ToolDef {
            name: "list_paused",
            description: "Flows currently paused on a breakpoint, with full request/response so you can decide what to edit. Resolve with resolve_breakpoint.",
            schema: obj(json!({ "maxBodyBytes": { "type": "integer", "description": "default 50000" } }), &[]),
        },
        ToolDef {
            name: "resolve_breakpoint",
            description: "Resolve a paused flow. action: execute (forward with edits), respond (answer without forwarding), abort. For execute/respond, `headers` REPLACES the full header list — take it from list_paused and modify. body is a string; bodyBase64 overrides it for binary.",
            schema: obj(
                json!({
                    "flowId": { "type": "integer" },
                    "phase": { "type": "string", "enum": ["request", "response"] },
                    "action": { "type": "string", "enum": ["execute", "respond", "abort"] },
                    "edits": {
                        "type": "object",
                        "properties": {
                            "method": { "type": "string" },
                            "path": { "type": "string", "description": "request path+query (request phase)" },
                            "status": { "type": "integer" },
                            "headers": { "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "string" }] } },
                            "body": { "type": "string" },
                            "bodyBase64": { "type": "string" },
                            "reason": { "type": "string", "description": "abort reason" }
                        }
                    }
                }),
                &["flowId", "phase", "action"],
            ),
        },
        ToolDef {
            name: "send_request",
            description: "Send a one-shot HTTP request (like the UI composer). viaProxy=true routes it through the local proxy so it shows up in the capture.",
            schema: obj(
                json!({
                    "method": { "type": "string" },
                    "url": { "type": "string" },
                    "headers": { "type": "array", "items": { "type": "array", "prefixItems": [{ "type": "string" }, { "type": "string" }] } },
                    "body": { "type": "string" },
                    "bodyB64": { "type": "string", "description": "base64 raw body, overrides body" },
                    "viaProxy": { "type": "boolean", "description": "default false" },
                    "maxBodyBytes": { "type": "integer", "description": "default 50000" }
                }),
                &["method", "url"],
            ),
        },
    ]
}

pub fn dispatch(deps: &Deps, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "get_status" => tool_get_status(deps),
        "query_flows" => tool_query_flows(deps, args),
        "get_flow" => tool_get_flow(deps, args),
        "flow_count" => tool_flow_count(deps, args),
        "aggregate_flows" => tool_aggregate_flows(deps, args),
        "list_rules" => tool_list_rules(deps, args),
        "save_rule" => tool_save_rule(deps, args),
        "delete_rule" => tool_delete_rule(deps, args),
        "get_scripting_reference" => tool_scripting_reference(deps),
        "list_projects" => tool_list_projects(deps),
        "save_project" => tool_save_project(deps, args),
        "delete_project" => tool_delete_project(deps, args),
        "set_active_project" => tool_set_active_project(deps, args),
        "list_breakpoints" => tool_list_breakpoints(deps),
        "save_breakpoint" => tool_save_breakpoint(deps, args),
        "delete_breakpoint" => tool_delete_breakpoint(deps, args),
        "list_paused" => tool_list_paused(deps, args),
        "resolve_breakpoint" => tool_resolve_breakpoint(deps, args),
        "send_request" => tool_send_request(deps, args),
        _ => Err(format!("unknown tool: {name}")),
    }
}

// ── helpers ──

fn u64_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| v.as_u64())
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn parse_filter(args: &Value) -> Result<FlowQuery, String> {
    serde_json::from_value(args.get("filter").cloned().unwrap_or_else(|| json!({})))
        .map_err(|e| format!("bad filter: {e}"))
}

fn reader(deps: &Deps) -> Result<crate::db::Db, String> {
    deps.state.db()?.reader().map_err(|e| e.to_string())
}

fn body_json(headers: &[(String, String)], body: &[u8], is_text: bool, max: usize, status: Option<u16>) -> Value {
    let mut v = if is_text {
        let mut cut = body.len().min(max);
        if cut < body.len() {
            // Don't cut mid-UTF-8 character: continuation bytes have pattern 10xxxxxx
            while cut > 0 && (body[cut] & 0xC0) == 0x80 {
                cut -= 1;
            }
        }
        json!({
            "headers": headers,
            "body": String::from_utf8_lossy(&body[..cut]),
            "bodySize": body.len(),
            "truncated": body.len() > max,
        })
    } else {
        json!({
            "headers": headers,
            "body": Value::Null,
            "binary": true,
            "bodySize": body.len(),
        })
    };
    if let Some(s) = status {
        v["status"] = json!(s);
    }
    v
}

pub fn flow_to_json(flow: &Flow, max_body: usize) -> Value {
    json!({
        "id": flow.id,
        "timestamp": flow.timestamp,
        "method": flow.method,
        "url": serde_json::to_value(&flow.url).unwrap_or(Value::Null),
        "state": serde_json::to_value(&flow.state).unwrap_or(Value::Null),
        "error": flow.error,
        "appliedRules": flow.applied_rules,
        "pausedPhase": flow.paused_phase,
        "timings": serde_json::to_value(&flow.timings).unwrap_or(Value::Null),
        "request": body_json(&flow.request.headers, &flow.request.body, flow.request.body_is_text, max_body, None),
        "response": flow.response.as_ref().map(|r| body_json(&r.headers, &r.body, r.body_is_text, max_body, Some(r.status))),
    })
}

// ── tools ──

fn tool_get_status(deps: &Deps) -> Result<Value, String> {
    let addr = deps.state.proxy.lock().unwrap().as_ref().map(|h| h.local_addr().to_string());
    let active = deps.state.active_project.read().unwrap().clone();
    let db_count = deps
        .state
        .db()
        .ok()
        .and_then(|h| h.reader().ok())
        .and_then(|db| db.count(&FlowQuery::default()).ok());
    Ok(json!({
        "proxyRunning": addr.is_some(),
        "proxyAddr": addr,
        "lanIp": crate::net::lan_ip().map(|ip| ip.to_string()),
        "intercept": *deps.state.intercept.read().unwrap(),
        "activeProject": active.map(|p| json!({ "id": p.id, "name": p.name })),
        "flowsInMemory": deps.state.store.all().len(),
        "flowsInDb": db_count,
    }))
}

fn tool_query_flows(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let limit = u64_arg(args, "limit").unwrap_or(50).min(500) as u32;
    let offset = u64_arg(args, "offset").unwrap_or(0) as u32;
    let rows = reader(deps)?.query(&filter, limit, offset).map_err(|e| e.to_string())?;
    Ok(json!({ "flows": rows, "limit": limit, "offset": offset }))
}

fn tool_get_flow(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = u64_arg(args, "id").ok_or("missing id")?;
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let flow = deps.state.store.get(id).ok_or_else(|| format!("flow {id} not found in memory"))?;
    Ok(flow_to_json(&flow, max))
}

fn tool_flow_count(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let count = reader(deps)?.count(&filter).map_err(|e| e.to_string())?;
    Ok(json!({ "count": count }))
}

fn tool_aggregate_flows(deps: &Deps, args: &Value) -> Result<Value, String> {
    let filter = parse_filter(args)?;
    let group_by = str_arg(args, "groupBy").unwrap_or_else(|| "host".into());
    let bucket = u64_arg(args, "bucket").unwrap_or(60_000);
    let limit = u64_arg(args, "limit").unwrap_or(50) as u32;
    let buckets = reader(deps)?
        .aggregate(&filter, &group_by, bucket, limit)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "buckets": buckets, "groupBy": group_by }))
}

const SCRIPT_API_DTS: &str = include_str!("../../../src/scripting/apiTypes.ts");
const SCRIPT_STDLIB: &str = include_str!("../../../src/scripting/stdlib.ts");

fn tool_list_rules(deps: &Deps, args: &Value) -> Result<Value, String> {
    let rules = crate::rules::load_rules(&deps.rules_dir).map_err(|e| e.to_string())?;
    let filter = str_arg(args, "projectId");
    let rules: Vec<_> = rules
        .into_iter()
        .filter(|r| filter.as_deref().map(|p| r.project_id.as_deref() == Some(p)).unwrap_or(true))
        .collect();
    Ok(json!({ "rules": rules }))
}

fn tool_save_rule(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("rule").cloned().ok_or("missing rule")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    if raw.get("enabled").is_none() {
        raw["enabled"] = json!(true);
    }
    let rule: crate::rules::Rule = serde_json::from_value(raw).map_err(|e| format!("bad rule: {e}"))?;
    let rules = crate::rules::upsert_rule(&deps.rules_dir, rule)?;
    *deps.state.rules.write().unwrap() = rules.clone();
    Ok(json!({ "rules": rules }))
}

fn tool_delete_rule(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let rules = crate::rules::remove_rule(&deps.rules_dir, &id)?;
    *deps.state.rules.write().unwrap() = rules.clone();
    Ok(json!({ "rules": rules }))
}

fn tool_scripting_reference(deps: &Deps) -> Result<Value, String> {
    let library = crate::rules::load_library(&deps.rules_dir).unwrap_or_default();
    Ok(json!({
        "apiTypes": SCRIPT_API_DTS,
        "stdlib": SCRIPT_STDLIB,
        "librarySource": library,
    }))
}

fn tool_list_projects(deps: &Deps) -> Result<Value, String> {
    let file = crate::projects::load_projects(&deps.data_dir).map_err(|e| e.to_string())?;
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_save_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("project").cloned().ok_or("missing project")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    for key in ["includeHosts", "excludeHosts", "env"] {
        if raw.get(key).is_none() {
            raw[key] = json!([]);
        }
    }
    let project: crate::projects::Project =
        serde_json::from_value(raw).map_err(|e| format!("bad project: {e}"))?;
    let file = crate::projects::upsert_project(&deps.data_dir, project.clone())?;
    // как в UI-команде: правка активного проекта обновляет общую ячейку
    let mut active = deps.state.active_project.write().unwrap();
    if active.as_ref().map(|p| &p.id) == Some(&project.id) {
        *active = Some(project);
    }
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_delete_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let file = crate::projects::remove_project(&deps.data_dir, &id)?;
    let mut active = deps.state.active_project.write().unwrap();
    if active.as_ref().map(|p| p.id.as_str()) == Some(id.as_str()) {
        *active = None;
    }
    serde_json::to_value(&file).map_err(|e| e.to_string())
}

fn tool_set_active_project(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id");
    let resolved = crate::projects::set_active(&deps.data_dir, id)?;
    *deps.state.active_project.write().unwrap() = resolved.clone();
    Ok(json!({ "active": resolved.map(|p| json!({ "id": p.id, "name": p.name })) }))
}

fn tool_list_breakpoints(deps: &Deps) -> Result<Value, String> {
    let bps = crate::breakpoints::load_breakpoints(&deps.rules_dir).map_err(|e| e.to_string())?;
    Ok(json!({ "breakpoints": bps }))
}

fn tool_save_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let mut raw = args.get("breakpoint").cloned().ok_or("missing breakpoint")?;
    if raw.get("id").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
        raw["id"] = json!(super::gen_id());
    }
    if raw.get("enabled").is_none() {
        raw["enabled"] = json!(true);
    }
    let bp: crate::breakpoints::Breakpoint =
        serde_json::from_value(raw).map_err(|e| format!("bad breakpoint: {e}"))?;
    let bps = crate::breakpoints::upsert_breakpoint(&deps.rules_dir, bp)?;
    *deps.state.breakpoints.write().unwrap() = bps.clone();
    Ok(json!({ "breakpoints": bps }))
}

fn tool_delete_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("missing id")?;
    let bps = crate::breakpoints::remove_breakpoint(&deps.rules_dir, &id)?;
    *deps.state.breakpoints.write().unwrap() = bps.clone();
    Ok(json!({ "breakpoints": bps }))
}

fn tool_list_paused(deps: &Deps, args: &Value) -> Result<Value, String> {
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let paused: Vec<Value> = deps
        .state
        .store
        .all()
        .iter()
        .filter(|f| f.state == crate::model::FlowState::Paused)
        .map(|f| flow_to_json(f, max))
        .collect();
    Ok(json!({ "paused": paused }))
}

fn tool_resolve_breakpoint(deps: &Deps, args: &Value) -> Result<Value, String> {
    let id = u64_arg(args, "flowId").ok_or("missing flowId")?;
    let phase = str_arg(args, "phase").ok_or("missing phase")?;
    let action = str_arg(args, "action").ok_or("missing action")?;
    let edited: crate::commands::EditedPayload =
        serde_json::from_value(args.get("edits").cloned().unwrap_or_else(|| json!({})))
            .map_err(|e| format!("bad edits: {e}"))?;
    crate::commands::resolve_breakpoint_core(&deps.state.pending_breakpoints, id, &phase, &action, edited)?;
    Ok(json!({ "ok": true }))
}

fn tool_send_request(_deps: &Deps, args: &Value) -> Result<Value, String> {
    let req: crate::httpsend::SendRequest =
        serde_json::from_value(args.clone()).map_err(|e| format!("bad request: {e}"))?;
    let via_proxy = args.get("viaProxy").and_then(|v| v.as_bool()).unwrap_or(false);
    let max = u64_arg(args, "maxBodyBytes").unwrap_or(50_000) as usize;
    let resp = crate::httpsend::send_http(&req, via_proxy);
    let mut v = serde_json::to_value(&resp).map_err(|e| e.to_string())?;
    if let Some(b) = v.get("body").and_then(|b| b.as_str()) {
        if b.len() > max {
            let cut: String = b.chars().take(max).collect();
            v["body"] = json!(cut);
            v["truncated"] = json!(true);
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::AppState;
    use crate::model::{Flow, HttpMessage, UrlParts};
    use serde_json::json;

    fn test_deps<'a>(state: &'a AppState, tmp: &std::path::Path) -> Deps<'a> {
        Deps {
            state,
            data_dir: tmp.to_path_buf(),
            rules_dir: tmp.join("scripting"),
        }
    }

    fn sample_flow(id: u64, body: &[u8], is_text: bool) -> Flow {
        let mut f = Flow::new_request(
            id,
            "GET".into(),
            UrlParts { scheme: "https".into(), host: "api.test".into(), port: 443, path: "/v1".into() },
            HttpMessage { headers: vec![("A".into(), "b".into())], body: body.to_vec(), body_is_text: is_text },
        );
        f.applied_rules = vec!["r1".into()];
        f
    }

    #[test]
    fn flow_to_json_truncates_text_body() {
        let f = sample_flow(1, b"hello world", true);
        let v = flow_to_json(&f, 5);
        assert_eq!(v["request"]["body"], json!("hello"));
        assert_eq!(v["request"]["truncated"], json!(true));
        assert_eq!(v["request"]["bodySize"], json!(11));
        assert_eq!(v["appliedRules"], json!(["r1"]));
    }

    #[test]
    fn flow_to_json_skips_binary_body() {
        let f = sample_flow(1, &[0u8, 159, 146, 150], false);
        let v = flow_to_json(&f, 50_000);
        assert_eq!(v["request"]["body"], json!(null));
        assert_eq!(v["request"]["binary"], json!(true));
    }

    #[test]
    fn dispatch_get_status_and_get_flow() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let id = state.store.next_id();
        state.store.insert(sample_flow(id, b"{}", true));
        let deps = test_deps(&state, tmp.path());

        let status = dispatch(&deps, "get_status", &json!({})).unwrap();
        assert_eq!(status["proxyRunning"], json!(false));
        assert_eq!(status["flowsInMemory"], json!(1));

        let flow = dispatch(&deps, "get_flow", &json!({ "id": id })).unwrap();
        assert_eq!(flow["method"], json!("GET"));

        let err = dispatch(&deps, "get_flow", &json!({ "id": 999 })).unwrap_err();
        assert!(err.contains("not found"), "err was: {err}");

        let err = dispatch(&deps, "nope", &json!({})).unwrap_err();
        assert!(err.contains("unknown tool"), "err was: {err}");
    }

    #[test]
    fn dispatch_query_flows_against_temp_db() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let handle = crate::db::DbHandle::open(tmp.path().join("t.db")).unwrap();
        let _ = state.db.set(handle);
        // прогнать один flow через writer (async actor thread — дождаться записи)
        let mut f = sample_flow(1, b"x", true);
        f.state = crate::model::FlowState::Completed;
        state.db().unwrap().record(&f, None);
        let reader = state.db().unwrap().reader().unwrap();
        for _ in 0..200 {
            if reader.count(&crate::db::FlowQuery::default()).unwrap() > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let deps = test_deps(&state, tmp.path());

        let out = dispatch(&deps, "query_flows", &json!({ "filter": { "host": "api.test" } })).unwrap();
        assert_eq!(out["flows"].as_array().unwrap().len(), 1);
        let cnt = dispatch(&deps, "flow_count", &json!({})).unwrap();
        assert_eq!(cnt["count"], json!(1));
        let agg = dispatch(&deps, "aggregate_flows", &json!({ "groupBy": "host" })).unwrap();
        assert_eq!(agg["buckets"][0]["key"], json!("api.test"));
    }

    #[test]
    fn flow_to_json_truncates_multibyte_utf8_at_char_boundary() {
        // "héllo" is 6 bytes: h(1) + é(2) + l(1) + l(1) + o(1)
        let body = "héllo".as_bytes().to_vec();
        let f = sample_flow(1, &body, true);
        // Truncate to 2 bytes: should yield just "h", not "h\u{FFFD}"
        let v = flow_to_json(&f, 2);
        assert_eq!(v["request"]["body"], json!("h"), "should not contain replacement character");
        assert_eq!(v["request"]["truncated"], json!(true));
        assert_eq!(v["request"]["bodySize"], json!(6));
    }

    #[test]
    fn every_tool_def_has_object_schema() {
        for def in core_tools() {
            assert_eq!(def.schema["type"], json!("object"), "tool {}", def.name);
        }
    }

    #[test]
    fn save_rule_generates_id_and_updates_shared_state() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        let out = dispatch(&deps, "save_rule", &json!({
            "rule": { "name": "R", "pattern": "api.test/*", "phase": "request", "script": "" }
        })).unwrap();
        let rules = out["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["enabled"], json!(true));
        assert_eq!(rules[0]["id"].as_str().unwrap().len(), 16);
        // разделяемое состояние обновилось — прокси увидит правило сразу
        assert_eq!(state.rules.read().unwrap().len(), 1);

        let id = rules[0]["id"].as_str().unwrap().to_string();
        dispatch(&deps, "delete_rule", &json!({ "id": id })).unwrap();
        assert!(state.rules.read().unwrap().is_empty());
    }

    #[test]
    fn save_rule_conflict_is_returned_as_error() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        dispatch(&deps, "save_rule", &json!({
            "rule": { "name": "A", "pattern": "x/*", "phase": "both", "script": "" }
        })).unwrap();
        let err = dispatch(&deps, "save_rule", &json!({
            "rule": { "name": "B", "pattern": "x/*", "phase": "request", "script": "" }
        })).unwrap_err();
        assert!(err.contains("Conflicts"), "err was: {err}");
    }

    #[test]
    fn scripting_reference_contains_api_and_library() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        let v = dispatch(&deps, "get_scripting_reference", &json!({})).unwrap();
        assert!(v["apiTypes"].as_str().unwrap().contains("API_DTS"));
        assert!(v["stdlib"].as_str().unwrap().contains("STD_DTS"));
        assert!(v["librarySource"].is_string());
    }

    #[test]
    fn project_tools_roundtrip() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        let out = dispatch(&deps, "save_project", &json!({
            "project": { "name": "P", "includeHosts": ["api.test"] }
        })).unwrap();
        let id = out["projects"][0]["id"].as_str().unwrap().to_string();

        let act = dispatch(&deps, "set_active_project", &json!({ "id": id })).unwrap();
        assert_eq!(act["active"]["name"], json!("P"));
        assert_eq!(state.active_project.read().unwrap().as_ref().unwrap().name, "P");

        dispatch(&deps, "delete_project", &json!({ "id": id })).unwrap();
        assert!(state.active_project.read().unwrap().is_none());

        let listed = dispatch(&deps, "list_projects", &json!({})).unwrap();
        assert!(listed["projects"].as_array().unwrap().is_empty());
    }

    #[test]
    fn breakpoint_tools_roundtrip() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        let out = dispatch(&deps, "save_breakpoint", &json!({
            "breakpoint": { "name": "B", "pattern": "*/login", "onRequest": true, "onResponse": false }
        })).unwrap();
        let id = out["breakpoints"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(state.breakpoints.read().unwrap().len(), 1);
        dispatch(&deps, "delete_breakpoint", &json!({ "id": id })).unwrap();
        assert!(state.breakpoints.read().unwrap().is_empty());
    }

    #[test]
    fn list_paused_returns_only_paused_flows() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let mut f = sample_flow(1, b"x", true);
        f.state = crate::model::FlowState::Paused;
        f.paused_phase = Some("request".into());
        state.store.insert(f);
        state.store.insert(sample_flow(2, b"y", true));
        let deps = test_deps(&state, tmp.path());
        let v = dispatch(&deps, "list_paused", &json!({})).unwrap();
        let paused = v["paused"].as_array().unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0]["id"], json!(1));
        assert_eq!(paused[0]["pausedPhase"], json!("request"));
    }

    #[tokio::test]
    async fn resolve_breakpoint_tool_sends_resolution() {
        use crate::proxy::{BpPhase, Resolution};
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.pending_breakpoints.lock().unwrap().insert((7, BpPhase::Request), tx);
        let deps = test_deps(&state, tmp.path());
        dispatch(&deps, "resolve_breakpoint", &json!({
            "flowId": 7, "phase": "request", "action": "abort",
            "edits": { "reason": "nope" }
        })).unwrap();
        match rx.await.unwrap() {
            Resolution::Abort(r) => assert_eq!(r, "nope"),
            _ => panic!("wrong resolution"),
        }
    }

    #[test]
    fn resolve_breakpoint_missing_flow_errors() {
        let state = AppState::new();
        let tmp = tempfile::tempdir().unwrap();
        let deps = test_deps(&state, tmp.path());
        let err = dispatch(&deps, "resolve_breakpoint", &json!({
            "flowId": 1, "phase": "request", "action": "abort", "edits": {}
        })).unwrap_err();
        assert!(err.contains("no pending breakpoint"), "err was: {err}");
    }
}
