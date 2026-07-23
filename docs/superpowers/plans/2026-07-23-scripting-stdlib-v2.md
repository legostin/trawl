# Scripting stdlib v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** JSONPath-ядро (RFC 9535) для правил trawl: `patch`/`pick`/`removeAt`/`mergeAt` + 4 домена хелперов, подсказки путей в Monaco, валидация при сохранении, dry-run на захваченном трафике, единая справка (UI + MCP + cookbook), полное тестовое покрытие.

**Architecture:** Rust-крейт `serde_json_path` парсит пути и возвращает нормализованные локации (JSON Pointer) через нативную функцию `__native_jsonpath_locate`; JS-stdlib применяет мутации по локациям. stdlib выносится из строки в `src-tauri/js/stdlib.js` (`include_str!`). Один Rust-парсер обслуживает рантайм, валидацию при сохранении, dry-run и подсказки.

**Tech Stack:** Rust (rquickjs 0.9, serde_json_path 0.7, regex 1 — уже в deps), TypeScript/React (Monaco via @monaco-editor/react), vitest.

**Spec:** `docs/superpowers/specs/2026-07-23-scripting-stdlib-v2-design.md`

## Global Constraints

- Fail-closed: ошибка скрипта / `patch` с 0 узлов → flow в `error` (текущее поведение прокси не смягчать).
- Обратная совместимость: сигнатуры и поведение существующих 13 функций stdlib не меняются.
- Ведущий `$.` в путях опционален: нормализация `items[*]` → `$.items[*]`, `[0].x` → `$[0].x` — только на Rust-стороне (`jsonpath::normalize`).
- Тексты ошибок и UI — на русском, в стиле существующих («handler не вернул ответ…»).
- Rust-тесты: `cd src-tauri && cargo test`. Frontend: `pnpm vitest run`.
- Коммит после каждой задачи.
- Path-функции с литеральным путём (для валидации/подсказок): `patch`, `tryPatch`, `pick`, `pickOne`, `removeAt`, `mergeAt` — этот список повторяется в Rust-регэкспе и TS (`pathContext.ts`), менять синхронно.

---

### Task 1: Вынос STD_LIB в src-tauri/js/stdlib.js

**Files:**
- Create: `src-tauri/js/stdlib.js`
- Modify: `src-tauri/src/scripting.rs:129-145`

**Interfaces:**
- Produces: `src-tauri/js/stdlib.js` — единственный источник stdlib; все последующие задачи дописывают функции в этот файл.

- [ ] **Step 1: Создать `src-tauri/js/stdlib.js`** с текущим содержимым STD_LIB, читаемо отформатированным. Логика функций — идентична строке из `scripting.rs:132-144` (существующие тесты это проверят):

```js
// Стандартная библиотека правил. Инжектируется перед каждым скриптом (обе фазы).
// Декларации для автокомплита: src/scripting/stdlib.ts (STD_DTS) — менять синхронно.
// Функции с префиксом __ — внутренние, в справку не попадают.

// ── Заголовки ──
function __lc(o) { var r = {}; for (var k in o) { r[k.toLowerCase()] = k; } return r; }
function header(msg, name) {
  if (!msg || !msg.headers) return undefined;
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  return k ? msg.headers[k] : undefined;
}
function hasHeader(msg, name) { return header(msg, name) !== undefined; }
function setHeader(msg, name, value) {
  if (!msg.headers) msg.headers = {};
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  if (k) delete msg.headers[k];
  msg.headers[name] = String(value);
}
function removeHeader(msg, name) {
  if (!msg || !msg.headers) return;
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  if (k) delete msg.headers[k];
}

// ── Тело ──
function jsonBody(msg) { try { return JSON.parse((msg && msg.body) || 'null'); } catch (e) { return null; } }
function setJsonBody(msg, obj) {
  msg.body = JSON.stringify(obj);
  if (!hasHeader(msg, 'content-type')) setHeader(msg, 'content-type', 'application/json');
}

// ── Запрос ──
function bearer(token) { setHeader(request, 'authorization', 'Bearer ' + token); }
function queryParam(req, name) {
  var q = (req.path || '').split('?')[1] || '';
  var parts = q.split('&');
  for (var i = 0; i < parts.length; i++) {
    var kv = parts[i].split('=');
    if (decodeURIComponent(kv[0]) === name) return decodeURIComponent((kv[1] || '').replace(/\+/g, ' '));
  }
  return undefined;
}

// ── handler-фаза ──
function sendJsonRequest(req) {
  var r = send(req);
  try { r.data = JSON.parse(r.body || 'null'); } catch (e) { r.data = null; }
  return r;
}
function sendWithRetry(req, opts) {
  opts = opts || {};
  var max = opts.retries || 3;
  var delay = opts.delay || 1000;
  var r = send(req);
  var n = 0;
  while (n < max && (r.status === 429 || r.status >= 500)) { sleep(delay); r = send(req); n++; }
  return r;
}

// ── Прочее ──
function secret(name) {
  var v = __native_secret(String(name));
  return (v === undefined || v === null) ? null : v;
}
function notify(text, opts) {
  opts = opts || {};
  ctx.__notifications.push({ text: String(text), channel: opts.channel, title: opts.title });
}
```

- [ ] **Step 2: Заменить константу в `scripting.rs`.** Удалить строки 131-145 (`const STD_LIB: &str = r#"..."#;`), вместо них:

```rust
/// Built-in helper functions injected before every rule (both phases). Source:
/// src-tauri/js/stdlib.js. Kept in sync with the declarations shown for
/// autocomplete in `src/scripting/stdlib.ts`.
const STD_LIB: &str = include_str!("../js/stdlib.js");
```

- [ ] **Step 3: Прогнать тесты**

Run: `cd src-tauri && cargo test scripting`
Expected: все существующие тесты PASS (в т.ч. `stdlib_helpers_are_available`, `stdlib_header_helpers_case_insensitive`).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "refactor(scripting): stdlib из строки в src-tauri/js/stdlib.js (include_str)"
```

---

### Task 2: Модуль jsonpath.rs (serde_json_path)

**Files:**
- Modify: `src-tauri/Cargo.toml` (в `[dependencies]`)
- Create: `src-tauri/src/jsonpath.rs`
- Modify: `src-tauri/src/lib.rs` (объявить `mod jsonpath;` рядом с `mod scripting;`)

**Interfaces:**
- Produces:
  - `pub fn normalize(path: &str) -> String` — добавляет `$.`/`$` если путь не начинается с `$`.
  - `pub fn locate(doc_json: &str, path: &str) -> String` — JSON-строка `{"locations":["/items/0/price",…]}` (JSON Pointer, RFC 6901) либо `{"error":"…"}`.
  - `pub fn validate(path: &str) -> Option<String>` — `Some(текст ошибки)` для невалидного пути, `None` для валидного.

- [ ] **Step 1: Добавить зависимость** в `src-tauri/Cargo.toml` после строки `rquickjs = "0.9"`:

```toml
serde_json_path = "0.7"
```

- [ ] **Step 2: Написать падающие тесты** — создать `src-tauri/src/jsonpath.rs` только с тестами:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_dollar_prefix() {
        assert_eq!(normalize("items[*].price"), "$.items[*].price");
        assert_eq!(normalize("[0].x"), "$[0].x");
        assert_eq!(normalize("$.a"), "$.a");
        assert_eq!(normalize("$"), "$");
        assert_eq!(normalize("  a.b "), "$.a.b");
    }

    #[test]
    fn locate_returns_pointers_for_wildcard() {
        let doc = r#"{"items":[{"price":1},{"price":2}]}"#;
        let v: serde_json::Value = serde_json::from_str(&locate(doc, "items[*].price")).unwrap();
        let locs: Vec<&str> = v["locations"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        assert_eq!(locs, vec!["/items/0/price", "/items/1/price"]);
    }

    #[test]
    fn locate_supports_filters() {
        let doc = r#"{"items":[{"t":"a","p":1},{"t":"b","p":2}]}"#;
        let v: serde_json::Value = serde_json::from_str(&locate(doc, "items[?@.t=='b'].p")).unwrap();
        assert_eq!(v["locations"].as_array().unwrap().len(), 1);
        assert_eq!(v["locations"][0], "/items/1/p");
    }

    #[test]
    fn locate_root_is_empty_pointer() {
        let v: serde_json::Value = serde_json::from_str(&locate("{}", "$")).unwrap();
        assert_eq!(v["locations"][0], "");
    }

    #[test]
    fn locate_reports_bad_path_and_bad_doc() {
        let v: serde_json::Value = serde_json::from_str(&locate("{}", "$[")).unwrap();
        assert!(v["error"].is_string());
        let v: serde_json::Value = serde_json::from_str(&locate("not json", "$.a")).unwrap();
        assert!(v["error"].is_string());
    }

    #[test]
    fn validate_ok_and_error() {
        assert!(validate("items[*].price").is_none());
        assert!(validate("$..a[?@.b>1]").is_none());
        assert!(validate("$[").is_some());
    }
}
```

- [ ] **Step 3: Убедиться, что не компилируется**

Run: `cd src-tauri && cargo test jsonpath 2>&1 | head -5`
Expected: ошибки компиляции `cannot find function normalize/locate/validate`.

- [ ] **Step 4: Реализация** — в начало `jsonpath.rs`:

```rust
//! JSONPath (RFC 9535) для правил: locate/validate поверх serde_json_path.
//! Один парсер обслуживает рантайм stdlib, валидацию при сохранении,
//! dry-run и подсказки в редакторе.

use serde_json::json;
use serde_json_path::JsonPath;

/// Ведущий `$` опционален для эргономики: `items[*]` → `$.items[*]`.
pub fn normalize(path: &str) -> String {
    let p = path.trim();
    if p.starts_with('$') {
        p.to_string()
    } else if p.starts_with('[') {
        format!("${p}")
    } else {
        format!("$.{p}")
    }
}

/// Локации совпадений как JSON Pointer'ы: {"locations":["/items/0/price"]} | {"error":"…"}.
pub fn locate(doc_json: &str, path: &str) -> String {
    let doc: serde_json::Value = match serde_json::from_str(doc_json) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("тело не JSON: {e}") }).to_string(),
    };
    let jp = match JsonPath::parse(&normalize(path)) {
        Ok(p) => p,
        Err(e) => return json!({ "error": e.to_string() }).to_string(),
    };
    let ptrs: Vec<String> = jp.query_located(&doc).locations().map(|l| l.to_json_pointer()).collect();
    json!({ "locations": ptrs }).to_string()
}

/// None — путь валиден; Some(msg) — текст ошибки парсера.
pub fn validate(path: &str) -> Option<String> {
    JsonPath::parse(&normalize(path)).err().map(|e| e.to_string())
}
```

И в `src-tauri/src/lib.rs` рядом с остальными `mod`: `pub mod jsonpath;`.

- [ ] **Step 5: Прогнать тесты**

