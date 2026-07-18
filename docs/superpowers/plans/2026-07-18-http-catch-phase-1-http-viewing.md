# http-catch — Phase 1: Каркас + просмотр HTTP — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Собрать работающий Tauri-каркас, который поднимает HTTP-прокси, перехватывает проходящие HTTP-запросы и показывает их в React-UI (список + детали).

**Architecture:** Один Tauri-процесс. Rust-бэкенд поднимает MITM-прокси на базе `hudsucker` (в Phase 1 используется только для HTTP; эфемерный in-memory CA нужен лишь чтобы прокси стартовал). Каждый завершённый запрос/ответ превращается в `Flow`, кладётся в in-memory store и эмитится в WebView как Tauri-событие. React подписывается на события и рендерит виртуализированный список с панелью деталей.

**Tech Stack:** Tauri 2, Rust (`hudsucker`, `hyper`, `tokio`, `rcgen`, `serde`), React 18 + TypeScript, Vite, `zustand`, `@tanstack/react-virtual`.

## Global Constraints

- **Платформа:** macOS (Apple Silicon/Intel). Другие ОС — вне рамок.
- **Менеджер пакетов JS:** `pnpm` (версия ≥ 10). Не использовать npm/yarn для установки.
- **Rust edition:** 2021, toolchain ≥ 1.95.
- **Порт прокси по умолчанию:** `8888`, слушать на `0.0.0.0` (чтобы телефон в LAN мог подключиться в следующих фазах).
- **Формат имён событий Tauri:** kebab-case (`flow-added`, `flow-updated`).
- **Единая модель `Flow`:** поля в Rust (`serde`, camelCase через `#[serde(rename_all = "camelCase")]`) и в TS должны совпадать один-в-один.
- **TDD:** каждая задача начинается с падающего теста; частые коммиты.

---

## File Structure

**Rust (`src-tauri/`):**
- `src-tauri/src/lib.rs` — точка входа Tauri, регистрация команд и state.
- `src-tauri/src/model.rs` — тип `Flow` и вложенные структуры (общая модель данных).
- `src-tauri/src/store.rs` — in-memory ring-buffer хранилище потоков.
- `src-tauri/src/ca.rs` — минимальная генерация эфемерного CA для запуска hudsucker.
- `src-tauri/src/proxy.rs` — прокси-движок на hudsucker, сбор `Flow`, эмит событий.
- `src-tauri/src/commands.rs` — Tauri-команды (`start_proxy`, `stop_proxy`, `get_flows`).

**Frontend (`src/`):**
- `src/types.ts` — TS-типы `Flow` (зеркало `model.rs`).
- `src/store.ts` — zustand-стор списка потоков + подписка на события.
- `src/components/TrafficList.tsx` — виртуализированная таблица.
- `src/components/FlowDetail.tsx` — детали выбранного потока (вкладки).
- `src/App.tsx` — компоновка: список слева, детали справа.

---

### Task 1: Скаффолд Tauri + React-TS и запуск окна

**Files:**
- Create: весь каркас Tauri (`src-tauri/`, `src/`, `package.json`, `vite.config.ts`, `index.html`, `tsconfig.json`).
- Modify: перенос сгенерированного каркаса в корень репозитория (сохранив `docs/` и `.git/`).

**Interfaces:**
- Consumes: ничего.
- Produces: рабочий `pnpm tauri dev`, открывающий пустое окно приложения `http-catch`.

- [ ] **Step 1: Сгенерировать каркас во временную папку**

Каркас нельзя создать прямо в непустом репозитории, поэтому генерируем рядом и переносим.

```bash
cd /Users/legostin/claude-projects
pnpm create tauri-app@latest http-catch-scaffold --template react-ts --manager pnpm
```

Если CLI задаёт вопросы интерактивно — выбрать: package manager `pnpm`, UI template `React`, flavor `TypeScript`.

- [ ] **Step 2: Перенести содержимое каркаса в корень репозитория**

```bash
cd /Users/legostin/claude-projects/http-catch-scaffold
# перенос всех файлов и скрытых, кроме .git (в каркасе его нет)
rsync -a --exclude '.git' ./ /Users/legostin/claude-projects/http-catch/
cd /Users/legostin/claude-projects/http-catch
rm -rf /Users/legostin/claude-projects/http-catch-scaffold
```

