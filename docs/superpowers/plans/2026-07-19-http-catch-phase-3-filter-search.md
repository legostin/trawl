# http-catch — Phase 3: Фильтр и поиск — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Дать возможность быстро находить нужные запросы в потоке трафика — текстовый поиск по URL плюс фильтры по методу и классу статуса, с живым счётчиком «показано / всего».

**Architecture:** Фильтрация целиком на фронтенде: потоки уже в zustand-сторе. Выносим чистую функцию-предикат `flowMatches(flow, filter)` в отдельный модуль и покрываем её vitest-тестами. Состояние фильтра живёт в сторе; `TrafficList` рендерит производный отфильтрованный список; новый `FilterBar` редактирует фильтр.

**Tech Stack:** React + TypeScript, zustand (уже есть), vitest (добавляется — первые фронт-тесты).

## Global Constraints

- **JS-менеджер:** `pnpm`. **Платформа:** macOS.
- **Бэкенд не трогаем** — Phase 3 чисто фронтендовая; модель `Flow` и события не меняются.
- **Чистая логика тестируется** — предикат фильтра не должен зависеть от React/стора.
- **Регистронезависимый** текстовый поиск.
- **Класс статуса:** `2xx | 3xx | 4xx | 5xx`; запрос без ответа (`response === null`) под фильтр по классу статуса не подпадает (кроме значения «any»).
- **TDD**, частые коммиты.

---

## File Structure

**Frontend (`src/`):**
- `filter.ts` (новый) — тип `FlowFilter`, значение по умолчанию `emptyFilter`, чистая функция `flowMatches(flow, filter)`.
- `filter.test.ts` (новый) — vitest-тесты предиката.
- `store.ts` (правится) — состояние фильтра + экшены `setFilter`; селекторы для отфильтрованного списка и счётчиков.
- `components/FilterBar.tsx` (новый) — поле поиска + селекты метода и класса статуса + счётчик.
- `components/TrafficList.tsx` (правится) — рендер отфильтрованного списка.
- `App.tsx` (правится) — вставка `FilterBar` над списком.
- `vitest.config.ts` (новый) и `package.json` (скрипт `test`).

---

### Task 1: Чистый предикат фильтра + vitest

**Files:**
- Create: `src/filter.ts`
- Create: `src/filter.test.ts`
- Create: `vitest.config.ts`
- Modify: `package.json` (dev-dep `vitest`, скрипт `"test": "vitest run"`)

**Interfaces:**
- Consumes: тип `Flow` из `src/types.ts`.
- Produces:
  - `export type StatusClass = "any" | "2xx" | "3xx" | "4xx" | "5xx"`
  - `export interface FlowFilter { query: string; method: string; statusClass: StatusClass }`
  - `export const emptyFilter: FlowFilter`
  - `export function flowMatches(flow: Flow, filter: FlowFilter): boolean`

- [ ] **Step 1: Установить vitest**

Run (из корня репо): `pnpm add -D vitest`
Expected: пакет добавлен.

- [ ] **Step 2: Конфиг vitest и npm-скрипт**

`vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
```

В `package.json` в блок `"scripts"` добавить:

```json
    "test": "vitest run"
```

- [ ] **Step 3: Написать падающие тесты предиката**

`src/filter.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { flowMatches, emptyFilter, type FlowFilter } from "./filter";
import type { Flow } from "./types";

function make(partial: Partial<Flow> & { host?: string; path?: string; status?: number }): Flow {
  const { host = "example.com", path = "/", status, ...rest } = partial;
  return {
    id: 1,
    timestamp: 0,
    method: "GET",
    url: { scheme: "http", host, port: 80, path },
    request: { headers: [], body: [], bodyIsText: true },
    response:
      status === undefined
        ? null
        : { status, headers: [], body: [], bodyIsText: true },
    timings: { sent: null, ttfb: null, done: null },
    state: "completed",
    error: null,
    ...rest,
  };
}

describe("flowMatches", () => {
  it("empty filter matches everything", () => {
    expect(flowMatches(make({}), emptyFilter)).toBe(true);
  });

  it("query matches host case-insensitively", () => {
    const f: FlowFilter = { ...emptyFilter, query: "EXAMPLE" };
    expect(flowMatches(make({ host: "example.com" }), f)).toBe(true);
    expect(flowMatches(make({ host: "other.org" }), f)).toBe(false);
  });

  it("query matches path", () => {
    const f: FlowFilter = { ...emptyFilter, query: "/api/users" };
    expect(flowMatches(make({ path: "/api/users?page=1" }), f)).toBe(true);
    expect(flowMatches(make({ path: "/health" }), f)).toBe(false);
  });

  it("method filter is exact", () => {
    const f: FlowFilter = { ...emptyFilter, method: "POST" };
    expect(flowMatches(make({ method: "POST" }), f)).toBe(true);
    expect(flowMatches(make({ method: "GET" }), f)).toBe(false);
  });

  it("status class matches the right bucket", () => {
    const f: FlowFilter = { ...emptyFilter, statusClass: "4xx" };
    expect(flowMatches(make({ status: 404 }), f)).toBe(true);
    expect(flowMatches(make({ status: 200 }), f)).toBe(false);
  });

  it("status class excludes flows without a response", () => {
    const f: FlowFilter = { ...emptyFilter, statusClass: "2xx" };
    expect(flowMatches(make({ status: undefined }), f)).toBe(false);
  });

  it("combined filters are AND", () => {
    const f: FlowFilter = { query: "example", method: "GET", statusClass: "2xx" };
    expect(flowMatches(make({ host: "example.com", method: "GET", status: 200 }), f)).toBe(true);
    expect(flowMatches(make({ host: "example.com", method: "GET", status: 500 }), f)).toBe(false);
  });
});
```

