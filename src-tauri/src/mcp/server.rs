//! rmcp ServerHandler + Streamable HTTP транспорт (axum) с bearer-аутентификацией.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use serde_json::Value;
use tauri::Manager;

use super::{core_tools, McpConfig, McpState};

// Generic over the Tauri runtime so the same handler works against the real
// Wry webview (production) and `tauri::test::MockRuntime` (integration test).
#[derive(Clone)]
pub struct TrawlMcp<R: tauri::Runtime = tauri::Wry> {
    app: tauri::AppHandle<R>,
}

fn tool_from(name: String, description: String, schema: Value) -> Tool {
    let obj = schema.as_object().cloned().unwrap_or_default();
    Tool::new(name, description, Arc::new(obj))
}

impl<R: tauri::Runtime> ServerHandler for TrawlMcp<R> {
    fn get_info(&self) -> ServerInfo {
        // InitializeResult/Implementation are #[non_exhaustive] in rmcp 2.2 —
        // struct-literal + `..Default::default()` doesn't compile outside the
        // defining crate, so build them via their constructor/builder methods.
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new("trawl", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "Trawl is a MITM HTTP(S) proxy. Inspect captured traffic (query_flows/get_flow), \
             manage rewrite rules and projects, resolve paused breakpoints, send requests. \
             Start with get_status.",
        )
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        // Запоминаем пира — ему будем слать tools/list_changed.
        self.app.state::<McpState>().peers.add(context.peer.clone());
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools: Vec<Tool> = core_tools::core_tools()
            .into_iter()
            .map(|d| tool_from(d.name.to_string(), d.description.to_string(), d.schema))
            .collect();
        let mcp = self.app.state::<McpState>();
        for t in mcp.bridge.tools.read().unwrap().iter() {
            tools.push(tool_from(t.full_name(), t.description.clone(), t.input_schema.clone()));
        }
        Ok(ListToolsResult { tools, ..Default::default() })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = request.name.to_string();
        let args = Value::Object(request.arguments.unwrap_or_default());
        let mcp = self.app.state::<McpState>();
        let result = if let Some(tool) = mcp.bridge.find(&name) {
            let app = self.app.clone();
            let bridge = mcp.bridge.clone();
            bridge
                .call(
                    move |payload| {
                        use tauri::Emitter;
                        let _ = app.emit("mcp:tool-call", payload);
                    },
                    &tool,
                    args,
                )
                .await
        } else {
            // Кор-тулы синхронные (rusqlite/файлы/blocking reqwest) — уводим с async-потока.
            let app = self.app.clone();
            tokio::task::spawn_blocking(move || {
                let state = app.state::<crate::commands::AppState>();
                let deps = core_tools::Deps {
                    state: state.inner(),
                    data_dir: crate::commands::data_dir(&app)?,
                    rules_dir: crate::commands::rules_dir(&app)?,
                };
                core_tools::dispatch(&deps, &name, &args)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
        };
        Ok(match result {
            Ok(v) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
        })
    }
}

// ── transport ──

pub struct ServerHandle {
    pub addr: SocketAddr,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl ServerHandle {
    /// Отправляет graceful-shutdown сигнал и дожидается, пока задача с
    /// `axum::serve` реально завершится и слушатель порта освободится —
    /// иначе немедленный ребайнд на тот же порт (regen token, set config)
    /// может словить EADDRINUSE, пока старый listener ещё жив.
    pub async fn stop(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

pub async fn require_bearer(token: &str, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == token)
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

pub async fn start_server<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    cfg: McpConfig,
) -> Result<ServerHandle, String> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    // Только loopback: MCP-сервер не должен быть виден с LAN.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", cfg.port))
        .await
        .map_err(|e| format!("bind 127.0.0.1:{}: {e}", cfg.port))?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let handler_app = app.clone();
    let service = StreamableHttpService::new(
        move || Ok(TrawlMcp { app: handler_app.clone() }),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let token = cfg.token.clone();
    let router = axum::Router::new().nest_service("/mcp", service).layer(
        axum::middleware::from_fn(move |req: Request, next: Next| {
            let token = token.clone();
            async move { require_bearer(&token, req, next).await }
        }),
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let task = tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(ServerHandle { addr, shutdown: tx, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::McpConfig;

    // `list_tools` needs a live `RequestContext<RoleServer>`, which rmcp only
    // hands out from inside a real session (it carries a `Peer` tied to the
    // transport) — there's no public constructor to fake one in a unit test.
    // So we test the two things list_tools actually assembles instead: the
    // core tool table's size, and that a plugin tool registered into the
    // bridge (the same `McpState` the handler reads via `app.state::<McpState>()`)
    // is discoverable by its `full_name()` — i.e. it would be included in the
    // `tools` vec that list_tools builds by chaining core_tools() with
    // `mcp.bridge.tools.read()`.
    #[test]
    fn core_tool_count_and_registered_plugin_tool_are_discoverable() {
        assert_eq!(
            core_tools::core_tools().len(),
            19,
            "core tool count changed — list_tools's static half moved without updating this test"
        );

        let app = tauri::test::mock_builder()
            .manage(crate::commands::AppState::new())
            .manage(crate::mcp::McpState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();

        let mcp = app.state::<McpState>();
        mcp.bridge.register(super::super::plugin_bridge::PluginTool {
            plugin_id: "demo-plugin".into(),
            name: "plugin_tool".into(),
            description: "A demo plugin tool".into(),
            input_schema: serde_json::json!({ "type": "object" }),
            timeout_ms: None,
        });

        // What list_tools iterates over (`mcp.bridge.tools.read().unwrap()`)
        // now contains the registered tool, keyed by its full_name().
        let found = mcp
            .bridge
            .find("demo-plugin_plugin_tool")
            .expect("registered plugin tool should be findable by full_name");
        assert_eq!(found.plugin_id, "demo-plugin");
        assert_eq!(found.name, "plugin_tool");
    }

    fn init_payload() -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        })
    }

    #[tokio::test]
    async fn rejects_missing_token_accepts_valid() {
        let app = tauri::test::mock_builder()
            .manage(crate::commands::AppState::new())
            .manage(crate::mcp::McpState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let cfg = McpConfig { enabled: true, port: 0, token: "secret".into() };
        let handle = start_server(app.handle().clone(), cfg).await.unwrap();
        let url = format!("http://{}/mcp", handle.addr);
        let client = reqwest::Client::new();

        let r = client
            .post(&url)
            .header("Accept", "application/json, text/event-stream")
            .json(&init_payload())
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 401);

        let r = client
            .post(&url)
            .header("Authorization", "Bearer secret")
            .header("Accept", "application/json, text/event-stream")
            .json(&init_payload())
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success(), "status was {}", r.status());
    }

    #[tokio::test]
    async fn restart_on_same_port_rebinds() {
        let app = tauri::test::mock_builder()
            .manage(crate::commands::AppState::new())
            .manage(crate::mcp::McpState::new())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let cfg = McpConfig { enabled: true, port: 0, token: "secret".into() };
        let h1 = start_server(app.handle().clone(), cfg.clone()).await.unwrap();
        let port = h1.addr.port();
        h1.stop().await;
        let cfg2 = McpConfig { enabled: true, port, token: "secret".into() };
        let h2 = start_server(app.handle().clone(), cfg2).await
            .expect("rebind on same port after stop must succeed");
        h2.stop().await;
    }
}