- [ ] **Step 3: Установить зависимости и добавить UI-библиотеки**

```bash
cd /Users/legostin/claude-projects/http-catch
pnpm install
pnpm add zustand @tanstack/react-virtual
```

- [ ] **Step 4: Запустить приложение и убедиться, что окно открывается**

Run: `pnpm tauri dev`
Expected: собирается Rust + Vite, открывается нативное окно с дефолтной страницей Tauri. Закрыть окно (Ctrl+C в терминале) после проверки.

- [ ] **Step 5: Настроить порт/заголовок окна**

В `src-tauri/tauri.conf.json` задать в блоке `app.windows[0]`: `"title": "http-catch"`, `"width": 1200`, `"height": 800`.

- [ ] **Step 6: Commit**

```bash
cd /Users/legostin/claude-projects/http-catch
git add -A
git commit -m "chore: скаффолд Tauri + React-TS каркаса http-catch"
```

---

### Task 2: Модель `Flow` (Rust + TS) с тестом сериализации

**Files:**
- Create: `src-tauri/src/model.rs`
- Create: `src/types.ts`
- Modify: `src-tauri/src/lib.rs` (добавить `mod model;`)

**Interfaces:**
- Consumes: ничего.
- Produces:
  - Rust: `Flow`, `HttpMessage`, `Timings`, `FlowState`, `UrlParts` (все `Serialize`/`Deserialize`, camelCase).
  - `Flow::new_request(id: u64, method: String, url: UrlParts, request: HttpMessage) -> Flow`
  - TS: интерфейс `Flow` с идентичными полями.

- [ ] **Step 1: Написать падающий тест сериализации (Rust)**

В конце `src-tauri/src/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_serializes_to_camel_case_json() {
        let flow = Flow::new_request(
            1,
            "GET".into(),
            UrlParts {
                scheme: "http".into(),
                host: "example.com".into(),
                port: 80,
                path: "/api/v1".into(),
            },
            HttpMessage {
                headers: vec![("Accept".into(), "application/json".into())],
                body: b"".to_vec(),
                body_is_text: true,
            },
        );
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("\"bodyIsText\":true"), "json was: {json}");
        assert!(json.contains("\"state\":\"pending\""), "json was: {json}");

        let back: Flow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 1);
        assert_eq!(back.method, "GET");
    }
}
```

- [ ] **Step 2: Запустить тест — убедиться, что не компилируется/падает**

Run: `cd src-tauri && cargo test model::tests::flow_serializes_to_camel_case_json`
Expected: FAIL — типы `Flow`/`HttpMessage`/... не определены.

- [ ] **Step 3: Реализовать модель**