Run: `cd src-tauri && cargo test jsonpath`
Expected: 6 тестов PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/jsonpath.rs src-tauri/src/lib.rs
git commit -m "feat(scripting): jsonpath-модуль на serde_json_path (locate/validate/normalize)"
```

---

### Task 3: __native_jsonpath_locate в движках + patch/tryPatch

**Files:**
- Modify: `src-tauri/src/scripting.rs` (spawn_engine ~строка 90, execute_handler ~строка 304)
- Modify: `src-tauri/js/stdlib.js` (дописать в конец)
- Test: `src-tauri/src/scripting.rs` (модуль tests)

**Interfaces:**
- Consumes: `crate::jsonpath::locate` (Task 2).
- Produces (JS, для правил и последующих задач):
  - `patch(target, path, valueOrFn) -> number` — target: сообщение (есть строковый `body`) или распарсенный объект; 0 узлов → throw.
  - `tryPatch(target, path, valueOrFn) -> number` — 0 узлов допустимо.
  - Внутренние: `__doc(target) -> {doc, msg}` (кэш парса на сообщении, non-enumerable `__docCache`), `__locate(doc, path) -> сегменты[][]`, `__getAt(doc, loc)`, `__parentAt(doc, loc)`, `__shape(doc) -> string`, `__traceOp(op, path, nodes)`, `__syncDoc(d)`.

- [ ] **Step 1: Падающие тесты** — добавить в `mod tests` scripting.rs:

```rust
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
```

- [ ] **Step 2: Убедиться, что падают**

Run: `cd src-tauri && cargo test scripting 2>&1 | grep -E "FAILED|failed|error"`
Expected: новые тесты падают с `action == "error"` («patch is not defined»).

- [ ] **Step 3: Зарегистрировать нативную функцию в обоих движках.** В `spawn_engine` (scripting.rs, внутри `ctx.with(|c| { … })` после `__native_secret`):

```rust
                let jp = Function::new(c.clone(), |doc: String, path: String| -> String {
                    crate::jsonpath::locate(&doc, &path)
                })
                .expect("bind jsonpath fn");
                c.globals().set("__native_jsonpath_locate", jp).expect("set jsonpath fn");
```

В `execute_handler` (после блока `__native_secret`):

```rust
        let jp_fn = match Function::new(c.clone(), |doc: String, path: String| -> String {
            crate::jsonpath::locate(&doc, &path)
        }) {
            Ok(f) => f,
            Err(e) => return ScriptResult::error(format!("bind jsonpath: {e}")),
        };
        let _ = g.set("__native_jsonpath_locate", jp_fn);
```

- [ ] **Step 4: Дописать ядро в `src-tauri/js/stdlib.js`:**

```js
// ── JSONPath-ядро (RFC 9535, парсер на стороне Rust) ──
// Сообщение отличаем от распарсенного объекта по строковому body + headers.
function __isMsg(x) {
  return !!(x && typeof x === 'object' && typeof x.body === 'string' && typeof x.headers === 'object');
}
// Распарсенный док кэшируется на сообщении (non-enumerable — не попадёт в сериализацию).
function __doc(target) {
  if (!__isMsg(target)) return { doc: target, msg: null };
  if (target.__docCache === undefined) {
    Object.defineProperty(target, '__docCache', {
      value: jsonBody(target), writable: true, configurable: true, enumerable: false,
    });
  }
  return { doc: target.__docCache, msg: target };
}
function __syncDoc(d) {
  if (d.msg) { d.msg.__docCache = d.doc; setJsonBody(d.msg, d.doc); }
}
// Rust возвращает JSON Pointer'ы; разворачиваем в массивы сегментов (~0/~1 по RFC 6901).
function __locate(doc, path) {
  var res = JSON.parse(__native_jsonpath_locate(JSON.stringify(doc === undefined ? null : doc), String(path)));
  if (res.error) throw new Error('JSONPath "' + path + '": ' + res.error);
  return res.locations.map(function (ptr) {
    if (ptr === '') return [];
    return ptr.split('/').slice(1).map(function (s) { return s.replace(/~1/g, '/').replace(/~0/g, '~'); });
  });
}
function __getAt(doc, loc) {
  var v = doc;
  for (var i = 0; i < loc.length; i++) { if (v == null) return undefined; v = v[loc[i]]; }
  return v;
}
function __parentAt(doc, loc) {
  var v = doc;
  for (var i = 0; i < loc.length - 1; i++) { v = v[loc[i]]; }
  return v;
}
// Срез структуры верхнего уровня для диагностики: { status, items[20], … }
function __shape(v) {
  if (v === null || v === undefined) return String(v);
  if (Array.isArray(v)) return '[' + v.length + ' элементов]';
  if (typeof v !== 'object') return typeof v;
  var ks = Object.keys(v), parts = [];
  for (var i = 0; i < Math.min(ks.length, 8); i++) {
    var k = ks[i], x = v[k];
    parts.push(Array.isArray(x) ? k + '[' + x.length + ']' : k);
  }
  if (ks.length > 8) parts.push('…');
  return '{ ' + parts.join(', ') + ' }';
}
// Трасса операций (ctx.__trace подключается в Task 9; до того — тихий no-op).
function __traceOp(op, path, nodes) {
  try { if (typeof ctx !== 'undefined' && ctx.__trace) ctx.__trace.push({ op: op, path: String(path), nodes: nodes }); } catch (e) {}
}

function __applyPatch(name, target, path, valueOrFn, minMatches) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (locs.length < minMatches) {
    throw new Error(name + '("' + path + '"): 0 узлов. Тело: ' + __shape(d.doc));
  }
  for (var i = 0; i < locs.length; i++) {
    var loc = locs[i];
    if (loc.length === 0) {
      d.doc = (typeof valueOrFn === 'function') ? valueOrFn(d.doc) : valueOrFn;
      continue;
    }
    var parent = __parentAt(d.doc, loc);
    var key = loc[loc.length - 1];
    parent[key] = (typeof valueOrFn === 'function') ? valueOrFn(parent[key]) : valueOrFn;
  }
  __syncDoc(d);
  __traceOp(name, path, locs.length);
  return locs.length;
}
// Записать значение/применить функцию во всех совпавших узлах. 0 узлов → ошибка.
function patch(target, path, valueOrFn) { return __applyPatch('patch', target, path, valueOrFn, 1); }
// То же, но отсутствие совпадений — не ошибка.
function tryPatch(target, path, valueOrFn) { return __applyPatch('tryPatch', target, path, valueOrFn, 0); }
```

- [ ] **Step 5: Прогнать тесты**

Run: `cd src-tauri && cargo test scripting`
Expected: все PASS, включая 8 новых.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): patch/tryPatch по JSONPath + __native_jsonpath_locate в обоих движках"
```

---

### Task 4: pick / pickOne / removeAt / mergeAt

**Files:**
- Modify: `src-tauri/js/stdlib.js`
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Consumes: `__doc`, `__locate`, `__getAt`, `__parentAt`, `__shape`, `__syncDoc`, `__traceOp` (Task 3).
- Produces (JS): `pick(target, path) -> any[]`, `pickOne(target, path) -> any|null`, `removeAt(target, path) -> number`, `mergeAt(target, path, obj) -> number` (0 узлов → throw), `__deepMerge(dst, src)`.

- [ ] **Step 1: Падающие тесты** в `mod tests`:

```rust
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
```

- [ ] **Step 2: Убедиться, что падают** — `cargo test scripting` → новые тесты FAIL («pick is not defined»).

- [ ] **Step 3: Реализация** — дописать в `stdlib.js`:

```js
// Все совпавшие значения массивом.
function pick(target, path) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  var out = [];
  for (var i = 0; i < locs.length; i++) out.push(__getAt(d.doc, locs[i]));
  __traceOp('pick', path, locs.length);
  return out;
}
// Первое совпавшее значение или null.
function pickOne(target, path) {
  var r = pick(target, path);
  return r.length ? r[0] : null;
}
// Удалить совпавшие узлы. Обратный порядок — чтобы splice не сдвигал ещё не обработанные индексы.
function removeAt(target, path) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  for (var i = locs.length - 1; i >= 0; i--) {
    var loc = locs[i];
    if (!loc.length) continue; // корень не удаляем
    var parent = __parentAt(d.doc, loc);
    var key = loc[loc.length - 1];
    if (Array.isArray(parent)) parent.splice(Number(key), 1);
    else delete parent[key];
  }
  __syncDoc(d);
  __traceOp('removeAt', path, locs.length);
  return locs.length;
}
function __deepMerge(dst, src) {
  for (var k in src) {
    var s = src[k];
    if (s && typeof s === 'object' && !Array.isArray(s) &&
        dst[k] && typeof dst[k] === 'object' && !Array.isArray(dst[k])) {
      __deepMerge(dst[k], s);
    } else {
      dst[k] = s;
    }
  }
  return dst;
}
// Deep-merge объекта в каждый совпавший узел. 0 узлов → ошибка.
function mergeAt(target, path, obj) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (!locs.length) throw new Error('mergeAt("' + path + '"): 0 узлов. Тело: ' + __shape(d.doc));
  for (var i = 0; i < locs.length; i++) {
    var n = __getAt(d.doc, locs[i]);
    if (n && typeof n === 'object') __deepMerge(n, obj);
  }
  __syncDoc(d);
  __traceOp('mergeAt', path, locs.length);
  return locs.length;
}
```

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): pick/pickOne/removeAt/mergeAt"
```

---

### Task 5: URL/query-хелперы

**Files:**
- Modify: `src-tauri/js/stdlib.js`
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Produces (JS): `setQueryParam(req, name, value)`, `removeQueryParam(req, name)`, `rewriteHost(req, host)`, `rewritePath(req, from, to)` (from: строка или RegExp), `pathSegments(req) -> string[]`, `__syncUrl(req)`.

- [ ] **Step 1: Падающие тесты:**

```rust
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
```

- [ ] **Step 2: Убедиться, что падают** — `cargo test scripting`.

- [ ] **Step 3: Реализация** — дописать в `stdlib.js`:

```js
// ── URL/query ──
// request.url и request.path должны меняться согласованно.
function __syncUrl(req) {
  if (req.url) {
    var m = String(req.url).match(/^(https?:\/\/[^\/]+)/);
    if (m) req.url = m[1] + req.path;
  }
}
function __splitPath(req) {
  var p = req.path || '';
  var i = p.indexOf('?');
  return { base: i < 0 ? p : p.slice(0, i), query: i < 0 ? '' : p.slice(i + 1) };
}
function __joinPath(req, base, parts) {
  req.path = parts.length ? base + '?' + parts.join('&') : base;
  __syncUrl(req);
}
function setQueryParam(req, name, value) {
  var sp = __splitPath(req);
  var parts = sp.query ? sp.query.split('&') : [];
  var enc = encodeURIComponent, found = false;
  for (var i = 0; i < parts.length; i++) {
    if (decodeURIComponent(parts[i].split('=')[0]) === name) { parts[i] = enc(name) + '=' + enc(String(value)); found = true; }
  }
  if (!found) parts.push(enc(name) + '=' + enc(String(value)));
  __joinPath(req, sp.base, parts);
}
function removeQueryParam(req, name) {
  var sp = __splitPath(req);
  var parts = (sp.query ? sp.query.split('&') : []).filter(function (p) {
    return decodeURIComponent(p.split('=')[0]) !== name;
  });
  __joinPath(req, sp.base, parts);
}
// Меняет host и авторити в url. Заголовок Host не трогаем — им управляет прокси.
function rewriteHost(req, host) {
  req.host = String(host);
  if (req.url) req.url = String(req.url).replace(/^(https?:\/\/)[^\/]+/, '$1' + host);
}
// from: строка (заменяются все вхождения) или RegExp. Query-часть не затрагивается.
function rewritePath(req, from, to) {
  var sp = __splitPath(req);
  var np = (from instanceof RegExp) ? sp.base.replace(from, to) : sp.base.split(from).join(to);
  req.path = sp.query ? np + '?' + sp.query : np;
  __syncUrl(req);
}
function pathSegments(req) {
  return ((req.path || '').split('?')[0]).split('/')
    .filter(function (s) { return s.length > 0; })
    .map(decodeURIComponent);
}
```

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS. (Если `set_query_param_adds_and_replaces` упал на кодировании пробела: `encodeURIComponent('а б')` даёт `%D0%B0%20%D0%B1` — сверить ожидание с фактическим и поправить тест, поведение функции менять не надо.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): URL/query-хелперы (setQueryParam, rewriteHost, rewritePath, pathSegments)"
```

