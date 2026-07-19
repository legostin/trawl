# http-catch — Phase 2: HTTPS MITM + Setup — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать приложению постоянный CA, раздавать его сертификат клиентам (телефону) через прокси, и добавить `SetupPanel`, который проводит пользователя через настройку прокси и установку сертификата на устройстве — чтобы HTTPS-трафик телефона расшифровывался и попадал в список.

**Architecture:** CA теперь генерируется один раз и хранится на диске в app data dir; при старте прокси он загружается из файлов. Прокси-хендлер перехватывает запросы к «магическому» хосту `http-catch` и отдаёт публичный CA-сертификат для скачивания (по образцу mitm.it). Фронтенд получает LAN-IP и порт через команду и показывает пошаговый `SetupPanel` с QR-кодом и индикатором проверки.

**Tech Stack:** дополнительно к Phase 1 — `rcgen` (load/serialize PEM), `qrcode` (npm, генерация QR на фронте). Определение LAN-IP — стандартной библиотекой (UDP-сокет), без новых crate.

## Global Constraints

- **Платформа:** macOS. **JS-менеджер:** `pnpm`. **Rust edition:** 2021.
- **Порт прокси:** `8888`, слушать `0.0.0.0`.
- **Магический хост для скачивания CA:** `http-catch` (запрос `http://http-catch/` через настроенный прокси отдаёт сертификат).
- **Файлы CA:** `<app_data_dir>/ca/ca.key` (приватный ключ, PEM) и `<app_data_dir>/ca/ca.pem` (публичный сертификат, PEM). Права ключа — `0600`.
- **Единая модель `Flow`** и **имена событий** (`flow-added`, `flow-updated`) — как в Phase 1, не менять.
- **TDD**, частые коммиты.
- **hudsucker 0.22.0 API** (сверено в Phase 1): builder-порядок `with_listener → with_rustls_client → with_ca → with_http_handler → with_graceful_shutdown → build`; `RcgenAuthority::new(key_pair, ca_cert, cache_size)`; `build()` возвращает `Proxy` (не `Result`). Из `handle_request` можно вернуть готовый ответ через `RequestOrResponse::Response(resp)`.

---

## File Structure

**Rust (`src-tauri/src/`):**
- `ca.rs` — расширяется: постоянный CA (load-or-create, сериализация PEM). Функция `generate_ephemeral_ca` заменяется на `load_or_create_ca(dir)`.
- `proxy.rs` — `start()` принимает загруженный CA; хендлер получает поле `ca_pem` и перехватывает магический хост.
- `commands.rs` — новые команды `get_setup_info`, `get_ca_pem`, `ca_cert_path`; `start_proxy` вычисляет `ca_dir` из app data dir; хелпер `lan_ip()`.
- `net.rs` (новый) — определение LAN-IP через UDP-сокет.

**Frontend (`src/`):**
- `setup.ts` (новый) — типы и обёртки команд setup (`SetupInfo`, `getSetupInfo`, `getCaPem`).
- `components/SetupPanel.tsx` (новый) — пошаговый экран настройки + QR + индикатор проверки.
- `App.tsx` — переключатель вида «Traffic / Setup».

---

### Task 1: Постоянный CA (load-or-create + сериализация)

**Files:**
- Modify: `src-tauri/src/ca.rs`

**Interfaces:**
- Consumes: ничего нового.
- Produces:
  - `pub struct CaMaterial { pub key_pair: rcgen::KeyPair, pub ca_cert: rcgen::Certificate, pub cert_pem: String }`
  - `pub fn load_or_create_ca(dir: &std::path::Path) -> anyhow::Result<CaMaterial>` — если в `dir` есть `ca.key` и `ca.pem`, загружает их; иначе генерирует, создаёт `dir`, пишет оба файла (ключ с правами 0600) и возвращает материал.
  - Функция `generate_ephemeral_ca` удаляется (её единственный потребитель — `proxy::start` — переводится на `load_or_create_ca` в Task 2).