В начало `src-tauri/src/model.rs` (перед тестами):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlParts {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMessage {
    /// Заголовки в порядке получения (дубликаты сохраняются).
    pub headers: Vec<(String, String)>,
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
    pub body_is_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMessage {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
    pub body_is_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    /// Миллисекунды от старта прокси-сессии; None пока не наступило.
    pub sent: Option<u64>,
    pub ttfb: Option<u64>,
    pub done: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FlowState {
    Pending,
    Completed,
    Error,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub id: u64,
    /// Unix-время в мс, когда запрос перехвачен.
    pub timestamp: u64,
    pub method: String,
    pub url: UrlParts,
    pub request: HttpMessage,
    pub response: Option<ResponseMessage>,
    pub timings: Timings,
    pub state: FlowState,
    /// Заполняется при state == Error.
    pub error: Option<String>,
}

impl Flow {
    pub fn new_request(id: u64, method: String, url: UrlParts, request: HttpMessage) -> Flow {
        Flow {
            id,
            timestamp: 0,
            method,
            url,
            request,
            response: None,
            timings: Timings { sent: None, ttfb: None, done: None },
            state: FlowState::Pending,
            error: None,
        }
    }
}
```

Добавить зависимость `serde_bytes` в `src-tauri/Cargo.toml`:

```toml
serde_bytes = "0.11"
```

(`serde` и `serde_json` уже присутствуют в дефолтном каркасе Tauri; если `serde_json` отсутствует в `[dependencies]`, добавить `serde_json = "1"`.)

В `src-tauri/src/lib.rs` добавить в начало: `mod model;`

- [ ] **Step 4: Запустить тест — убедиться, что проходит**

Run: `cd src-tauri && cargo test model::tests::flow_serializes_to_camel_case_json`
Expected: PASS.

- [ ] **Step 5: Создать зеркальные TS-типы**

`src/types.ts`:

```ts
export type FlowState = "pending" | "completed" | "error" | "paused";

export interface UrlParts {
  scheme: string;
  host: string;
  port: number;
  path: string;
}

export type Header = [name: string, value: string];

export interface HttpMessage {
  headers: Header[];
  /** base64 при передаче через serde_bytes может прийти массивом чисел — см. store.ts нормализацию. */
  body: number[] | string;
  bodyIsText: boolean;
}

export interface ResponseMessage {
  status: number;
  headers: Header[];
  body: number[] | string;
  bodyIsText: boolean;
}

export interface Timings {
  sent: number | null;
  ttfb: number | null;
  done: number | null;
}

export interface Flow {
  id: number;
  timestamp: number;
  method: string;
  url: UrlParts;
  request: HttpMessage;
  response: ResponseMessage | null;
  timings: Timings;
  state: FlowState;
  error: string | null;
}
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: модель Flow (Rust + TS) с тестом сериализации"
```

---

### Task 3: In-memory store с ring-лимитом

**Files:**
- Create: `src-tauri/src/store.rs`
- Modify: `src-tauri/src/lib.rs` (`mod store;`)

**Interfaces:**
- Consumes: `model::Flow` из Task 2.
- Produces:
  - `FlowStore::new(capacity: usize) -> FlowStore`
  - `FlowStore::next_id(&self) -> u64` (монотонный счётчик)
  - `FlowStore::insert(&self, flow: Flow)` (добавляет; при превышении capacity вытесняет старейший)
  - `FlowStore::update<F: FnOnce(&mut Flow)>(&self, id: u64, f: F) -> bool`
  - `FlowStore::all(&self) -> Vec<Flow>`
  - `FlowStore` потокобезопасен (внутри `Mutex`), клонируется дёшево (`Arc`).

- [ ] **Step 1: Написать падающие тесты**

`src-tauri/src/store.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Flow, HttpMessage, UrlParts};

    fn sample(id: u64) -> Flow {
        Flow::new_request(
            id,
            "GET".into(),
            UrlParts { scheme: "http".into(), host: "h".into(), port: 80, path: "/".into() },
            HttpMessage { headers: vec![], body: vec![], body_is_text: true },
        )
    }

    #[test]
    fn next_id_is_monotonic() {
        let s = FlowStore::new(10);
        assert_eq!(s.next_id(), 1);
        assert_eq!(s.next_id(), 2);
    }

    #[test]
    fn insert_and_all_roundtrip() {
        let s = FlowStore::new(10);
        s.insert(sample(1));
        s.insert(sample(2));
        assert_eq!(s.all().len(), 2);
    }

    #[test]
    fn ring_limit_evicts_oldest() {
        let s = FlowStore::new(2);
        s.insert(sample(1));
        s.insert(sample(2));
        s.insert(sample(3));
        let ids: Vec<u64> = s.all().iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn update_mutates_existing() {
        let s = FlowStore::new(10);
        s.insert(sample(1));
        let ok = s.update(1, |f| f.method = "POST".into());
        assert!(ok);
        assert_eq!(s.all()[0].method, "POST");
        assert!(!s.update(999, |_| {}));
    }
}
```

- [ ] **Step 2: Запустить тесты — убедиться, что падают**

Run: `cd src-tauri && cargo test store::tests`
Expected: FAIL — `FlowStore` не определён.

- [ ] **Step 3: Реализовать store**

В начало `src-tauri/src/store.rs`:

```rust
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::model::Flow;

#[derive(Clone)]
pub struct FlowStore {
    inner: Arc<Inner>,
}

struct Inner {
    flows: Mutex<VecDeque<Flow>>,
    capacity: usize,
    counter: AtomicU64,
}

impl FlowStore {
    pub fn new(capacity: usize) -> FlowStore {
        FlowStore {
            inner: Arc::new(Inner {
                flows: Mutex::new(VecDeque::new()),
                capacity,
                counter: AtomicU64::new(0),
            }),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.inner.counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn insert(&self, flow: Flow) {
        let mut q = self.inner.flows.lock().unwrap();
        if q.len() >= self.inner.capacity {
            q.pop_front();
        }
        q.push_back(flow);
    }

    pub fn update<F: FnOnce(&mut Flow)>(&self, id: u64, f: F) -> bool {
        let mut q = self.inner.flows.lock().unwrap();
        if let Some(flow) = q.iter_mut().find(|x| x.id == id) {
            f(flow);
            true
        } else {
            false
        }
    }

    pub fn all(&self) -> Vec<Flow> {
        self.inner.flows.lock().unwrap().iter().cloned().collect()
    }
}
```

В `src-tauri/src/lib.rs` добавить: `mod store;`

- [ ] **Step 4: Запустить тесты — убедиться, что проходят**

Run: `cd src-tauri && cargo test store::tests`
Expected: PASS (4 теста).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: in-memory FlowStore с ring-лимитом и тестами"
```

---

### Task 4: Прокси-движок на hudsucker (HTTP-перехват)

**Files:**
- Create: `src-tauri/src/ca.rs`
- Create: `src-tauri/src/proxy.rs`
- Modify: `src-tauri/src/lib.rs` (`mod ca; mod proxy;`)
- Modify: `src-tauri/Cargo.toml` (зависимости hudsucker/tokio/rcgen)

**Interfaces:**
- Consumes: `FlowStore` (Task 3), `Flow`/`HttpMessage`/`ResponseMessage` (Task 2).
- Produces:
  - `ca::generate_ephemeral_ca() -> anyhow::Result<(rcgen::KeyPair, rcgen::Certificate)>` (Phase 2 заменит на персистентный CA).
  - `proxy::ProxyHandle` со `stop()`.
  - `proxy::start(addr: SocketAddr, store: FlowStore, emit: EmitFn) -> anyhow::Result<ProxyHandle>`, где `EmitFn = Arc<dyn Fn(&str, &Flow) + Send + Sync>` — колбэк для эмита события (в тесте — сбор в вектор, в приложении — Tauri emit).

> **ВАЖНО (сверка API):** точные сигнатуры `hudsucker` меняются между версиями. Код ниже нацелен на **hudsucker 0.22.x** (на базе hyper 1.x). После добавления зависимости выполнить `cargo doc -p hudsucker` или открыть docs.rs для установленной версии и сверить: имя трейта обработчика (`HttpHandler`), сигнатуры `handle_request`/`handle_response`, тип тела (`hudsucker::Body`), конструктор `RcgenAuthority::new(...)` и билдер `Proxy::builder()`. При расхождении поправить только glue в `proxy.rs`; наши модель/стор не меняются.

- [ ] **Step 1: Добавить зависимости**

В `src-tauri/Cargo.toml` `[dependencies]`:

```toml
tokio = { version = "1", features = ["full"] }
hudsucker = "0.22"
rcgen = "0.13"
anyhow = "1"
http = "1"
```

Run: `cd src-tauri && cargo build`
Expected: зависимости скачиваются и компилируются (может занять несколько минут).

- [ ] **Step 2: Реализовать эфемерный CA**

`src-tauri/src/ca.rs`:

```rust
use anyhow::Result;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

/// Генерирует самоподписанный CA в памяти. В Phase 1 нужен только чтобы hudsucker стартовал;
/// HTTPS-перехват (доверие на устройстве) добавляется в Phase 2.
pub fn generate_ephemeral_ca() -> Result<(KeyPair, rcgen::Certificate)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "http-catch CA");
    dn.push(DnType::OrganizationName, "http-catch");
    params.distinguished_name = dn;
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((key_pair, cert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_ca_certificate() {
        let (_kp, cert) = generate_ephemeral_ca().unwrap();
        let pem = cert.pem();
        assert!(pem.contains("BEGIN CERTIFICATE"));
    }
}
```

Run: `cd src-tauri && cargo test ca::tests`
Expected: PASS (сверить `rcgen` 0.13 API: `params.self_signed(&key_pair)` и `KeyPair::generate()` — при расхождении поправить по docs.rs).

- [ ] **Step 3: Написать интеграционный тест прокси (падающий)**

`src-tauri/src/proxy.rs` (в конце файла):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FlowStore;
    use std::sync::{Arc, Mutex};
    use std::net::SocketAddr;

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
```

Добавить в `src-tauri/Cargo.toml` `[dev-dependencies]`:

```toml
reqwest = { version = "0.12", default-features = false }
```

- [ ] **Step 4: Запустить тест — убедиться, что падает**

Run: `cd src-tauri && cargo test proxy::tests::captures_http_flow_through_proxy`
Expected: FAIL — `start`/`EmitFn`/`ProxyHandle` не определены.

- [ ] **Step 5: Реализовать прокси-движок**

В начало `src-tauri/src/proxy.rs`:

```rust
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use hudsucker::{
    certificate_authority::RcgenAuthority,
    hyper::{Request, Response},
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse,
};
use tokio::sync::oneshot;

use crate::ca::generate_ephemeral_ca;
use crate::model::{Flow, HttpMessage, ResponseMessage, UrlParts};
use crate::store::FlowStore;

pub type EmitFn = Arc<dyn Fn(&str, &Flow) + Send + Sync>;

#[derive(Clone)]
struct CaptureHandler {
    store: FlowStore,
    emit: EmitFn,
    current_id: Option<u64>,
}

fn headers_to_vec(headers: &hudsucker::hyper::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).to_string()))
        .collect()
}

fn looks_textual(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("content-type")
            && (v.contains("text") || v.contains("json") || v.contains("xml") || v.contains("form-urlencoded"))
    })
}