---

### Task 6: Моки и ответы-сахар

**Files:**
- Modify: `src-tauri/js/stdlib.js`
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Produces (JS): `json(objOrStatus, obj?) -> response`, `textResponse(status, body, contentType?) -> response`, `httpError(status, msg?) -> response`, `delay(ms)`.
- Поведение: в request/response-фазе `json(...)` дополнительно вызывает `ctx.mock(...)`; в handler-фазе `ctx.mock` отсутствует — функция просто возвращает объект ответа для `return json(...)`. `delay` работает только в handler-фазе (`__native_sleep` есть только там), иначе бросает понятную ошибку.

- [ ] **Step 1: Падающие тесты:**

```rust
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
```

- [ ] **Step 2: Убедиться, что падают.**

- [ ] **Step 3: Реализация** — дописать в `stdlib.js`:

```js
// ── Моки и ответы ──
function __mockOrReturn(resp) {
  // request/response-фаза: ctx.mock существует → мок; handler: просто вернуть объект.
  if (typeof ctx !== 'undefined' && typeof ctx.mock === 'function') ctx.mock(resp);
  return resp;
}
// json({obj}) | json(status, {obj}) — JSON-ответ одной строкой.
function json(a, b) {
  var status = 200, obj = a;
  if (typeof a === 'number') { status = a; obj = b; }
  return __mockOrReturn({
    status: status,
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(obj === undefined ? null : obj),
  });
}
function textResponse(status, body, contentType) {
  return __mockOrReturn({
    status: status,
    headers: { 'content-type': contentType || 'text/plain; charset=utf-8' },
    body: String(body),
  });
}
function httpError(status, msg) {
  return json(status, { error: msg === undefined ? ('HTTP ' + status) : String(msg) });
}
// Блокирующая пауза. Только handler-фаза: request/response исполняются на общем потоке движка.
function delay(ms) {
  if (typeof __native_sleep !== 'function') {
    throw new Error('delay() доступен только в handler-фазе (phase: handler)');
  }
  __native_sleep(Math.max(0, Number(ms) || 0));
}
```

Примечание: в handler-фазе `sleep` объявлен локальной функцией в обёртке, но `__native_sleep` — глобальная нативная функция, поэтому `typeof`-проверка корректна в обеих фазах.

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): сахар для моков (json/textResponse/httpError/delay)"
```

---

### Task 7: Генерация данных

**Files:**
- Modify: `src-tauri/js/stdlib.js`
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Produces (JS): `uuid() -> string` (v4-формат), `randomInt(a, b) -> number` (включительно), `randomFrom(arr) -> any`, `nowISO(shift?, tz?) -> string` — `nowISO()` → UTC `…Z`; `nowISO('+2d')`; `nowISO(null, '+05:00')`; сдвиги `±N` + `s|m|h|d`.

- [ ] **Step 1: Падающие тесты:**

```rust
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
```

- [ ] **Step 2: Убедиться, что падают.**

- [ ] **Step 3: Реализация** — дописать в `stdlib.js`:

```js
// ── Генерация данных ──
function uuid() {
  var hex = '0123456789abcdef', s = [];
  for (var i = 0; i < 36; i++) s[i] = hex[Math.floor(Math.random() * 16)];
  s[14] = '4';
  s[19] = hex[(parseInt(s[19], 16) & 0x3) | 0x8];
  s[8] = s[13] = s[18] = s[23] = '-';
  return s.join('');
}
// Целое из [a, b] включительно.
function randomInt(a, b) { return Math.floor(Math.random() * (b - a + 1)) + a; }
function randomFrom(arr) { return arr[Math.floor(Math.random() * arr.length)]; }
// nowISO('+2d', '+05:00') → "2026-07-25T…+05:00"; без tz — UTC c 'Z'.
function nowISO(shift, tz) {
  var ms = Date.now();
  if (shift !== undefined && shift !== null) {
    var m = String(shift).match(/^([+-])(\d+)([smhd])$/);
    if (!m) throw new Error('nowISO: сдвиг вида "+2d", "-30m", "+1h", "+10s"');
    var mult = { s: 1e3, m: 6e4, h: 36e5, d: 864e5 }[m[3]];
    ms += (m[1] === '-' ? -1 : 1) * Number(m[2]) * mult;
  }
  var offMin = 0;
  if (tz !== undefined && tz !== null) {
    var t = String(tz).match(/^([+-])(\d\d):(\d\d)$/);
    if (!t) throw new Error('nowISO: tz вида "+05:00"');
    offMin = (t[1] === '-' ? -1 : 1) * (Number(t[2]) * 60 + Number(t[3]));
  }
  var d = new Date(ms + offMin * 6e4);
  function p(n) { return (n < 10 ? '0' : '') + n; }
  var iso = d.getUTCFullYear() + '-' + p(d.getUTCMonth() + 1) + '-' + p(d.getUTCDate()) +
    'T' + p(d.getUTCHours()) + ':' + p(d.getUTCMinutes()) + ':' + p(d.getUTCSeconds());
  return iso + (tz ? tz : 'Z');
}
```

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): генерация данных (uuid/randomInt/randomFrom/nowISO)"
```

---

### Task 8: Коллекции (lodash-lite)

**Files:**
- Modify: `src-tauri/js/stdlib.js`
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Produces (JS): `groupBy(arr, keyOrFn) -> object`, `sortBy(arr, keyOrFn) -> array` (копия), `uniqBy(arr, keyOrFn) -> array`, `chunk(arr, n) -> array[]`, `sample(arr, n) -> array` (n случайных без повторов).

- [ ] **Step 1: Падающие тесты:**

```rust
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
```

- [ ] **Step 2: Убедиться, что падают.**

- [ ] **Step 3: Реализация** — дописать в `stdlib.js`:

```js
// ── Коллекции ──
function __keyFn(key) {
  return typeof key === 'function' ? key : function (x) { return x == null ? undefined : x[key]; };
}
function groupBy(arr, key) {
  var f = __keyFn(key), r = {};
  for (var i = 0; i < arr.length; i++) {
    var g = String(f(arr[i]));
    (r[g] = r[g] || []).push(arr[i]);
  }
  return r;
}
// Возвращает отсортированную копию (исходный массив не трогает).
function sortBy(arr, key) {
  var f = __keyFn(key);
  return arr.slice().sort(function (a, b) {
    var x = f(a), y = f(b);
    return x < y ? -1 : x > y ? 1 : 0;
  });
}
function uniqBy(arr, key) {
  var f = __keyFn(key), seen = {}, r = [];
  for (var i = 0; i < arr.length; i++) {
    var k = String(f(arr[i]));
    if (!seen[k]) { seen[k] = 1; r.push(arr[i]); }
  }
  return r;
}
function chunk(arr, n) {
  var r = [];
  for (var i = 0; i < arr.length; i += n) r.push(arr.slice(i, i + n));
  return r;
}
// n случайных элементов без повторов (Фишер–Йетс по копии).
function sample(arr, n) {
  var a = arr.slice();
  for (var i = a.length - 1; i > 0; i--) {
    var j = Math.floor(Math.random() * (i + 1));
    var t = a[i]; a[i] = a[j]; a[j] = t;
  }
  return a.slice(0, Math.min(n === undefined ? 1 : n, a.length));
}
```

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/js/stdlib.js src-tauri/src/scripting.rs
git commit -m "feat(scripting): коллекции (groupBy/sortBy/uniqBy/chunk/sample)"
```

---

### Task 9: Трасса выполнения правил

**Files:**
- Modify: `src-tauri/src/scripting.rs` (ScriptResult, build_source, build_handler_source)
- Modify: `src-tauri/src/model.rs:52-83` (Flow)
- Modify: `src-tauri/src/proxy.rs` (3 места: request-фаза ~905-980, response-фаза ~1149-1294, handler ~784-815)
- Modify: `src-tauri/src/mcp/core_tools.rs:332-346` (flow_to_json)
- Modify: `src/types.ts:42` (тип Flow), `src/components/FlowDetail.tsx` (overview-таб, ~строка 229)
- Test: `src-tauri/src/scripting.rs`, `src-tauri/src/proxy.rs` (mod tests)

**Interfaces:**
- Produces:
  - `ScriptResult.trace: Vec<serde_json::Value>` — элементы `{op, path, nodes}` (stdlib) и `{op:"send", status, ms}` (handler).
  - `Flow.rule_trace: Vec<serde_json::Value>` (serde camelCase → `ruleTrace`), каждый элемент дополнен `"rule": <имя правила>`.
  - TS: `ruleTrace: { rule: string; op: string; path?: string; nodes?: number; status?: number; ms?: number }[]` в `Flow` (types.ts).

- [ ] **Step 1: Падающие тесты** в scripting.rs:

```rust
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
```

- [ ] **Step 2: Убедиться, что падают** (нет поля `trace`) — не компилируется.

- [ ] **Step 3: ScriptResult + build_source.** В `ScriptResult` после `notifications`:

```rust
    /// Трасса операций stdlib/send за прогон правила.
    #[serde(default)]
    pub trace: Vec<serde_json::Value>,