- [ ] **Step 4: Запустить тесты — убедиться, что падают**

Run (из корня): `pnpm test`
Expected: FAIL — модуль `./filter` не существует.

- [ ] **Step 5: Реализовать предикат**

`src/filter.ts`:

```ts
import type { Flow } from "./types";

export type StatusClass = "any" | "2xx" | "3xx" | "4xx" | "5xx";

export interface FlowFilter {
  query: string;
  method: string; // "" = любой
  statusClass: StatusClass;
}

export const emptyFilter: FlowFilter = { query: "", method: "", statusClass: "any" };

function matchesStatusClass(status: number | undefined, cls: StatusClass): boolean {
  if (cls === "any") return true;
  if (status === undefined) return false;
  const bucket = Math.floor(status / 100);
  return `${bucket}xx` === cls;
}

export function flowMatches(flow: Flow, filter: FlowFilter): boolean {
  if (filter.method && flow.method !== filter.method) return false;

  if (!matchesStatusClass(flow.response?.status, filter.statusClass)) return false;

  const q = filter.query.trim().toLowerCase();
  if (q) {
    const haystack = `${flow.url.host}${flow.url.path}`.toLowerCase();
    if (!haystack.includes(q)) return false;
  }

  return true;
}
```

- [ ] **Step 6: Запустить тесты — убедиться, что проходят**

Run (из корня): `pnpm test`
Expected: PASS (7 тестов).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: чистый предикат фильтра трафика + vitest"
```

---

### Task 2: Состояние фильтра в сторе + FilterBar

**Files:**
- Modify: `src/store.ts`
- Create: `src/components/FilterBar.tsx`

**Interfaces:**
- Consumes: `FlowFilter`, `emptyFilter`, `flowMatches` из `src/filter.ts`.
- Produces (в сторе `useFlows`):
  - поле `filter: FlowFilter`
  - `setFilter: (patch: Partial<FlowFilter>) => void`
  - `filteredFlows: () => Flow[]` — вычисляемый список (метод стора, не поле)
- `FilterBar` — UI редактирования фильтра со счётчиком «показано / всего».

- [ ] **Step 1: Расширить стор состоянием фильтра**

В `src/store.ts` добавить импорт и поля. Изменения точечные:

1. Добавить импорт вверху (после существующих импортов):

```ts
import { flowMatches, emptyFilter, type FlowFilter } from "./filter";
```

2. В интерфейс `FlowsState` добавить поля:

```ts
  filter: FlowFilter;
  setFilter: (patch: Partial<FlowFilter>) => void;
  filteredFlows: () => Flow[];
```

3. В объект стора (в `create<FlowsState>((set, get) => ({ ... }))`) добавить реализацию — например, сразу после `selectedId: null,`:

```ts
  filter: emptyFilter,
  setFilter: (patch) => set((s) => ({ filter: { ...s.filter, ...patch } })),
  filteredFlows: () => {
    const { flows, filter } = get();
    return flows.filter((f) => flowMatches(f, filter));
  },
```

(Импорт типа `Flow` в `store.ts` уже есть — он используется в сигнатурах. Если нет, добавить `import type { Flow } from "./types";`.)

- [ ] **Step 2: FilterBar-компонент**

`src/components/FilterBar.tsx`:

```tsx
import { useFlows } from "../store";
import type { StatusClass } from "../filter";