- [ ] **Step 1: Заменить содержимое `ca.rs` тестами и новым API (падающий тест)**

Полностью заменить `src-tauri/src/ca.rs`:

```rust
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};

pub struct CaMaterial {
    pub key_pair: KeyPair,
    pub ca_cert: rcgen::Certificate,
    /// Публичный сертификат в PEM — раздаётся клиентам для установки.
    pub cert_pem: String,
}

/// Загружает CA из `dir/ca.key` + `dir/ca.pem`, либо создаёт новый и сохраняет его.
pub fn load_or_create_ca(dir: &Path) -> Result<CaMaterial> {
    let key_path = dir.join("ca.key");
    let cert_path = dir.join("ca.pem");

    if key_path.exists() && cert_path.exists() {
        let key_pem = fs::read_to_string(&key_path).context("read ca.key")?;
        let cert_pem = fs::read_to_string(&cert_path).context("read ca.pem")?;
        let key_pair = KeyPair::from_pem(&key_pem).context("parse ca.key")?;
        let ca_cert = CertificateParams::from_ca_cert_pem(&cert_pem)
            .context("parse ca.pem")?
            .self_signed(&key_pair)
            .context("re-sign ca")?;
        return Ok(CaMaterial { key_pair, ca_cert, cert_pem });
    }

    let (key_pair, ca_cert) = generate_ca()?;
    let cert_pem = ca_cert.pem();
    let key_pem = key_pair.serialize_pem();

    fs::create_dir_all(dir).context("create ca dir")?;
    fs::write(&cert_path, &cert_pem).context("write ca.pem")?;
    fs::write(&key_path, &key_pem).context("write ca.key")?;
    set_key_permissions(&key_path)?;

    Ok(CaMaterial { key_pair, ca_cert, cert_pem })
}

fn generate_ca() -> Result<(KeyPair, rcgen::Certificate)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "http-catch CA");
    dn.push(DnType::OrganizationName, "http-catch");
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((key_pair, cert))
}

#[cfg(unix)]
fn set_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_key_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_ca_files_when_absent() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-ca-test-a-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let mat = load_or_create_ca(&tmp).unwrap();
        assert!(mat.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(tmp.join("ca.key").exists());
        assert!(tmp.join("ca.pem").exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn reuses_existing_ca_on_second_call() {
        let tmp = std::env::temp_dir().join(format!("httpcatch-ca-test-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let first = load_or_create_ca(&tmp).unwrap().cert_pem;
        let second = load_or_create_ca(&tmp).unwrap().cert_pem;
        assert_eq!(first, second, "CA должен переиспользоваться, а не пересоздаваться");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
```

- [ ] **Step 2: Запустить тесты — убедиться, что падают компиляцией**

Run: `cd src-tauri && cargo test ca::tests`
Expected: FAIL — `proxy.rs` ещё вызывает удалённую `generate_ephemeral_ca` (ошибка компиляции). Это ожидаемо; чинится в Task 2. Если хочется изолированно проверить только ca.rs логику — временно закомментировать `mod proxy;` в `lib.rs` не нужно; переходим к Task 2, где всё сойдётся.

> Примечание: Task 1 и Task 2 связаны одним изменением сигнатуры CA. Коммит делаем в конце Task 2, когда крейт снова компилируется. Здесь коммита нет.

---

### Task 2: Прокси на постоянном CA + раздача сертификата по `/cert`

**Files:**
- Modify: `src-tauri/src/proxy.rs`

**Interfaces:**
- Consumes: `ca::load_or_create_ca`, `ca::CaMaterial`.
- Produces:
  - Новая сигнатура: `pub async fn start(addr: SocketAddr, store: FlowStore, emit: EmitFn, ca_dir: PathBuf) -> Result<ProxyHandle>`.
  - Хендлер отвечает на любой проксированный запрос к хосту `http-catch` ответом `200` с телом = публичный CA PEM и заголовками для скачивания.

- [ ] **Step 1: Добавить магический хост и поле `ca_pem` в хендлер (обновить `proxy.rs`)**