impl HttpHandler for CaptureHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        let (parts, body) = req.into_parts();
        let bytes = match http_body_util::BodyExt::collect(body).await {
            Ok(c) => c.to_bytes().to_vec(),
            Err(_) => Vec::new(),
        };
        let headers = headers_to_vec(&parts.headers);
        let uri = &parts.uri;
        let url = UrlParts {
            scheme: uri.scheme_str().unwrap_or("http").to_string(),
            host: uri.host().unwrap_or_default().to_string(),
            port: uri.port_u16().unwrap_or(80),
            path: uri.path_and_query().map(|p| p.to_string()).unwrap_or_else(|| "/".into()),
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

        // пересобрать запрос без изменений
        let rebuilt = Request::from_parts(parts, Body::from(bytes));
        RequestOrResponse::Request(rebuilt)
    }

    async fn handle_response(
        &mut self,
        _ctx: &HttpContext,
        res: Response<Body>,
    ) -> Response<Body> {
        let (parts, body) = res.into_parts();
        let bytes = match http_body_util::BodyExt::collect(body).await {
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
                f.state = crate::model::FlowState::Completed;
            });
            if let Some(updated) = self.store.all().into_iter().find(|f| f.id == id) {
                (self.emit)("flow-updated", &updated);
            }
        }
        Response::from_parts(parts, Body::from(bytes))
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
    let authority = RcgenAuthority::new(ca_key, ca_cert, 1_000, aws_lc_rs_provider());

    // забиндиться заранее, чтобы узнать реальный порт при :0
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let handler = CaptureHandler { store, emit, current_id: None };
    let (tx, rx) = oneshot::channel::<()>();

    let proxy = Proxy::builder()
        .with_listener(listener)
        .with_ca(authority)
        .with_rustls_client(aws_lc_rs_provider())
        .with_http_handler(handler)
        .with_graceful_shutdown(async move {
            let _ = rx.await;
        })
        .build()
        .expect("proxy build");

    tokio::spawn(async move {
        let _ = proxy.start().await;
    });

    Ok(ProxyHandle { shutdown: Some(tx), addr: bound })
}