```

(в `ScriptResult::error(...)` добавить `trace: Vec::new()`).

В `build_source`: после `ctx.__notifications = [];` добавить `ctx.__trace = [];`; в возвращаемый JSON добавить `trace: ctx.__trace`; в catch-ветке — `trace: (typeof ctx !== "undefined" && ctx.__trace) || []`.

В `build_handler_source`: после `ctx.__notifications = [];` добавить `ctx.__trace = [];`; заменить строку `function send(req) {{ … }}` на версию с таймингом:

```
    function send(req) {{ var __t0 = Date.now(); var __r = JSON.parse(__native_send(JSON.stringify(req || request))); ctx.__trace.push({{ op: "send", status: __r.status, ms: Date.now() - __t0 }}); return __r; }}
```

и добавить `trace: ctx.__trace` в оба JSON-возврата (respond и error-без-return), `trace: (typeof ctx !== "undefined" && ctx.__trace) || []` — в catch.

- [ ] **Step 4: Прогнать scripting-тесты** — `cargo test scripting` → PASS.

- [ ] **Step 5: Flow + прокси.** В `model.rs` Flow после `applied_rules`:

```rust
    /// Трасса операций правил: {rule, op, path?, nodes?, status?, ms?}.
    #[serde(default)]
    pub rule_trace: Vec<serde_json::Value>,
```

(и `rule_trace: Vec::new()` в конструкторе `new_request`, рядом с `applied_rules: Vec::new()`).

В `proxy.rs`, вспомогательная функция (рядом с `emit_notifications`):

```rust
    /// Дополняет каждый элемент трассы именем правила.
    fn tag_trace(res: &crate::scripting::ScriptResult, rule_name: &str) -> Vec<serde_json::Value> {
        res.trace
            .iter()
            .map(|t| {
                let mut t = t.clone();
                if let Some(o) = t.as_object_mut() {
                    o.insert("rule".into(), serde_json::Value::String(rule_name.to_string()));
                }
                t
            })
            .collect()
    }
```

Три точки подключения (искать по уже известным строкам):
1. **Request-фаза** (~905): рядом с `let mut applied: Vec<String>` завести `let mut rule_trace: Vec<serde_json::Value> = Vec::new();`; после каждого `self.scripts.run(...)` добавить `rule_trace.extend(Self::tag_trace(&res, &rule.name));`; в месте `flow.applied_rules = applied;` (~980) добавить `flow.rule_trace = rule_trace;`.
2. **Response-фаза** (~1149-1294): аналогично — накапливать и в `store.update(id, |f| { … f.applied_rules.push(…) })` (~1293) добавить `f.rule_trace.extend(tagged.clone());` (где `tagged` — результат `tag_trace` для этого правила).
3. **Handler** (~812, `flow.applied_rules = vec![hrule.name.clone()];`): рядом `flow.rule_trace = Self::tag_trace(&res, &hrule.name);`.

- [ ] **Step 6: Тест прокси-уровня** — в `proxy.rs` mod tests найти существующий тест, проверяющий `flow.applied_rules.contains(&"add-debug".to_string())` (~1777), и в него добавить утверждение:

```rust
        assert!(
            flow.rule_trace.iter().all(|t| t["rule"].is_string()),
            "trace элементы тегированы именем правила: {:?}", flow.rule_trace
        );
```

(Если правило в этом тесте не вызывает stdlib-операций, trace будет пуст — утверждение `all` на пустом векторе истинно; главное, что поле есть и сериализуется.)

- [ ] **Step 7: MCP + frontend.** В `flow_to_json` (core_tools.rs:333) после `"appliedRules": flow.applied_rules,`:

```rust
        "ruleTrace": flow.rule_trace,
```

В `src/types.ts` после `appliedRules: string[];`:

```ts
  ruleTrace: { rule: string; op: string; path?: string; nodes?: number; status?: number; ms?: number }[];
```

Прогнать `pnpm vitest run` — тесты, создающие Flow-объекты литералами (`store.test.ts:16`, `filter.test.ts:18`, `breakpoints.test.ts:50`), дополнить `ruleTrace: [],` рядом с `appliedRules: []`.

В `FlowDetail.tsx`, в overview-`<dl>` (после строк Duration, ~строка 229) добавить:

```tsx
            {flow.ruleTrace?.length > 0 && (
              <>
                <dt className="text-muted-foreground">Rule trace</dt>
                <dd className="font-mono">
                  {flow.ruleTrace.map((t, i) => (
                    <div key={i}>
                      {t.rule}: {t.op}
                      {t.path ? `('${t.path}')` : ""}
                      {t.nodes !== undefined ? ` → ${t.nodes} узлов` : ""}
                      {t.status !== undefined ? ` → ${t.status} (${t.ms} ms)` : ""}
                    </div>
                  ))}
                </dd>
              </>
            )}
```

- [ ] **Step 8: Прогнать всё**

Run: `cd src-tauri && cargo test && cd .. && pnpm vitest run`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/scripting.rs src-tauri/src/model.rs src-tauri/src/proxy.rs src-tauri/src/mcp/core_tools.rs src/types.ts src/components/FlowDetail.tsx src/store.test.ts src/filter.test.ts src/breakpoints.test.ts
git commit -m "feat(scripting): трасса выполнения правил (ruleTrace) в flow, UI и MCP"
```

---

### Task 10: Диагностические ошибки (строка + имя правила)

**Files:**
- Modify: `src-tauri/src/scripting.rs` (build_source, build_handler_source)
- Modify: `src-tauri/src/proxy.rs` (места, где ошибка скрипта становится ошибкой flow)
- Test: `src-tauri/src/scripting.rs` (mod tests)

**Interfaces:**
- Produces: сообщения ошибок вида `rule "<имя>": <текст> (строка N)`, где N — строка в тексте скрипта правила (1-based). Строку добавляет движок, имя правила — прокси.

- [ ] **Step 1: Падающий тест:**

```rust
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
```

- [ ] **Step 2: Убедиться, что падает.**

- [ ] **Step 3: Реализация в build_source.** Разбить формирование исходника на префикс/суффикс, чтобы посчитать смещение строк (скрипт должен начинаться с новой строки):

```rust
fn build_source(prelude: &str, script: &str) -> String {
    let full_prelude = format!("{STD_LIB}\n{prelude}");
    let prefix = format!(
        r#"(function() {{
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
    if (e && e.lineNumber && (e.lineNumber - {offset}) > 0) {{ __m += " (строка " + (e.lineNumber - {offset}) + ")"; }}
    return JSON.stringify({{ action: "error", error: __m, trace: (typeof ctx !== "undefined" && ctx.__trace) || [] }});
  }}
}})()
"#
    )
}
```

Ту же схему применить в `build_handler_source` (скрипт вынести на отдельную строку внутри `const __out = (function() {`…`})();`, посчитать префикс, добавить строку в catch). Если QuickJS в сборке не выставляет `e.lineNumber`, условие ложно и сообщение остаётся без строки — тест Step 1 это выявит: тогда заменить источник на разбор `e.stack` (первая строка вида `at <eval> (eval_script:N)`): `var __ln = e && e.lineNumber; if (!__ln && e && e.stack) { var __sm = String(e.stack).match(/:(\d+)/); if (__sm) __ln = Number(__sm[1]); }`.

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS (существующий `thrown_error_becomes_error_result` тоже должен остаться зелёным).

- [ ] **Step 5: Имя правила в прокси.** В `proxy.rs` — во всех местах, где текст ошибки скрипта пишется в flow или в abort-ответ с известным именем правила, обернуть. Найти командой `grep -n "res.error" src-tauri/src/proxy.rs`. Известные места:
  - handler (~872): `flow.error = Some(res.error.unwrap_or_else(|| "handler error".into()));` →

```rust
            flow.error = Some(format!(
                "rule \"{}\": {}",
                hrule.name,
                res.error.unwrap_or_else(|| "handler error".into())
            ));
```

  - request-фаза и response-фаза: в match-ветках `"error"` для `res.action` (после `self.scripts.run(...)`) — там, где ошибка транслируется во flow/abort, обернуть тем же `format!("rule \"{}\": {}", rule.name, …)`. Если в ветке ошибка только эмитится как rule-error событие и flow не помечается — не менять (fail-closed уже обеспечен другим местом).

- [ ] **Step 6: Прогнать всё** — `cd src-tauri && cargo test` → PASS (существующие проксти-тесты с текстами ошибок могут потребовать правки ожиданий — обновить их на новый формат `rule "…": …`).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/scripting.rs src-tauri/src/proxy.rs
git commit -m "feat(scripting): диагностические ошибки — имя правила и строка скрипта"
```

---

### Task 11: Валидация при сохранении правила

**Files:**
- Modify: `src-tauri/src/scripting.rs` (validate_rule_script + extract_path_literals)
- Modify: `src-tauri/src/commands.rs:219-223` (save_rule), новая команда validate_jsonpath
- Modify: `src-tauri/src/lib.rs:47-55` (регистрация команды)
- Modify: `src-tauri/src/mcp/core_tools.rs:415-427` (tool_save_rule)
- Test: `src-tauri/src/scripting.rs`

**Interfaces:**
- Produces:
  - `pub fn validate_rule_script(script: &str) -> Result<(), String>` (scripting.rs) — синтаксис JS + все литеральные JSONPath-аргументы.
  - `pub fn extract_path_literals(script: &str) -> Vec<String>` (scripting.rs).
  - Tauri-команда `validate_jsonpath(path: String) -> Option<String>` (None = валиден) — используется Task 15.
- Consumes: `crate::jsonpath::validate` (Task 2).

- [ ] **Step 1: Падающие тесты:**

```rust
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
```

- [ ] **Step 2: Убедиться, что не компилируется.**

- [ ] **Step 3: Реализация** в scripting.rs (вне mod tests):

```rust
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
```

- [ ] **Step 4: Прогнать** — `cargo test scripting` → PASS.

- [ ] **Step 5: Подключить к сохранению.** `commands.rs` save_rule — первой строкой тела:

```rust
    crate::scripting::validate_rule_script(&rule.script).map_err(|e| format!("правило не сохранено: {e}"))?;
```

Новая команда там же:

```rust
/// None — путь валиден; Some(текст ошибки) — нет. Для live-валидации в редакторе.
#[tauri::command]
pub fn validate_jsonpath(path: String) -> Option<String> {
    crate::jsonpath::validate(&path)
}
```

В `lib.rs` в `generate_handler![…]` добавить `commands::validate_jsonpath,`.

В `mcp/core_tools.rs` tool_save_rule — после десериализации `rule`:

```rust
    crate::scripting::validate_rule_script(&rule.script)
        .map_err(|e| format!("правило не сохранено: {e}"))?;
```

- [ ] **Step 6: Прогнать всё** — `cd src-tauri && cargo test` → PASS (тесты rules/commands, сохраняющие валидные скрипты, должны остаться зелёными; если какой-то тест сохраняет заведомо битый скрипт — обновить его на ожидание ошибки).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/scripting.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/mcp/core_tools.rs
git commit -m "feat(rules): валидация JS и JSONPath при сохранении правила (UI и MCP)"
```