В `src-tauri/src/proxy.rs` внести изменения:

1. Обновить `use` для CA и путей — заменить строку `use crate::ca::generate_ephemeral_ca;` на:

```rust
use std::path::PathBuf;

use crate::ca::load_or_create_ca;
```

2. Добавить поле в структуру хендлера:

```rust
#[derive(Clone)]
struct CaptureHandler {
    store: FlowStore,
    emit: EmitFn,
    current_id: Option<u64>,
    ca_pem: String,
}
```

3. В самое начало `handle_request` (до чтения тела) добавить перехват магического хоста:

```rust
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        req: Request<Body>,
    ) -> RequestOrResponse {
        // Раздача CA-сертификата: клиент с настроенным прокси открывает http://http-catch/
        if req.uri().host() == Some("http-catch") {
            let body = Body::from(Full::new(Bytes::from(self.ca_pem.clone().into_bytes())));
            let resp = Response::builder()
                .status(200)
                .header("content-type", "application/x-x509-ca-cert")
                .header(
                    "content-disposition",
                    "attachment; filename=\"http-catch-ca.pem\"",
                )
                .body(body)
                .expect("build cert response");
            return RequestOrResponse::Response(resp);
        }

        let (parts, body) = req.into_parts();
        // ... остальной код без изменений
```

4. Обновить `start()` — грузить постоянный CA и прокидывать `ca_pem` в хендлер:

```rust
pub async fn start(
    addr: SocketAddr,
    store: FlowStore,
    emit: EmitFn,
    ca_dir: PathBuf,
) -> Result<ProxyHandle> {
    let ca = load_or_create_ca(&ca_dir)?;
    let authority = RcgenAuthority::new(ca.key_pair, ca.ca_cert, 1_000);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let handler = CaptureHandler {
        store,
        emit,
        current_id: None,
        ca_pem: ca.cert_pem,
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
```

- [ ] **Step 2: Обновить существующий интеграционный тест под новую сигнатуру и добавить тест `/cert`**

В `proxy.rs` в модуле `tests`:

1. В тесте `captures_http_flow_through_proxy` заменить вызов `start` — добавить временную ca-директорию:

```rust
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-proxy-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start(proxy_addr, store.clone(), emit, ca_dir.clone()).await.unwrap();
```

и в конце теста, после `handle.stop();`, добавить очистку: `let _ = std::fs::remove_dir_all(&ca_dir);`

2. Добавить новый тест раздачи сертификата:

```rust
    #[tokio::test]
    async fn serves_ca_pem_on_magic_host() {
        let store = FlowStore::new(10);
        let emit: EmitFn = Arc::new(|_e, _f| {});
        let ca_dir = std::env::temp_dir().join(format!("httpcatch-cert-ca-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ca_dir);
        let handle = start("127.0.0.1:0".parse().unwrap(), store, emit, ca_dir.clone())
            .await
            .unwrap();
        let bound = handle.local_addr();

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://{bound}")).unwrap())
            .build()
            .unwrap();
        let resp = client.get("http://http-catch/").send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let text = resp.text().await.unwrap();
        assert!(text.contains("BEGIN CERTIFICATE"), "got: {text}");

        handle.stop();
        let _ = std::fs::remove_dir_all(&ca_dir);
    }
```

- [ ] **Step 3: Обновить вызов `start` в `commands.rs` (иначе крейт не собирается)**

В `src-tauri/src/commands.rs` в `start_proxy` временно (до Task 3) прокинуть ca_dir. Полноценно это делает Task 3; чтобы крейт собрался сейчас, добавить импорт и вычисление пути:

```rust
use tauri::Manager;
```

и в `start_proxy` перед вызовом `proxy::start` вычислить каталог:

```rust
    let ca_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca");
    let handle = proxy::start(addr, state.store.clone(), emit, ca_dir)
        .await
        .map_err(|e| e.to_string())?;
```

- [ ] **Step 4: Запустить тесты**

Run: `cd src-tauri && cargo test proxy::tests`
Expected: PASS — `captures_http_flow_through_proxy` и `serves_ca_pem_on_magic_host` зелёные.