fn aws_lc_rs_provider() -> Arc<hudsucker::rustls::crypto::CryptoProvider> {
    Arc::new(hudsucker::rustls::crypto::aws_lc_rs::default_provider())
}
```

Добавить в `[dependencies]` `src-tauri/Cargo.toml`: `http-body-util = "0.1"`.

В `src-tauri/src/lib.rs`: `mod ca;` и `mod proxy;`

> Если билд ругается на `with_listener` / `with_rustls_client` / провайдер — сверить с docs.rs установленной hudsucker: в части версий билдер принимает `.with_addr(addr)` вместо `.with_listener`, а провайдер задаётся иначе. Поправить только эти вызовы; сигнатуры `start`/`ProxyHandle`/handler-логику сохранить.

- [ ] **Step 6: Запустить тест — убедиться, что проходит**

Run: `cd src-tauri && cargo test proxy::tests::captures_http_flow_through_proxy`
Expected: PASS. Если падает из-за API hudsucker — выполнить сверку из заметки выше и повторить.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: прокси-движок на hudsucker с перехватом HTTP-flow"
```

---

### Task 5: Tauri-команды и запуск прокси при старте приложения

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (state, регистрация команд, setup-хук)

**Interfaces:**
- Consumes: `FlowStore`, `proxy::start`, `proxy::ProxyHandle`.
- Produces Tauri-команды (вызываются из JS через `invoke`):
  - `start_proxy(port: u16) -> Result<String, String>` (возвращает `host:port`)
  - `stop_proxy() -> Result<(), String>`
  - `get_flows() -> Vec<Flow>`