---

### Task 12: Dry-run бэкенд (test_rule, test_path)

**Files:**
- Modify: `src-tauri/src/scripting.rs` (рефакторинг execute_handler → send_impl; execute_once)
- Create: `src-tauri/src/dryrun.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod dryrun;`, регистрация команд)
- Modify: `src-tauri/src/commands.rs` (команды test_rule, test_path)
- Modify: `src-tauri/src/mcp/core_tools.rs` (MCP-инструмент test_rule)
- Test: `src-tauri/src/scripting.rs`, `src-tauri/src/dryrun.rs`

**Interfaces:**
- Consumes: `crate::projects::merged_env_object` (уже pub, используется прокси), `crate::rules::glob_match_env`, `AppState.store/global_env/active_project` (commands.rs:16).
- Produces:
  - `pub type SendFn = Arc<dyn Fn(&str) -> String + Send + Sync>` (scripting.rs).
  - `pub fn execute_handler_with_send(prelude, script, input_json, js_timeout, secrets, send_impl: SendFn) -> ScriptResult`; существующий `execute_handler` делегирует с `native_send`.
  - `pub fn execute_once(prelude, script, input_json, js_timeout, secrets) -> ScriptResult` — одноразовый прогон request/response-фазы (fresh runtime, без сети).
  - `dryrun::run(flow: &Flow, script: &str, phase: &str, prelude: &str, env: serde_json::Value, timeout) -> serde_json::Value` — `{action, error?, trace, before: {status?, body}, after: {status?, headers?, body}?}`.
  - Tauri: `test_rule(script, phase, pattern, flow_id: Option<u64>) -> Value`, `test_path(path, pattern) -> Option<Value>` (`{flowId, nodes, error?}`).
  - MCP: инструмент `test_rule` `{script, phase, pattern, flowId?}`.

- [ ] **Step 1: Рефакторинг execute_handler.** В scripting.rs:

```rust
/// Реализация send() для handler-движка (подменяется в dry-run на реплей).
pub type SendFn = Arc<dyn Fn(&str) -> String + Send + Sync>;

pub fn execute_handler(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
) -> ScriptResult {
    execute_handler_with_send(prelude, script, input_json, js_timeout, secrets, Arc::new(|req: &str| native_send(req)))
}

pub fn execute_handler_with_send(
    prelude: &str,
    script: &str,
    input_json: &str,
    js_timeout: Duration,
    secrets: SecretFn,
    send_impl: SendFn,
) -> ScriptResult {
    // Тело прежнего execute_handler без изменений, кроме привязки send:
    // вместо
    //   Function::new(c.clone(), move |req: String| -> String { native_send(&req) })
    // написать
    //   let si = send_impl.clone();
    //   Function::new(c.clone(), move |req: String| -> String { si(&req) })
}
```

И одноразовый прогон request/response-фазы:

```rust
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
```

Тест реплея (в scripting.rs mod tests):

```rust
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
```

Run: `cargo test scripting` → PASS.

- [ ] **Step 2: Модуль dryrun.rs:**

```rust
//! Dry-run правила на захваченном flow: без сети (send реплеит захваченный
//! ответ), без сохранения. Используется Tauri-командой test_rule и MCP.

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

/// Прогоняет script (phase: request|response|handler) над захваченным flow.
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

    // after: что уйдёт клиенту / на сервер после правила.
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
```

Тест в dryrun.rs:

```rust
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
```

`todo!` в sample_flow заменить на реальный конструктор, скопированный из `mcp/core_tools.rs` tests (`fn sample_flow`, ~строка 590) — тело ответа задать `{"items":[{"x":1}]}` с `body_is_text: true` и `content-type: application/json`. В плане это допустимая отсылка, потому что код-образец существует в репо и тест не скомпилируется, пока не подставлен.

Run: `cargo test dryrun` → PASS.

- [ ] **Step 3: Tauri-команды** в commands.rs:

```rust
/// Dry-run правила на захваченном flow (или последнем, совпавшем с pattern).
#[tauri::command]
pub fn test_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    script: String,
    phase: String,
    pattern: String,
    flow_id: Option<u64>,
) -> Result<serde_json::Value, String> {
    let env = {
        let global = state.global_env.read().unwrap();
        let guard = state.active_project.read().unwrap();
        crate::projects::merged_env_object(&global, guard.as_ref())
    };
    let flow = match flow_id {
        Some(id) => state.store.get(id).ok_or_else(|| format!("flow {id} не найден в памяти"))?,
        None => state
            .store
            .all()
            .into_iter()
            .filter(|f| f.response.is_some())
            .filter(|f| {
                crate::rules::glob_match_env(&pattern, &format!("{}{}", f.url.host, f.url.path), &env)
            })
            .max_by_key(|f| f.timestamp)
            .ok_or_else(|| format!("нет захваченного flow под паттерн «{pattern}» — сделайте запрос через прокси"))?,
    };
    let prelude = crate::rules::load_library(&rules_dir(&app)?).unwrap_or_default();
    Ok(crate::dryrun::run(&flow, &script, &phase, &prelude, env, std::time::Duration::from_secs(10)))
}

/// Счётчик совпадений пути по последнему захваченному flow под pattern.
#[tauri::command]
pub fn test_path(
    state: State<'_, AppState>,
    path: String,
    pattern: String,
) -> Result<Option<serde_json::Value>, String> {
    if let Some(err) = crate::jsonpath::validate(&path) {
        return Err(err);
    }
    let env = {
        let global = state.global_env.read().unwrap();
        let guard = state.active_project.read().unwrap();
        crate::projects::merged_env_object(&global, guard.as_ref())
    };
    let flow = state
        .store
        .all()
        .into_iter()
        .filter(|f| f.response.as_ref().map(|r| r.body_is_text).unwrap_or(false))
        .filter(|f| crate::rules::glob_match_env(&pattern, &format!("{}{}", f.url.host, f.url.path), &env))
        .max_by_key(|f| f.timestamp);
    let Some(f) = flow else { return Ok(None) };
    let body = String::from_utf8_lossy(&f.response.as_ref().unwrap().body).to_string();
    let res: serde_json::Value =
        serde_json::from_str(&crate::jsonpath::locate(&body, &path)).unwrap_or_default();
    let nodes = res.get("locations").and_then(|l| l.as_array()).map(|a| a.len());
    Ok(Some(serde_json::json!({ "flowId": f.id, "nodes": nodes, "error": res.get("error") })))
}
```

Сигнатуру `merged_env_object` сверить с использованием в proxy.rs:282-284 и подстроить вызов (там `merged_env_object(&global, guard.as_ref())`). В `lib.rs`: `pub mod dryrun;` и в `generate_handler![…]` добавить `commands::test_rule, commands::test_path,`.

- [ ] **Step 4: MCP-инструмент.** В `mcp/core_tools.rs` — в список инструментов (рядом с `save_rule`):

```rust
        ToolDef {
            name: "test_rule",
            description: "Dry-run a rule script against a captured flow (no network, no save): send() replays the captured response. Args: script, phase (request|response|handler), pattern, flowId (optional). Returns action/error/trace/before/after.",
            // схему аргументов оформить по образцу соседних ToolDef
        },
```

в диспетчер (`match name { … }`): `"test_rule" => tool_test_rule(deps, args),` и реализация по образцу `tool_save_rule`:

```rust
fn tool_test_rule(deps: &Deps, args: &Value) -> Result<Value, String> {
    let script = str_arg(args, "script").ok_or("missing script")?;
    let phase = str_arg(args, "phase").unwrap_or_else(|| "request".into());
    let pattern = str_arg(args, "pattern").unwrap_or_default();
    let flow_id = u64_arg(args, "flowId");
    let env = {
        let global = deps.state.global_env.read().unwrap();
        let guard = deps.state.active_project.read().unwrap();
        crate::projects::merged_env_object(&global, guard.as_ref())
    };
    let flow = match flow_id {
        Some(id) => deps.state.store.get(id).ok_or_else(|| format!("flow {id} not found in memory"))?,
        None => deps
            .state
            .store
            .all()
            .into_iter()
            .filter(|f| f.response.is_some())
            .filter(|f| crate::rules::glob_match_env(&pattern, &format!("{}{}", f.url.host, f.url.path), &env))
            .max_by_key(|f| f.timestamp)
            .ok_or_else(|| format!("no captured flow matching pattern \"{pattern}\""))?,
    };
    let prelude = crate::rules::load_library(&deps.rules_dir).unwrap_or_default();
    Ok(crate::dryrun::run(&flow, &script, &phase, &prelude, env, std::time::Duration::from_secs(10)))
}
```

- [ ] **Step 5: Прогнать всё** — `cd src-tauri && cargo test` → PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/scripting.rs src-tauri/src/dryrun.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/mcp/core_tools.rs
git commit -m "feat(rules): dry-run правил на захваченном трафике (test_rule/test_path, UI+MCP)"
```

---

### Task 13: Манифест документации + Function library UI

**Files:**
- Create: `src/scripting/stdlib-docs.ts`
- Modify: `src/scripting/stdlib.ts` (STD_DTS дополнить; STD_FUNCTIONS удалить)
- Modify: `src/components/RulesView.tsx:12,313-326` (библиотека функций)
- Test: `src/scripting/stdlib-docs.test.ts`

**Interfaces:**
- Produces:
  - `export interface StdFnDoc { name: string; category: string; signature: string; doc: string; example: string; phase?: "handler" }`
  - `export const STD_FN_DOCS: StdFnDoc[]` — все публичные функции stdlib (существующие + новые).
  - `export const DOC_CATEGORIES: string[]` — порядок категорий: `"Тело (JSONPath)", "Заголовки", "URL и query", "Моки и ответы", "Данные", "Коллекции", "Сеть (handler)", "Прочее"`.
  - `export const JSONPATH_CHEATSHEET: { syntax: string; doc: string }[]`.

- [ ] **Step 1: Создать `src/scripting/stdlib-docs.ts`.** Каждая запись — имя, категория, сигнатура, док (RU), пример-однострочник. Обязательный состав (name → category):
  - Тело (JSONPath): `patch`, `tryPatch`, `pick`, `pickOne`, `removeAt`, `mergeAt`, `jsonBody`, `setJsonBody`
  - Заголовки: `header`, `hasHeader`, `setHeader`, `removeHeader`, `bearer`
  - URL и query: `queryParam`, `setQueryParam`, `removeQueryParam`, `rewriteHost`, `rewritePath`, `pathSegments`
  - Моки и ответы: `json`, `textResponse`, `httpError`, `delay` (phase: handler)
  - Данные: `uuid`, `randomInt`, `randomFrom`, `nowISO`
  - Коллекции: `groupBy`, `sortBy`, `uniqBy`, `chunk`, `sample`
  - Сеть (handler): `sendJsonRequest`, `sendWithRetry` (обе phase: handler)
  - Прочее: `secret`, `notify`

Образец записей (остальные по аналогии, с реальными сигнатурами из задач 3-8):

```ts
export interface StdFnDoc {
  name: string;
  category: string;
  signature: string;
  doc: string;
  example: string;
  phase?: "handler";
}