Затем весь набор: `cargo test`
Expected: все тесты (ca + store + model + proxy) зелёные.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: постоянный CA на диске + раздача сертификата через http://http-catch"
```

---

### Task 3: LAN-IP и команды setup

**Files:**
- Create: `src-tauri/src/net.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (`mod net;`, регистрация новых команд)

**Interfaces:**
- Consumes: `ca::load_or_create_ca`, `net::lan_ip`.
- Produces Tauri-команды:
  - `get_setup_info(app) -> Result<SetupInfo, String>` где `SetupInfo { lan_ip: Option<String>, port: u16, cert_host: String }` (`cert_host` = `"http-catch"`).
  - `get_ca_pem(app) -> Result<String, String>` — публичный CA PEM (создаёт CA при необходимости).
  - `ca_cert_path(app) -> Result<String, String>` — путь к `ca.pem` на диске.

- [ ] **Step 1: Определение LAN-IP (падающий тест)**

`src-tauri/src/net.rs`:

```rust
use std::net::{IpAddr, UdpSocket};

/// Определяет локальный IP, используемый для исходящих соединений в LAN.
/// UDP `connect` не отправляет пакетов — только выбирает маршрут/интерфейс.
pub fn lan_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_private_or_real_ip() {
        // На машине с сетью должен вернуться какой-то не-loopback IPv4/IPv6.
        // Тест не должен падать при отсутствии сети — тогда допустим None.
        if let Some(ip) = lan_ip() {
            assert!(!ip.is_loopback(), "ожидали не-loopback, получили {ip}");
        }
    }
}
```

- [ ] **Step 2: Запустить тест**

Run: `cd src-tauri && cargo test net::tests` (сначала добавить `mod net;` в `lib.rs`)
Expected: PASS.

- [ ] **Step 3: Добавить команды setup в `commands.rs`**

В `src-tauri/src/commands.rs` добавить (import `Manager` уже добавлен в Task 2):

```rust
use serde::Serialize;

use crate::ca::load_or_create_ca;
use crate::net::lan_ip;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub lan_ip: Option<String>,
    pub port: u16,
    pub cert_host: String,
}

fn ca_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("ca"))
}

#[tauri::command]
pub fn get_setup_info(app: AppHandle) -> Result<SetupInfo, String> {
    Ok(SetupInfo {
        lan_ip: lan_ip().map(|ip| ip.to_string()),
        port: 8888,
        cert_host: "http-catch".into(),
    })
}

#[tauri::command]
pub fn get_ca_pem(app: AppHandle) -> Result<String, String> {
    let mat = load_or_create_ca(&ca_dir(&app)?).map_err(|e| e.to_string())?;
    Ok(mat.cert_pem)
}

#[tauri::command]
pub fn ca_cert_path(app: AppHandle) -> Result<String, String> {
    let dir = ca_dir(&app)?;
    // гарантируем, что файл существует
    load_or_create_ca(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ca.pem").to_string_lossy().to_string())
}
```

Также заменить дублирование `ca_dir` в `start_proxy` (из Task 2) на вызов этого хелпера:

```rust
    let handle = proxy::start(addr, state.store.clone(), emit, ca_dir(&app)?)
        .await
        .map_err(|e| e.to_string())?;
```

(Удалить ранее добавленный inline-блок с `app.path().app_data_dir()...join("ca")` в `start_proxy`.)

- [ ] **Step 4: Зарегистрировать команды и модуль в `lib.rs`**

`src-tauri/src/lib.rs` — добавить `mod net;` и расширить `generate_handler!`:

```rust
mod ca;
mod commands;
mod model;
mod net;
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
            commands::get_setup_info,
            commands::get_ca_pem,
            commands::ca_cert_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Сборка + тесты**

Run: `cd src-tauri && cargo build && cargo test`
Expected: сборка без ошибок; все тесты зелёные. Предупреждение о неиспользуемом `app` в `get_setup_info` — убрать, переименовав параметр в `_app` если clippy/варнинг мешает (функция не использует `app`).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: команды setup (LAN-IP, CA PEM, путь к сертификату)"
```