- Эмитит события `flow-added` / `flow-updated` с payload = `Flow`.

- [ ] **Step 1: Определить app state и команды**

`src-tauri/src/commands.rs`:

```rust
use std::net::SocketAddr;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, State};

use crate::model::Flow;
use crate::proxy::{self, ProxyHandle};
use crate::store::FlowStore;

pub struct AppState {
    pub store: FlowStore,
    pub proxy: Mutex<Option<ProxyHandle>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState { store: FlowStore::new(5000), proxy: Mutex::new(None) }
    }
}

#[tauri::command]
pub async fn start_proxy(
    port: u16,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    {
        if state.proxy.lock().unwrap().is_some() {
            return Err("proxy already running".into());
        }
    }
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().map_err(|e: std::net::AddrParseError| e.to_string())?;
    let app_for_emit = app.clone();
    let emit: proxy::EmitFn = std::sync::Arc::new(move |event: &str, flow: &Flow| {
        let _ = app_for_emit.emit(event, flow.clone());
    });
    let handle = proxy::start(addr, state.store.clone(), emit).await.map_err(|e| e.to_string())?;
    *state.proxy.lock().unwrap() = Some(handle);
    Ok(format!("0.0.0.0:{port}"))
}

#[tauri::command]
pub fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(handle) = state.proxy.lock().unwrap().take() {
        handle.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn get_flows(state: State<'_, AppState>) -> Vec<Flow> {
    state.store.all()
}
```

- [ ] **Step 2: Зарегистрировать state и команды в `lib.rs`**

В `src-tauri/src/lib.rs` привести `run()` к виду (сохранив существующие `mod`-декларации):

```rust
mod ca;
mod commands;
mod model;
mod proxy;
mod store;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_proxy,
            commands::stop_proxy,
            commands::get_flows,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

(Если в дефолтном каркасе была команда `greet` — удалить её и её регистрацию. Плагин `tauri_plugin_opener` оставить, если он есть в каркасе; иначе убрать строку `.plugin(...)`.)

- [ ] **Step 3: Проверить, что бэкенд собирается**

Run: `cd src-tauri && cargo build`
Expected: сборка без ошибок.

- [ ] **Step 4: Проверить команды вручную из devtools**

Run: `pnpm tauri dev`, в открытом окне открыть DevTools (правый клик → Inspect, если включено) и в консоли:

```js
const { invoke } = window.__TAURI__.core;
await invoke("start_proxy", { port: 8888 });
```

Expected: возвращает `"0.0.0.0:8888"`. Затем прогнать трафик через прокси (например, в терминале `curl -x http://127.0.0.1:8888 http://example.com`) и `await invoke("get_flows")` — должен вернуть массив с одним flow. Остановить: `await invoke("stop_proxy")`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: Tauri-команды start/stop proxy, get_flows и эмит событий"
```

---

### Task 6: Стор фронтенда и подписка на события

**Files:**
- Create: `src/store.ts`
- Modify: `src/App.tsx` (инициализация подписки и старт прокси)

**Interfaces:**
- Consumes: TS-тип `Flow` (Task 2), Tauri `invoke`/`listen`.
- Produces zustand-стор `useFlows`:
  - `flows: Flow[]`
  - `selectedId: number | null`
  - `select(id: number): void`
  - `init(): Promise<() => void>` — навешивает слушатели `flow-added`/`flow-updated`, подтягивает `get_flows`, возвращает функцию-отписку.
  - `startProxy(port: number): Promise<string>`, `stopProxy(): Promise<void>`

- [ ] **Step 1: Реализовать стор**

`src/store.ts`:

```ts
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Flow } from "./types";

interface FlowsState {
  flows: Flow[];
  selectedId: number | null;
  select: (id: number) => void;
  upsert: (flow: Flow) => void;
  init: () => Promise<() => void>;
  startProxy: (port: number) => Promise<string>;
  stopProxy: () => Promise<void>;
}