export const DOC_CATEGORIES = [
  "Тело (JSONPath)", "Заголовки", "URL и query", "Моки и ответы",
  "Данные", "Коллекции", "Сеть (handler)", "Прочее",
] as const;

export const STD_FN_DOCS: StdFnDoc[] = [
  {
    name: "patch",
    category: "Тело (JSONPath)",
    signature: "patch(msg, path, valueOrFn): number",
    doc: "Записывает значение (или применяет функцию) во всех узлах, совпавших с JSONPath. 0 узлов — ошибка (используйте tryPatch, если поле опционально). Тело парсится и сериализуется автоматически.",
    example: "patch(res, 'items[*].advertData.addDateFormatted', nowISO())",
  },
  {
    name: "delay",
    category: "Моки и ответы",
    signature: "delay(ms): void",
    doc: "Блокирующая пауза для эмуляции медленной сети. Только handler-фаза.",
    example: "delay(1500); return send(request);",
    phase: "handler",
  },
  // …остальные записи по той же схеме
];

export const JSONPATH_CHEATSHEET = [
  { syntax: "$", doc: "корень документа (можно опускать: 'items' == '$.items')" },
  { syntax: "items[*]", doc: "все элементы массива" },
  { syntax: "items[0] / items[-1]", doc: "по индексу / с конца" },
  { syntax: "items[0:3]", doc: "срез [от:до)" },
  { syntax: "$..price", doc: "поле на любой глубине" },
  { syntax: "items[?@.type=='advert']", doc: "фильтр по условию" },
  { syntax: "items[?@.price>1000 && @.isVip]", doc: "логические условия" },
  { syntax: "items[?length(@.tags)>2]", doc: "функции: length(), count(), match(), search(), value()" },
  { syntax: "$['ключ с пробелом']", doc: "имена в скобках/кавычках" },
];
```

- [ ] **Step 2: Тест синхронности** `src/scripting/stdlib-docs.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { STD_FN_DOCS, DOC_CATEGORIES } from "./stdlib-docs";
import { STD_DTS } from "./stdlib";

const stdlibJs = readFileSync(resolve(__dirname, "../../src-tauri/js/stdlib.js"), "utf8");
const implemented = new Set(
  [...stdlibJs.matchAll(/^function ([a-zA-Z]\w*)\(/gm)].map((m) => m[1]).filter((n) => !n.startsWith("__")),
);
// send/sleep объявляются обёрткой handler-фазы в scripting.rs, не в stdlib.js.
const externals = new Set(["send", "sleep"]);

describe("stdlib docs sync", () => {
  it("каждая функция stdlib.js задокументирована в манифесте", () => {
    const documented = new Set(STD_FN_DOCS.map((f) => f.name));
    for (const name of implemented) expect(documented, `нет доки для ${name}`).toContain(name);
  });
  it("каждая запись манифеста реализована и объявлена в STD_DTS", () => {
    for (const f of STD_FN_DOCS) {
      if (externals.has(f.name)) continue;
      expect(implemented, `${f.name} есть в манифесте, но не в stdlib.js`).toContain(f.name);
      expect(STD_DTS, `${f.name} не объявлен в STD_DTS`).toContain(`function ${f.name}(`);
    }
  });
  it("категории записей — из фиксированного списка", () => {
    for (const f of STD_FN_DOCS) expect(DOC_CATEGORIES).toContain(f.category);
  });
});
```

Run: `pnpm vitest run stdlib-docs` — FAIL (STD_DTS ещё без новых функций).

- [ ] **Step 3: Дополнить STD_DTS** в `src/scripting/stdlib.ts` декларациями всех новых функций (по сигнатурам из задач 3-8, с jsdoc-описаниями из манифеста), например:

```ts
/** Записать значение/функцию во все узлы JSONPath. 0 узлов → исключение. */
declare function patch(target: TrawlMessage | object, path: string, valueOrFn: any): number;
declare function tryPatch(target: TrawlMessage | object, path: string, valueOrFn: any): number;
declare function pick(target: TrawlMessage | object, path: string): any[];
declare function pickOne(target: TrawlMessage | object, path: string): any | null;
declare function removeAt(target: TrawlMessage | object, path: string): number;
declare function mergeAt(target: TrawlMessage | object, path: string, obj: object): number;
declare function setQueryParam(req: TrawlRequest, name: string, value: string | number): void;
declare function removeQueryParam(req: TrawlRequest, name: string): void;
declare function rewriteHost(req: TrawlRequest, host: string): void;
declare function rewritePath(req: TrawlRequest, from: string | RegExp, to: string): void;
declare function pathSegments(req: TrawlRequest): string[];
declare function json(obj: any): TrawlMock;
declare function json(status: number, obj: any): TrawlMock;
declare function textResponse(status: number, body: string, contentType?: string): TrawlMock;
declare function httpError(status: number, msg?: string): TrawlMock;
/** Только handler-фаза. */
declare function delay(ms: number): void;
declare function uuid(): string;
declare function randomInt(a: number, b: number): number;
declare function randomFrom<T>(arr: T[]): T;
declare function nowISO(shift?: string | null, tz?: string | null): string;
declare function groupBy<T>(arr: T[], key: string | ((x: T) => unknown)): Record<string, T[]>;
declare function sortBy<T>(arr: T[], key: string | ((x: T) => unknown)): T[];
declare function uniqBy<T>(arr: T[], key: string | ((x: T) => unknown)): T[];
declare function chunk<T>(arr: T[], n: number): T[][];
declare function sample<T>(arr: T[], n?: number): T[];
```

Удалить `export const STD_FUNCTIONS: StdFn[]` и `export interface StdFn` из stdlib.ts.

- [ ] **Step 4: RulesView.** Заменить импорт `STD_FUNCTIONS` на `STD_FN_DOCS, DOC_CATEGORIES, JSONPATH_CHEATSHEET` из `../scripting/stdlib-docs`; блок списка функций (строки ~311-326) заменить на группированный по категориям с кнопкой вставки примера (через существующий `apiRef` ScriptEditor, как это делают сниппеты — см. `insert` в `ScriptEditorApi`):

```tsx
          {DOC_CATEGORIES.map((cat) => {
            const fns = STD_FN_DOCS.filter((f) => f.category === cat);
            if (fns.length === 0) return null;
            return (
              <div key={cat} className="mb-3">
                <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{cat}</div>
                <ul className="flex flex-col gap-2">
                  {fns.map((fn) => (
                    <li key={fn.name} className="rounded border border-border/60 bg-card px-2 py-1.5">
                      <div className="flex items-center gap-2">
                        <code className="font-mono text-[11px] text-primary break-all">{fn.signature}</code>
                        {fn.phase === "handler" && (
                          <span className="shrink-0 rounded bg-secondary px-1 text-[9px] uppercase text-muted-foreground">handler</span>
                        )}
                        <button
                          className="ml-auto shrink-0 text-[10px] text-muted-foreground hover:text-foreground"
                          title="Вставить пример в скрипт"
                          onClick={() => editorApi.current?.insert(fn.example + "\n")}
                        >
                          вставить
                        </button>
                      </div>
                      <div className="mt-0.5 text-[11px] leading-snug text-muted-foreground">{fn.doc}</div>
                      <code className="mt-0.5 block font-mono text-[10px] text-muted-foreground/80 break-all">{fn.example}</code>
                    </li>
                  ))}
                </ul>
              </div>
            );
          })}
          <div className="mb-1 mt-4 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">JSONPath — шпаргалка</div>
          <ul className="flex flex-col gap-1">
            {JSONPATH_CHEATSHEET.map((r) => (
              <li key={r.syntax} className="flex gap-2 text-[11px]">
                <code className="shrink-0 font-mono text-primary">{r.syntax}</code>
                <span className="text-muted-foreground">{r.doc}</span>
              </li>
            ))}
          </ul>
```

`editorApi` — ref на `ScriptEditorApi`; если в этом месте RulesView его нет, найти существующий ref, передаваемый в `<ScriptEditor apiRef={…}>` (сниппеты уже вставляют текст), и использовать его.

- [ ] **Step 5: Прогнать** — `pnpm vitest run` → PASS (включая sync-тест); `pnpm tsc --noEmit` если есть type-check скрипт — чисто.

- [ ] **Step 6: Commit**

```bash
git add src/scripting/stdlib-docs.ts src/scripting/stdlib-docs.test.ts src/scripting/stdlib.ts src/components/RulesView.tsx
git commit -m "feat(ui): манифест доков stdlib + Function library по категориям + шпаргалка JSONPath"
```

---

### Task 14: pathContext.ts — детект пути под курсором и литералов

**Files:**
- Create: `src/scripting/pathContext.ts`
- Test: `src/scripting/pathContext.test.ts`

**Interfaces:**
- Produces:
  - `export const PATH_FNS = ["patch","tryPatch","pick","pickOne","removeAt","mergeAt"]`
  - `export function pathArgContext(line: string, column: number): { fn: string; prefix: string } | null` — column 1-based (Monaco).
  - `export interface PathLiteral { path: string; line: number; startColumn: number; endColumn: number }`
  - `export function extractPathLiterals(script: string): PathLiteral[]` — line/startColumn 1-based; endColumn указывает на закрывающую кавычку.

- [ ] **Step 1: Падающие тесты** `src/scripting/pathContext.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { extractPathLiterals, pathArgContext } from "./pathContext";

describe("pathArgContext", () => {
  it("курсор внутри строки-пути → fn и prefix", () => {
    const line = "patch(res, 'items[*].adv";
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "patch", prefix: "items[*].adv" });
  });
  it("двойные кавычки и пустой префикс", () => {
    const line = 'pick(res, "';
    expect(pathArgContext(line, line.length + 1)).toEqual({ fn: "pick", prefix: "" });
  });
  it("вне строки-пути → null", () => {
    expect(pathArgContext("patch(res, ", 12)).toBeNull();
    expect(pathArgContext("someFn(res, 'a.b", 17)).toBeNull();
    expect(pathArgContext("patch(res, 'a.b', ", 19)).toBeNull(); // строка закрыта
  });
});