---

### Task 4: SetupPanel UI (инструкция, QR, проверка)

**Files:**
- Create: `src/setup.ts`
- Create: `src/components/SetupPanel.tsx`
- Modify: `src/App.tsx`
- Modify: `package.json` (добавить `qrcode`)

**Interfaces:**
- Consumes: команды `get_setup_info`, `get_ca_pem`, `ca_cert_path`; стор `useFlows` (для индикатора «первый HTTPS-поток пойман»).
- Produces: экран Setup с шагами, QR-кодом на `http://http-catch/`, путём к сертификату и живым индикатором проверки.

- [ ] **Step 1: Добавить npm-зависимость qrcode**

Run (из корня репо): `pnpm add qrcode && pnpm add -D @types/qrcode`
Expected: пакеты установлены.

- [ ] **Step 2: Обёртки команд setup**

`src/setup.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export interface SetupInfo {
  lanIp: string | null;
  port: number;
  certHost: string;
}

export const getSetupInfo = (): Promise<SetupInfo> => invoke<SetupInfo>("get_setup_info");
export const getCaPem = (): Promise<string> => invoke<string>("get_ca_pem");
export const caCertPath = (): Promise<string> => invoke<string>("ca_cert_path");
```

- [ ] **Step 3: SetupPanel**

`src/components/SetupPanel.tsx`:

```tsx
import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { useFlows } from "../store";
import { getSetupInfo, caCertPath, type SetupInfo } from "../setup";

export function SetupPanel() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [certPath, setCertPath] = useState<string>("");
  const [qr, setQr] = useState<string>("");
  const httpsSeen = useFlows((s) => s.flows.some((f) => f.url.scheme === "https"));

  useEffect(() => {
    getSetupInfo().then(setInfo);
    caCertPath().then(setCertPath);
    QRCode.toDataURL("http://http-catch/").then(setQr).catch(() => setQr(""));
  }, []);

  const ip = info?.lanIp ?? "<нет сети>";
  const port = info?.port ?? 8888;

  return (
    <div style={{ padding: 16, overflow: "auto", height: "100%", fontSize: 13, lineHeight: 1.5 }}>
      <h2 style={{ marginTop: 0 }}>Настройка перехвата трафика телефона</h2>

      <ol style={{ paddingLeft: 20 }}>
        <li>
          Телефон и этот Mac должны быть в одной Wi-Fi-сети. Адрес прокси:{" "}
          <code style={{ fontSize: 15, background: "#333", padding: "2px 6px" }}>
            {ip}:{port}
          </code>
        </li>
        <li>
          На телефоне: Wi-Fi → настройки сети → HTTP-прокси <b>вручную</b> → впишите IP{" "}
          <code>{ip}</code> и порт <code>{port}</code>.
        </li>
        <li>
          Установите CA-сертификат. На телефоне откройте{" "}
          <code>http://http-catch/</code> (отсканируйте QR) — сертификат скачается.
          <div style={{ marginTop: 8 }}>
            {qr && <img src={qr} width={160} height={160} alt="QR http://http-catch/" />}
          </div>
          <div style={{ opacity: 0.75, marginTop: 4 }}>
            Файл сертификата на диске: <code>{certPath}</code>
          </div>
        </li>
        <li>
          <b>Доверьте</b> сертификат вручную:
          <ul>
            <li>iOS: Settings → General → About → Certificate Trust Settings → включить http-catch CA.</li>
            <li>Android: Settings → Security → Install a certificate → CA certificate.</li>
          </ul>
        </li>
      </ol>

      <div
        style={{
          marginTop: 12,
          padding: 10,
          borderRadius: 6,
          background: httpsSeen ? "#1e4d2b" : "#3a3a1e",
        }}
      >
        {httpsSeen
          ? "✓ HTTPS-трафик расшифровывается — всё работает."
          : "Ожидание первого расшифрованного HTTPS-запроса…"}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Переключатель вида в App.tsx**

Заменить содержимое `src/App.tsx` (добавляется состояние вкладки Traffic/Setup):

```tsx
import { useEffect, useState } from "react";
import { TrafficList } from "./components/TrafficList";
import { FlowDetail } from "./components/FlowDetail";
import { SetupPanel } from "./components/SetupPanel";
import { useFlows } from "./store";
import "./App.css";