export const useFlows = create<FlowsState>((set, get) => ({
  flows: [],
  selectedId: null,
  select: (id) => set({ selectedId: id }),
  upsert: (flow) =>
    set((s) => {
      const idx = s.flows.findIndex((f) => f.id === flow.id);
      if (idx === -1) return { flows: [...s.flows, flow] };
      const next = s.flows.slice();
      next[idx] = flow;
      return { flows: next };
    }),
  init: async () => {
    const existing = await invoke<Flow[]>("get_flows");
    set({ flows: existing });
    const un1 = await listen<Flow>("flow-added", (e) => get().upsert(e.payload));
    const un2 = await listen<Flow>("flow-updated", (e) => get().upsert(e.payload));
    return () => {
      un1();
      un2();
    };
  },
  startProxy: (port) => invoke<string>("start_proxy", { port }),
  stopProxy: () => invoke<void>("stop_proxy"),
}));
```

- [ ] **Step 2: Проверить типами (tsc)**

Run: `pnpm exec tsc --noEmit`
Expected: без ошибок типов.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: zustand-стор потоков с подпиской на события Tauri"
```

---

### Task 7: UI — TrafficList, FlowDetail и компоновка

**Files:**
- Create: `src/components/TrafficList.tsx`
- Create: `src/components/FlowDetail.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.css` (или создать простые стили)

**Interfaces:**
- Consumes: `useFlows` (Task 6), тип `Flow`.
- Produces: рабочий экран — список слева, детали справа; тумблер Start/Stop прокси сверху.

- [ ] **Step 1: TrafficList (виртуализированный)**

`src/components/TrafficList.tsx`:

```tsx
import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useFlows } from "../store";

export function TrafficList() {
  const flows = useFlows((s) => s.flows);
  const selectedId = useFlows((s) => s.selectedId);
  const select = useFlows((s) => s.select);
  const parentRef = useRef<HTMLDivElement>(null);

  const rowVirtualizer = useVirtualizer({
    count: flows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 20,
  });

  return (
    <div ref={parentRef} style={{ height: "100%", overflow: "auto" }}>
      <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
        {rowVirtualizer.getVirtualItems().map((vi) => {
          const flow = flows[vi.index];
          const status = flow.response?.status ?? "";
          return (
            <div
              key={flow.id}
              onClick={() => select(flow.id)}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: vi.size,
                transform: `translateY(${vi.start}px)`,
                display: "flex",
                gap: 8,
                padding: "0 8px",
                fontSize: 12,
                lineHeight: `${vi.size}px`,
                cursor: "pointer",
                background: flow.id === selectedId ? "#2b4b6f" : flow.state === "error" ? "#5a1e1e" : "transparent",
                whiteSpace: "nowrap",
              }}
            >
              <span style={{ width: 50 }}>{flow.method}</span>
              <span style={{ width: 40 }}>{status}</span>
              <span style={{ width: 160, overflow: "hidden", textOverflow: "ellipsis" }}>{flow.url.host}</span>
              <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis" }}>{flow.url.path}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: FlowDetail (вкладки)**

`src/components/FlowDetail.tsx`:

```tsx
import { useState } from "react";
import { useFlows } from "../store";
import type { HttpMessage, ResponseMessage } from "../types";

function bodyToText(msg: HttpMessage | ResponseMessage | null | undefined): string {
  if (!msg) return "";
  const b = msg.body;
  if (typeof b === "string") return b;
  if (!msg.bodyIsText) return `<binary ${b.length} bytes>`;
  try {
    return new TextDecoder().decode(new Uint8Array(b));
  } catch {
    return `<binary ${b.length} bytes>`;
  }
}