describe("extractPathLiterals", () => {
  it("находит все литералы с координатами", () => {
    const script = "const r = send(request);\npatch(r, 'items[*].x', 1);\npick(r, \"a\");";
    const lits = extractPathLiterals(script);
    expect(lits).toHaveLength(2);
    expect(lits[0]).toMatchObject({ path: "items[*].x", line: 2 });
    // столбцы: путь начинается сразу после кавычки
    expect(script.split("\n")[1].slice(lits[0].startColumn - 1, lits[0].endColumn - 1)).toBe("items[*].x");
    expect(lits[1]).toMatchObject({ path: "a", line: 3 });
  });
  it("динамический путь (переменная) пропускается", () => {
    expect(extractPathLiterals("patch(r, dyn, 1)")).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Убедиться, что падают** — `pnpm vitest run pathContext` → FAIL (модуля нет).

- [ ] **Step 3: Реализация** `src/scripting/pathContext.ts`:

```ts
/** Функции stdlib, у которых 2-й аргумент — JSONPath-литерал.
 *  Синхронизировано с extract_path_literals в src-tauri/src/scripting.rs. */
export const PATH_FNS = ["patch", "tryPatch", "pick", "pickOne", "removeAt", "mergeAt"] as const;

const OPEN_RE =
  /\b(patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(['"])((?:\\.|(?!\2).)*)$/;

/** Курсор (column, 1-based) внутри незакрытого строкового литерала-пути? */
export function pathArgContext(line: string, column: number): { fn: string; prefix: string } | null {
  const before = line.slice(0, column - 1);
  const m = before.match(OPEN_RE);
  return m ? { fn: m[1], prefix: m[3] } : null;
}

export interface PathLiteral {
  path: string;
  line: number;
  startColumn: number;
  endColumn: number;
}

const LITERAL_RE =
  /\b(?:patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)")/g;

/** Все литеральные пути в скрипте с координатами (1-based, для Monaco). */
export function extractPathLiterals(script: string): PathLiteral[] {
  const out: PathLiteral[] = [];
  script.split("\n").forEach((lineText, i) => {
    LITERAL_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = LITERAL_RE.exec(lineText))) {
      const raw = m[1] ?? m[2];
      const start = m.index + m[0].length - raw.length - 1; // 0-based индекс первого символа пути
      out.push({ path: raw, line: i + 1, startColumn: start + 1, endColumn: start + 1 + raw.length });
    }
  });
  return out;
}
```

- [ ] **Step 4: Прогнать** — `pnpm vitest run pathContext` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src/scripting/pathContext.ts src/scripting/pathContext.test.ts
git commit -m "feat(ui): детект JSONPath-литералов и контекста курсора (pathContext)"
```

---

### Task 15: Подсказки в Monaco (автокомплит + маркеры + inlay)

**Files:**
- Create: `src/scripting/pathHints.ts`
- Modify: `src/monaco-setup.ts` (регистрация провайдеров)
- Modify: `src/components/ScriptEditor.tsx` (подключение диагностики в handleMount)
- Modify: `src/components/RulesView.tsx:184-187` (передача fields+pattern)
- Test: `src/scripting/pathHints.test.ts` (чистая логика segmentCandidates)

**Interfaces:**
- Consumes: `pathArgContext`, `extractPathLiterals` (Task 14), `FieldInfo` (`src/lib/analyze.ts:1`), Tauri-команды `validate_jsonpath`, `test_path` (Tasks 11-12).
- Produces:
  - `export function setPathHintContext(fields: FieldInfo[], pattern: string): void`
  - `export function segmentCandidates(prefix: string, fields: FieldInfo[]): { label: string; kind: "field" | "array"; type?: string }[]` — prefix уже обрезан до последнего `.`/`[`.
  - `export function registerPathHints(m: typeof monaco): void` — completion + inlay providers (вызывается один раз из monaco-setup).
  - `export function attachPathDiagnostics(editor: monaco.editor.IStandaloneCodeEditor): void` — debounce-валидация литералов через `validate_jsonpath`, маркеры `trawl-jsonpath`.

- [ ] **Step 1: Падающие тесты** `src/scripting/pathHints.test.ts` (только чистая функция):

```ts
import { describe, expect, it } from "vitest";
import { segmentCandidates } from "./pathHints";
import type { FieldInfo } from "@/lib/analyze";

const f = (path: string, type = "string"): FieldInfo => ({ path, type, varying: false });
const FIELDS = [
  f("status"),
  f("items", "array"),
  f("items[].type"),
  f("items[].advertData.id", "number"),
  f("items[].advertData.title"),
];

describe("segmentCandidates", () => {
  it("пустой префикс → ключи верхнего уровня", () => {
    const labels = segmentCandidates("", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["items", "status"]);
    expect(segmentCandidates("", FIELDS).find((c) => c.label === "items")?.kind).toBe("array");
  });
  it("items[*]. → поля элемента", () => {
    const labels = segmentCandidates("items[*].", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["advertData", "type"]);
  });
  it("селектор-фильтр эквивалентен [*]", () => {
    const labels = segmentCandidates("items[?@.type=='a'].", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["advertData", "type"]);
  });
  it("глубокий префикс", () => {
    const labels = segmentCandidates("items[*].advertData.", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["id", "title"]);
  });
  it("$ в начале игнорируется", () => {
    expect(segmentCandidates("$.items[*].", FIELDS).map((c) => c.label).sort()).toEqual(["advertData", "type"]);
  });
});
```

- [ ] **Step 2: Убедиться, что падают.**

- [ ] **Step 3: Реализация** `src/scripting/pathHints.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { FieldInfo } from "@/lib/analyze";
import type * as monacoNs from "monaco-editor";
import { extractPathLiterals, pathArgContext } from "./pathContext";

let hintFields: FieldInfo[] = [];
let hintPattern = "";

/** Контекст подсказок: структура прошлых ответов + паттерн текущего правила. */
export function setPathHintContext(fields: FieldInfo[], pattern: string) {
  hintFields = fields;
  hintPattern = pattern;
}

/** Кандидаты следующего сегмента. prefix — до последнего './[' (частичное слово отрезано). */
export function segmentCandidates(
  prefix: string,
  fields: FieldInfo[],
): { label: string; kind: "field" | "array"; type?: string }[] {
  // Приводим JSONPath-префикс к форме путей FieldInfo: "$.items[?…]." → "items[]"
  const norm = prefix
    .replace(/^\$\.?/, "")
    .replace(/\[[^\]]*\]/g, "[]")
    .replace(/\.+$/, "");
  const base = norm === "" ? "" : norm + ".";
  const seen = new Map<string, { kind: "field" | "array"; type?: string }>();
  for (const fi of fields) {
    if (base && !fi.path.startsWith(base)) continue;
    const rest = base ? fi.path.slice(base.length) : fi.path;
    const seg = rest.split(".")[0];
    if (!seg) continue;
    const isArr = seg.endsWith("[]");
    const name = isArr ? seg.slice(0, -2) : seg;
    if (!name) continue;
    const prev = seen.get(name);
    if (!prev || (isArr && prev.kind === "field")) {
      seen.set(name, { kind: isArr ? "array" : "field", type: rest === seg && !isArr ? fi.type : prev?.type });
    }
  }
  return [...seen.entries()].map(([label, v]) => ({ label, ...v }));
}

/** Однократная регистрация completion/inlay-провайдеров для javascript. */
export function registerPathHints(m: typeof monacoNs) {
  m.languages.registerCompletionItemProvider("javascript", {
    triggerCharacters: ["'", '"', ".", "["],
    provideCompletionItems(model, position) {
      const line = model.getLineContent(position.lineNumber);
      const ctx = pathArgContext(line, position.column);
      if (!ctx) return { suggestions: [] };
      const cut = ctx.prefix.replace(/[^.\[\]]*$/, "");
      const word = model.getWordUntilPosition(position);
      const range = new m.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
      return {
        suggestions: segmentCandidates(cut, hintFields).map((c) => ({
          label: c.type ? `${c.label}: ${c.type}` : c.label,
          filterText: c.label,
          sortText: c.label,
          kind: c.kind === "array" ? m.languages.CompletionItemKind.Struct : m.languages.CompletionItemKind.Field,
          insertText: c.kind === "array" ? `${c.label}[*]` : c.label,
          detail: c.kind === "array" ? "массив" : c.type,
          range,
        })),
      };
    },
  });

  // Inlay: " → N узлов" после каждого литерала-пути (по последнему совпавшему flow).
  const countCache = new Map<string, { at: number; text: string | null }>();
  m.languages.registerInlayHintsProvider("javascript", {
    async provideInlayHints(model, range) {
      const hints: monacoNs.languages.InlayHint[] = [];
      const lits = extractPathLiterals(model.getValue()).filter(
        (l) => l.line >= range.startLineNumber && l.line <= range.endLineNumber,
      );
      for (const lit of lits) {
        const key = `${hintPattern}\u0000${lit.path}`;
        const cached = countCache.get(key);
        let text: string | null;
        if (cached && Date.now() - cached.at < 3000) {
          text = cached.text;
        } else {
          text = await invoke<{ nodes: number | null } | null>("test_path", { path: lit.path, pattern: hintPattern })
            .then((r) => (r == null || r.nodes == null ? null : r.nodes === 0 ? " → 0 узлов (нет совпадений)" : ` → ${r.nodes} узлов`))
            .catch(() => null);
          countCache.set(key, { at: Date.now(), text });
        }
        if (text) {
          hints.push({
            position: { lineNumber: lit.line, column: lit.endColumn + 1 },
            label: text,
            paddingLeft: true,
          });
        }
      }
      return { hints, dispose() {} };
    },
  });
}

/** Debounce-валидация JSONPath-литералов; маркеры под невалидными путями. */
export function attachPathDiagnostics(editor: monacoNs.editor.IStandaloneCodeEditor) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const validateNow = async () => {
    const model = editor.getModel();
    if (!model) return;
    // Прямой импорт monaco-editor (не monaco-setup) — иначе циклический импорт:
    // monaco-setup сам импортирует pathHints ради registerPathHints.
    const monaco = await import("monaco-editor");
    const lits = extractPathLiterals(model.getValue());
    const markers: monacoNs.editor.IMarkerData[] = [];
    for (const lit of lits) {
      const err = await invoke<string | null>("validate_jsonpath", { path: lit.path }).catch(() => null);
      if (err) {
        markers.push({
          severity: monaco.MarkerSeverity.Error,
          message: `JSONPath: ${err}`,
          startLineNumber: lit.line,
          startColumn: lit.startColumn,
          endLineNumber: lit.line,
          endColumn: lit.endColumn,
        });
      }
    }
    monaco.editor.setModelMarkers(model, "trawl-jsonpath", markers);
  };
  editor.onDidChangeModelContent(() => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void validateNow(), 400);
  });
  void validateNow();
}
```

- [ ] **Step 4: Подключение.** В `monaco-setup.ts` после `setResponseDataType("{ [key: string]: any }");`:

```ts
import { registerPathHints } from "./scripting/pathHints";
registerPathHints(monaco);
```

(импорт — в шапку файла). В `ScriptEditor.tsx` в `handleMount` первой строкой (до `if (!apiRef) return;`):

```ts
    if (language === "javascript") attachPathDiagnostics(editor);
```

(импорт `attachPathDiagnostics` из `../scripting/pathHints`; `handleMount` сейчас начинается с `if (!apiRef) return;` — вызов диагностики поставить до этой строки).

В `RulesView.tsx` в существующий `useEffect` (строка ~185):

```tsx
  useEffect(() => {
    setResponseDataType(fieldsToType(fields));
    setPathHintContext(fields, draft.pattern);
  }, [fields, draft.pattern]);
```

(импорт `setPathHintContext` из `../scripting/pathHints`).

- [ ] **Step 5: Прогнать** — `pnpm vitest run` → PASS. Ручная проверка (запуск приложения — по желанию исполнителя, поведение подтверждается уже покрытой чистой логикой + типами).

- [ ] **Step 6: Commit**

```bash
git add src/scripting/pathHints.ts src/scripting/pathHints.test.ts src/monaco-setup.ts src/components/ScriptEditor.tsx src/components/RulesView.tsx
git commit -m "feat(ui): подсказки JSONPath в Monaco — автокомплит по трафику, маркеры, счётчик узлов"
```

---

### Task 16: Dry-run UI в редакторе правил

**Files:**
- Modify: `src/components/RulesView.tsx` (кнопка + панель результата)
- Test: ручная проверка + существующие vitest не ломаются

**Interfaces:**
- Consumes: Tauri-команда `test_rule` (Task 12), тип результата `{flowId, action, error, trace, before, after}`.

- [ ] **Step 1: Состояние и запуск.** В компонент редактора правила RulesView добавить:

```tsx
interface DryRunResult {
  flowId: number;
  action: string;
  error: string | null;
  trace: { rule?: string; op: string; path?: string; nodes?: number; status?: number; ms?: number }[];
  before: { status?: number; body?: string } | null;
  after: { status?: number; body?: string } | null;
}
```

```tsx
  const [dryRun, setDryRun] = useState<DryRunResult | null>(null);
  const [dryRunBusy, setDryRunBusy] = useState(false);
  const runDryRun = async () => {
    setDryRunBusy(true);
    try {
      const r = await invoke<DryRunResult>("test_rule", {
        script: draft.script,
        phase: draft.phase,
        pattern: draft.pattern,
        flowId: null,
      });
      setDryRun(r);
    } catch (e) {
      showToast(String(e));
    } finally {
      setDryRunBusy(false);
    }
  };
```

Кнопка в тулбар редактора правила (рядом с существующими Input pattern/name, стиль соседних кнопок):

```tsx
        <Button size="sm" variant="outline" disabled={dryRunBusy} onClick={() => void runDryRun()}>
          {dryRunBusy ? "Проверяю…" : "Проверить на трафике"}
        </Button>
```

- [ ] **Step 2: Панель результата** (рендер под редактором, когда `dryRun != null`):

```tsx
      {dryRun && (
        <div className="max-h-64 overflow-auto border-t border-border bg-card p-3 text-xs">
          <div className="mb-2 flex items-center gap-3">
            <span className="font-semibold">Dry-run · flow #{dryRun.flowId} · {dryRun.action}</span>
            <button className="ml-auto text-muted-foreground hover:text-foreground" onClick={() => setDryRun(null)}>
              закрыть
            </button>
          </div>
          {dryRun.error && <div className="mb-2 text-http-red">{dryRun.error}</div>}
          {dryRun.trace.length > 0 && (
            <div className="mb-2 font-mono text-muted-foreground">
              {dryRun.trace.map((t, i) => (
                <div key={i}>
                  {t.op}{t.path ? `('${t.path}')` : ""}
                  {t.nodes !== undefined ? ` → ${t.nodes} узлов` : ""}
                  {t.status !== undefined ? ` → ${t.status} (${t.ms} ms)` : ""}
                </div>
              ))}
            </div>
          )}
          {dryRun.after && (
            <div className="grid grid-cols-2 gap-2">
              <div>
                <div className="mb-1 font-semibold text-muted-foreground">До</div>
                <pre className="overflow-auto whitespace-pre-wrap break-all rounded bg-secondary/40 p-2 font-mono">
                  {formatMaybeJson(dryRun.before?.body)}
                </pre>
              </div>
              <div>
                <div className="mb-1 font-semibold text-muted-foreground">После</div>
                <pre className="overflow-auto whitespace-pre-wrap break-all rounded bg-secondary/40 p-2 font-mono">
                  {formatMaybeJson(dryRun.after?.body)}
                </pre>
              </div>
            </div>
          )}
        </div>
      )}
```

Хелпер в том же файле:

```tsx
function formatMaybeJson(s: string | undefined): string {
  if (!s) return "—";
  try { return JSON.stringify(JSON.parse(s), null, 2); } catch { return s; }
}
```

- [ ] **Step 3: Прогнать** — `pnpm vitest run` → PASS, `pnpm tsc --noEmit` (или соответствующий скрипт) — чисто.

- [ ] **Step 4: Commit**

```bash
git add src/components/RulesView.tsx
git commit -m "feat(ui): dry-run правила на захваченном трафике из редактора"
```

---

### Task 17: Cookbook + MCP-справка

**Files:**
- Create: `docs/scripting-cookbook.md`
- Modify: `src-tauri/src/mcp/core_tools.rs:402-403, 436-443` (константы + tool_scripting_reference)

**Interfaces:**
- Produces: MCP `get_scripting_reference` дополнительно возвращает `docsManifest` (stdlib-docs.ts), `cookbook` (markdown) и `commonMistakes` (строка).

- [ ] **Step 1: Написать `docs/scripting-cookbook.md`** — рецепты «задача → правило целиком». Обязательный состав (каждый рецепт: заголовок, phase+pattern, скрипт в 1-5 строк):

```markdown
# Рецепты правил trawl

Каждый рецепт — готовое правило: паттерн, фаза, скрипт. Справка по функциям —
Function library в приложении, синтаксис путей — шпаргалка JSONPath там же.

## 1. Замокать эндпоинт целиком
phase: request, pattern: `*/api/config*`
```js
json({ featureFlags: { newUi: true }, maintenance: false });
```

## 2. Проставить поле во всех элементах массива
phase: handler, pattern: `app.kolesa.kz/v3/adverts/recommendation*`
```js
const res = send(request);
patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00'));
return res;
```

## 3. Изменить только элементы, подходящие под условие
phase: handler
```js
const res = send(request);
tryPatch(res, "items[?@.type=='advert'].advertData.price", p => p * 2);
return res;
```

## 4. Удалить поле везде, где встречается
phase: response
```js
removeAt(response, '$..recommendationAnalyticsData');
```

## 5. Редирект запросов на стейджинг
phase: request, pattern: `api.example.com/*`
```js
rewriteHost(request, 'staging.example.com');
```

## 6. Переписать версию API в пути
phase: request
```js
rewritePath(request, '/v3/', '/v4/');
```

## 7. Эмуляция медленной сети
phase: handler
```js
delay(3000);
return send(request);
```

## 8. Эмуляция 500-й ошибки
phase: request
```js
httpError(500, 'внутренняя ошибка (тест)');
```

## 9. Подложить свой query-параметр
phase: request
```js
setQueryParam(request, 'limit', 100);
```

## 10. Вытащить токен из ответа логина в env
phase: response, pattern: `*/auth/login*`
```js
env.token = pickOne(response, 'data.accessToken');
```

## 11. Подставить сохранённый токен в запросы
phase: request
```js
bearer(env.token);
```

## 12. A/B: часть ответов подменять
phase: handler
```js
const res = send(request);
if (randomInt(1, 100) <= 50) tryPatch(res, 'experiments.variant', 'B');
return res;
```

## 13. Обогатить каждый элемент массива
phase: handler
```js
const res = send(request);
mergeAt(res, 'items[*]', { debugMark: uuid() });
return res;
```

## 14. Оставить в ответе только первые 3 элемента
phase: handler
```js
const res = send(request);
patch(res, 'items', items => items.slice(0, 3));
return res;
```

## 15. Ретрай нестабильного апстрима
phase: handler
```js
return sendWithRetry(request, { retries: 5, delay: 500 });
```

## Частые ошибки
- `send()` возвращает `{status, headers, body}` — поля `.data` у него НЕТ.
  Парсенный JSON даёт `sendJsonRequest()` (поле `.data`) либо `jsonBody(res)`.
- Мутация `res.data`/распарсенного объекта сама по себе НЕ меняет `body` —
  сериализуйте назад через `setJsonBody(res, obj)`. `patch`/`removeAt`/`mergeAt`
  делают это автоматически.
- handler-правило обязано вернуть ответ: `return res;`.
- `patch` с 0 совпадений — ошибка (fail-closed). Для опциональных полей — `tryPatch`.
- `delay()` работает только в handler-фазе.
```

- [ ] **Step 2: MCP.** В `core_tools.rs` рядом с существующими константами (строки 402-403):

```rust
const SCRIPT_DOCS_MANIFEST: &str = include_str!("../../../src/scripting/stdlib-docs.ts");
const SCRIPT_COOKBOOK: &str = include_str!("../../../docs/scripting-cookbook.md");
```

`tool_scripting_reference` дополнить:

```rust
fn tool_scripting_reference(deps: &Deps) -> Result<Value, String> {
    let library = crate::rules::load_library(&deps.rules_dir).unwrap_or_default();
    Ok(json!({
        "apiTypes": SCRIPT_API_DTS,
        "stdlib": SCRIPT_STDLIB,
        "docsManifest": SCRIPT_DOCS_MANIFEST,
        "cookbook": SCRIPT_COOKBOOK,
        "librarySource": library,
        "commonMistakes": "send() не имеет .data (только sendJsonRequest). Мутация распарсенного объекта не меняет body — нужен setJsonBody (patch/removeAt/mergeAt делают это сами). handler обязан return response. patch с 0 узлов — ошибка, для опциональных полей tryPatch. delay() — только handler-фаза.",
    }))
}
```

Обновить description инструмента `save_rule` (строка ~100): добавить в конец `" Script is validated (JS syntax + JSONPath literals) before saving; use test_rule for a dry-run."`.

- [ ] **Step 3: Прогнать всё**

Run: `cd src-tauri && cargo test && cd .. && pnpm vitest run`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add docs/scripting-cookbook.md src-tauri/src/mcp/core_tools.rs
git commit -m "docs(scripting): cookbook рецептов + расширенный get_scripting_reference в MCP"
```

---

## Финальная проверка (после всех задач)

- [ ] `cd src-tauri && cargo test` — весь бэкенд зелёный.
- [ ] `pnpm vitest run` — весь фронтенд зелёный.
- [ ] `pnpm tsc --noEmit` (если настроен) — без ошибок типов.
- [ ] Смоук вручную: запустить приложение, открыть правило `GET /v3/adverts/recommendation`, заменить скрипт на
  `const res = send(request); patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00')); return res;`,
  нажать «Проверить на трафике» — увидеть diff и трассу `patch → 20 узлов`; сохранить; прогнать живой запрос.