type View = "traffic" | "setup";

function App() {
  const init = useFlows((s) => s.init);
  const startProxy = useFlows((s) => s.startProxy);
  const stopProxy = useFlows((s) => s.stopProxy);
  const [running, setRunning] = useState(false);
  const [addr, setAddr] = useState<string>("");
  const [view, setView] = useState<View>("traffic");

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
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        color: "#ddd",
        background: "#1e1e1e",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: 8,
          borderBottom: "1px solid #333",
        }}
      >
        <button onClick={toggle}>{running ? "Stop" : "Start"} proxy</button>
        {addr && <span>Proxy: {addr}</span>}
        <div style={{ flex: 1 }} />
        <button
          onClick={() => setView("traffic")}
          style={{ fontWeight: view === "traffic" ? "bold" : "normal" }}
        >
          Traffic
        </button>
        <button
          onClick={() => setView("setup")}
          style={{ fontWeight: view === "setup" ? "bold" : "normal" }}
        >
          Setup
        </button>
      </div>

      {view === "setup" ? (
        <SetupPanel />
      ) : (
        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          <div style={{ width: "45%", borderRight: "1px solid #333" }}>
            <TrafficList />
          </div>
          <div style={{ flex: 1, minWidth: 0 }}>
            <FlowDetail />
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
```

- [ ] **Step 5: Проверка типов и сборка**

Run (из корня): `pnpm exec tsc --noEmit && pnpm build`
Expected: без ошибок типов; сборка проходит.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: SetupPanel — инструкция, QR, индикатор проверки HTTPS"
```

---

## Ручная проверка HTTPS end-to-end (обязательна перед завершением фазы)

Автоматический тест сквозного HTTPS-перехвата не входит в набор: hudsucker валидирует TLS-сертификат вышестоящего сервера по webpki-корням, поэтому локальный self-signed upstream в юнит-тесте потребовал бы тяжёлой TLS-обвязки с внедрением доверенного корня в клиент прокси. Вместо этого — ручной smoke на реальном доверенном сайте:

1. `pnpm tauri dev`, нажать **Start proxy**.
2. Найти путь сертификата: вкладка **Setup** → строка «Файл сертификата на диске».
3. В терминале выполнить HTTPS-запрос через прокси, доверяя нашему CA:

   ```bash
   curl -x http://127.0.0.1:8888 --cacert "<путь к ca.pem>" https://example.com/
   ```

   Ожидаемо: `curl` получает страницу (значит MITM с нашим CA работает), а во вкладке **Traffic** появляется запрос со `scheme = https`, вкладка **Setup** показывает «✓ HTTPS-трафик расшифровывается».
4. (Опционально, реальный сценарий) Настроить прокси на телефоне, открыть `http://http-catch/`, установить и **доверить** сертификат, затем открыть любой https-сайт/приложение — запросы появляются в списке.

## Definition of Done (Phase 2)

- CA создаётся один раз и переиспользуется между запусками (юнит-тест).
- `http://http-catch/` через прокси отдаёт публичный CA PEM (юнит-тест).
- `SetupPanel` показывает LAN-IP, порт, QR, путь к сертификату и индикатор проверки.
- `cargo test` и `pnpm exec tsc --noEmit` зелёные.
- Ручной HTTPS-smoke (выше) пройден: `curl --cacert` через прокси на https-сайт даёт расшифрованный поток в списке.

## Вне рамок Phase 2 (следующие планы)

- Phase 3: фильтр/поиск. Phase 4: повтор/Composer. Phase 5: breakpoints. Phase 6: экспорт (HAR/curl) + save/load сессии.