const METHODS = ["", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const STATUS_CLASSES: StatusClass[] = ["any", "2xx", "3xx", "4xx", "5xx"];

export function FilterBar() {
  const filter = useFlows((s) => s.filter);
  const setFilter = useFlows((s) => s.setFilter);
  const total = useFlows((s) => s.flows.length);
  const shown = useFlows((s) => s.filteredFlows().length);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 8px",
        borderBottom: "1px solid #333",
        fontSize: 12,
      }}
    >
      <input
        value={filter.query}
        onChange={(e) => setFilter({ query: e.target.value })}
        placeholder="Поиск по host/URL…"
        style={{ flex: 1, background: "#2a2a2a", color: "#ddd", border: "1px solid #444", padding: "3px 6px" }}
      />
      <select
        value={filter.method}
        onChange={(e) => setFilter({ method: e.target.value })}
        style={{ background: "#2a2a2a", color: "#ddd" }}
      >
        {METHODS.map((m) => (
          <option key={m} value={m}>
            {m === "" ? "метод: любой" : m}
          </option>
        ))}
      </select>
      <select
        value={filter.statusClass}
        onChange={(e) => setFilter({ statusClass: e.target.value as StatusClass })}
        style={{ background: "#2a2a2a", color: "#ddd" }}
      >
        {STATUS_CLASSES.map((c) => (
          <option key={c} value={c}>
            {c === "any" ? "статус: любой" : c}
          </option>
        ))}
      </select>
      <span style={{ opacity: 0.7, whiteSpace: "nowrap" }}>
        {shown} / {total}
      </span>
    </div>
  );
}
```

- [ ] **Step 3: Проверка типов**

Run (из корня): `pnpm exec tsc --noEmit`
Expected: без ошибок.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: состояние фильтра в сторе + FilterBar со счётчиком"
```

---

### Task 3: Отфильтрованный список в TrafficList + вставка FilterBar

**Files:**
- Modify: `src/components/TrafficList.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `filteredFlows()` из стора, `FilterBar`.
- Produces: список показывает только совпавшие потоки; над списком — панель фильтра.

- [ ] **Step 1: Рендерить отфильтрованные потоки**

В `src/components/TrafficList.tsx` заменить строку получения потоков:

было —
```tsx
  const flows = useFlows((s) => s.flows);
```
стало —
```tsx
  const flows = useFlows((s) => s.filteredFlows());
```

Остальной код компонента (виртуализация по `flows.length`, рендер строк) не меняется — он уже опирается на массив `flows`.

- [ ] **Step 2: Вставить FilterBar над списком в App.tsx**

В `src/App.tsx` добавить импорт:

```tsx
import { FilterBar } from "./components/FilterBar";
```

и обернуть левую колонку так, чтобы `FilterBar` был сверху над `TrafficList`. Заменить блок левой колонки в ветке `view !== "setup"`:

было —
```tsx
          <div style={{ width: "45%", borderRight: "1px solid #333" }}>
            <TrafficList />
          </div>
```
стало —
```tsx
          <div
            style={{
              width: "45%",
              borderRight: "1px solid #333",
              display: "flex",
              flexDirection: "column",
              minHeight: 0,
            }}
          >
            <FilterBar />
            <div style={{ flex: 1, minHeight: 0 }}>
              <TrafficList />
            </div>
          </div>
```

- [ ] **Step 3: Проверка типов и сборка**

Run (из корня): `pnpm exec tsc --noEmit && pnpm build`
Expected: без ошибок типов; сборка проходит.

- [ ] **Step 4: Ручная проверка**

Run: приложение уже в dev-режиме (hot-reload). Если не запущено — `pnpm tauri dev`.
- Start proxy, прогнать трафик: `curl -x http://127.0.0.1:8888 http://example.com/` и пару других запросов.
- Ввести в поле поиска `example` → в списке остаются только запросы к example.com; счётчик показывает «N / M».
- Выбрать метод `POST` или класс статуса → список сужается; сброс на «любой» возвращает всё.

Expected: фильтрация работает вживую, счётчик корректен.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: TrafficList показывает отфильтрованный список + FilterBar в UI"
```

---

## Definition of Done (Phase 3)

- Текстовый поиск по host/URL (регистронезависимо) + фильтры по методу и классу статуса.
- Счётчик «показано / всего» в `FilterBar`.
- `pnpm test` (vitest) — предикат фильтра зелёный; `pnpm exec tsc --noEmit` — без ошибок.
- Ручная проверка: фильтрация в реальном времени работает.

## Вне рамок Phase 3 (следующие планы)

- Phase 4: повтор/Composer. Phase 5: breakpoints. Phase 6: экспорт (HAR/curl) + save/load сессии.