export function FlowDetail() {
  const flow = useFlows((s) => s.flows.find((f) => f.id === s.selectedId) ?? null);
  const [tab, setTab] = useState<"headers" | "body" | "timing">("headers");

  if (!flow) return <div style={{ padding: 16, opacity: 0.6 }}>Выберите запрос</div>;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", fontSize: 12 }}>
      <div style={{ padding: 8, borderBottom: "1px solid #333" }}>
        <strong>{flow.method}</strong> {flow.url.scheme}://{flow.url.host}:{flow.url.port}{flow.url.path}
        {flow.error && <div style={{ color: "#f88" }}>Ошибка: {flow.error}</div>}
      </div>
      <div style={{ display: "flex", gap: 8, padding: 8 }}>
        {(["headers", "body", "timing"] as const).map((t) => (
          <button key={t} onClick={() => setTab(t)} style={{ fontWeight: tab === t ? "bold" : "normal" }}>
            {t}
          </button>
        ))}
      </div>
      <div style={{ flex: 1, overflow: "auto", padding: 8, fontFamily: "monospace", whiteSpace: "pre-wrap" }}>
        {tab === "headers" && (
          <>
            <div style={{ opacity: 0.6 }}>— Request —</div>
            {flow.request.headers.map(([k, v], i) => (
              <div key={`rq${i}`}>{k}: {v}</div>
            ))}
            <div style={{ opacity: 0.6, marginTop: 8 }}>— Response —</div>
            {flow.response?.headers.map(([k, v], i) => (
              <div key={`rs${i}`}>{k}: {v}</div>
            ))}
          </>
        )}
        {tab === "body" && (
          <>
            <div style={{ opacity: 0.6 }}>— Request body —</div>
            <div>{bodyToText(flow.request)}</div>
            <div style={{ opacity: 0.6, marginTop: 8 }}>— Response body —</div>
            <div>{bodyToText(flow.response)}</div>
          </>
        )}
        {tab === "timing" && (
          <div>
            sent: {flow.timings.sent ?? "-"} · ttfb: {flow.timings.ttfb ?? "-"} · done: {flow.timings.done ?? "-"}
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Компоновка в App.tsx**

Заменить содержимое `src/App.tsx`:

```tsx
import { useEffect, useState } from "react";
import { TrafficList } from "./components/TrafficList";
import { FlowDetail } from "./components/FlowDetail";
import { useFlows } from "./store";
import "./App.css";

function App() {
  const init = useFlows((s) => s.init);
  const startProxy = useFlows((s) => s.startProxy);
  const stopProxy = useFlows((s) => s.stopProxy);
  const [running, setRunning] = useState(false);
  const [addr, setAddr] = useState<string>("");

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    init().then((c) => (cleanup = c));
    return () => cleanup?.();
  }, [init]);

  const toggle = async () => {
    if (running) {
      await stopProxy();
      setRunning(false);
      setAddr("");
    } else {
      const a = await startProxy(8888);
      setRunning(true);
      setAddr(a);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100vh", color: "#ddd", background: "#1e1e1e" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: 8, borderBottom: "1px solid #333" }}>
        <button onClick={toggle}>{running ? "Stop" : "Start"} proxy</button>
        {addr && <span>Proxy: {addr}</span>}
      </div>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <div style={{ width: "45%", borderRight: "1px solid #333" }}>
          <TrafficList />
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <FlowDetail />
        </div>
      </div>
    </div>
  );
}

export default App;
```

- [ ] **Step 4: Проверка типов**

Run: `pnpm exec tsc --noEmit`
Expected: без ошибок.

- [ ] **Step 5: End-to-end проверка вручную**

Run: `pnpm tauri dev`
- Нажать «Start proxy» → появляется `Proxy: 0.0.0.0:8888`.
- В терминале: `curl -x http://127.0.0.1:8888 http://example.com/`
- В окне: в списке появляется строка `GET 200 example.com /`; клик по ней показывает заголовки и тело ответа.
- Нажать «Stop proxy».

Expected: всё вышеописанное работает.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: UI просмотра трафика — TrafficList, FlowDetail, компоновка"
```

---

## Definition of Done (Phase 1)

- `pnpm tauri dev` открывает окно; кнопка Start/Stop управляет прокси.
- HTTP-запросы, проходящие через `127.0.0.1:8888`, появляются в списке в реальном времени.
- Клик по строке показывает заголовки/тело запроса и ответа.
- `cargo test` (в `src-tauri`) — все юнит- и интеграционные тесты зелёные.
- `pnpm exec tsc --noEmit` — без ошибок типов.

## Что НЕ входит в Phase 1 (следующие планы)

- Phase 2: персистентный CA + `/cert`-эндпоинт + `SetupPanel` + реальный HTTPS-перехват на устройстве.
- Phase 3: фильтр/поиск. Phase 4: повтор/Composer. Phase 5: breakpoints. Phase 6: экспорт (HAR/curl) + save/load сессии.
