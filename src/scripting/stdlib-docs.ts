// Единый манифест документации стандартной библиотеки (src-tauri/js/stdlib.js).
// Питает Function library (RulesView) и синхронизируется тестом
// src/scripting/stdlib-docs.test.ts с STD_DTS (stdlib.ts) и самим stdlib.js —
// меняя сигнатуру/добавляя функцию, обновляйте все три места разом.

export interface StdFnDoc {
  name: string;
  category: string;
  signature: string;
  doc: string;
  example: string;
  phase?: "handler";
}

export const DOC_CATEGORIES = [
  "Тело (JSONPath)",
  "Заголовки",
  "URL и query",
  "Моки и ответы",
  "Данные",
  "Коллекции",
  "Сеть (handler)",
  "Прочее",
] as const;

export const STD_FN_DOCS: StdFnDoc[] = [
  // ── Тело (JSONPath) ──
  {
    name: "patch",
    category: "Тело (JSONPath)",
    signature: "patch(target, path, valueOrFn): number",
    doc: "Записывает значение (или применяет функцию к текущему значению) во всех узлах, совпавших с JSONPath. 0 узлов — ошибка (используйте tryPatch, если поле опционально). target — сообщение (тело парсится/сериализуется автоматически) или обычный объект/массив.",
    example: "patch(response, 'items[*].advertData.addDateFormatted', nowISO())",
  },
  {
    name: "tryPatch",
    category: "Тело (JSONPath)",
    signature: "tryPatch(target, path, valueOrFn): number",
    doc: "То же, что patch(), но отсутствие совпадений не считается ошибкой — просто вернёт 0.",
    example: "tryPatch(response, 'items[*].discount', 0)",
  },
  {
    name: "pick",
    category: "Тело (JSONPath)",
    signature: "pick(target, path): any[]",
    doc: "Возвращает массив всех значений, совпавших с JSONPath.",
    example: "const prices = pick(response, 'items[*].price')",
  },
  {
    name: "pickOne",
    category: "Тело (JSONPath)",
    signature: "pickOne(target, path): any | null",
    doc: "Первое совпавшее значение или null, если совпадений нет.",
    example: "const status = pickOne(response, 'meta.status')",
  },
  {
    name: "removeAt",
    category: "Тело (JSONPath)",
    signature: "removeAt(target, path): number",
    doc: "Удаляет все совпавшие узлы (элементы массива или ключи объекта). Корень не удаляется. Возвращает число удалённых узлов.",
    example: "removeAt(response, 'items[?@.hidden]')",
  },
  {
    name: "mergeAt",
    category: "Тело (JSONPath)",
    signature: "mergeAt(target, path, obj): number",
    doc: "Делает deep-merge объекта в каждый совпавший узел. 0 узлов — ошибка.",
    example: "mergeAt(response, 'items[*]', { promo: true })",
  },
  {
    name: "jsonBody",
    category: "Тело (JSONPath)",
    signature: "jsonBody(msg): any",
    doc: "Парсит тело сообщения как JSON. Возвращает null при пустом или некорректном теле.",
    example: "const body = jsonBody(response)",
  },
  {
    name: "setJsonBody",
    category: "Тело (JSONPath)",
    signature: "setJsonBody(msg, obj): void",
    doc: "Сериализует obj в тело (JSON.stringify) и проставляет content-type: application/json, если он ещё не задан.",
    example: "setJsonBody(response, { ok: true })",
  },

  // ── Заголовки ──
  {
    name: "header",
    category: "Заголовки",
    signature: "header(msg, name): string | undefined",
    doc: "Регистронезависимый поиск заголовка. undefined, если заголовка нет.",
    example: "const auth = header(request, 'authorization')",
  },
  {
    name: "hasHeader",
    category: "Заголовки",
    signature: "hasHeader(msg, name): boolean",
    doc: "true, если заголовок присутствует (регистронезависимо).",
    example: "if (hasHeader(request, 'x-debug')) { /* ... */ }",
  },
  {
    name: "setHeader",
    category: "Заголовки",
    signature: "setHeader(msg, name, value): void",
    doc: "Устанавливает заголовок, заменяя уже существующий с тем же именем (регистронезависимо).",
    example: "setHeader(request, 'x-request-id', uuid())",
  },
  {
    name: "removeHeader",
    category: "Заголовки",
    signature: "removeHeader(msg, name): void",
    doc: "Удаляет заголовок (регистронезависимо). Если его нет — ничего не делает.",
    example: "removeHeader(request, 'if-none-match')",
  },
  {
    name: "bearer",
    category: "Заголовки",
    signature: "bearer(token): void",
    doc: "Устанавливает Authorization: Bearer <token> на текущем запросе.",
    example: "bearer(secret('api_token'))",
  },

  // ── URL и query ──
  {
    name: "queryParam",
    category: "URL и query",
    signature: "queryParam(req, name): string | undefined",
    doc: "Читает декодированный query-параметр из request.path. undefined, если параметра нет.",
    example: "const page = queryParam(request, 'page')",
  },
  {
    name: "setQueryParam",
    category: "URL и query",
    signature: "setQueryParam(req, name, value): void",
    doc: "Устанавливает query-параметр (добавляет или заменяет), синхронно обновляя req.path и req.url.",
    example: "setQueryParam(request, 'debug', '1')",
  },
  {
    name: "removeQueryParam",
    category: "URL и query",
    signature: "removeQueryParam(req, name): void",
    doc: "Удаляет query-параметр, если он есть.",
    example: "removeQueryParam(request, 'utm_source')",
  },
  {
    name: "rewriteHost",
    category: "URL и query",
    signature: "rewriteHost(req, host): void",
    doc: "Меняет host и авторити в url. Заголовок Host не трогает — им управляет прокси.",
    example: "rewriteHost(request, 'staging.example.com')",
  },
  {
    name: "rewritePath",
    category: "URL и query",
    signature: "rewritePath(req, from, to): void",
    doc: "Заменяет часть пути (from — строка или RegExp, заменяются все вхождения); query-часть не затрагивается.",
    example: "rewritePath(request, '/v1/', '/v2/')",
  },
  {
    name: "pathSegments",
    category: "URL и query",
    signature: "pathSegments(req): string[]",
    doc: "Путь без query, разбитый на декодированные непустые сегменты.",
    example: "const [resource, id] = pathSegments(request)",
  },

  // ── Моки и ответы ──
  {
    name: "json",
    category: "Моки и ответы",
    signature: "json(obj) | json(status, obj): TrawlMock",
    doc: "JSON-ответ одной строкой: status по умолчанию 200. В request/response-фазе сразу применяется как мок (ctx.mock), в handler — просто возвращает объект.",
    example: "return json(404, { error: 'not found' })",
  },
  {
    name: "textResponse",
    category: "Моки и ответы",
    signature: "textResponse(status, body, contentType?): TrawlMock",
    doc: "Текстовый ответ; contentType по умолчанию 'text/plain; charset=utf-8'.",
    example: "return textResponse(200, 'OK')",
  },
  {
    name: "httpError",
    category: "Моки и ответы",
    signature: "httpError(status, msg?): TrawlMock",
    doc: "JSON-ответ вида { error: msg }; msg по умолчанию 'HTTP <status>'.",
    example: "return httpError(500, 'upstream unavailable')",
  },
  {
    name: "delay",
    category: "Моки и ответы",
    signature: "delay(ms): void",
    doc: "Блокирующая пауза для эмуляции медленной сети. Только handler-фаза.",
    example: "delay(1500); return send(request);",
    phase: "handler",
  },

  // ── Данные ──
  {
    name: "uuid",
    category: "Данные",
    signature: "uuid(): string",
    doc: "Случайный UUID v4.",
    example: "setHeader(request, 'x-request-id', uuid())",
  },
  {
    name: "randomInt",
    category: "Данные",
    signature: "randomInt(a, b): number",
    doc: "Случайное целое из отрезка [a, b] включительно.",
    example: "patch(response, 'items[*].stock', () => randomInt(0, 100))",
  },
  {
    name: "randomFrom",
    category: "Данные",
    signature: "randomFrom(arr): any",
    doc: "Случайный элемент массива.",
    example: "patch(response, 'status', randomFrom(['ok', 'pending', 'failed']))",
  },
  {
    name: "nowISO",
    category: "Данные",
    signature: "nowISO(shift?, tz?): string",
    doc: "Текущее время в ISO 8601. shift — сдвиг вида '+2d', '-30m', '+1h', '+10s'. tz — смещение вида '+05:00' (по умолчанию UTC, суффикс 'Z').",
    example: "patch(response, 'items[*].addDateFormatted', nowISO('+2d'))",
  },

  // ── Коллекции ──
  {
    name: "groupBy",
    category: "Коллекции",
    signature: "groupBy(arr, key): Record<string, any[]>",
    doc: "Группирует массив по ключу (имя поля или функция). Возвращает объект { значение_ключа: [элементы] }.",
    example: "const byType = groupBy(pick(response, 'items[*]'), 'type')",
  },
  {
    name: "sortBy",
    category: "Коллекции",
    signature: "sortBy(arr, key): any[]",
    doc: "Возвращает отсортированную копию массива (исходный массив не меняется). key — имя поля или функция.",
    example: "const sorted = sortBy(items, (x) => -x.price)",
  },
  {
    name: "uniqBy",
    category: "Коллекции",
    signature: "uniqBy(arr, key): any[]",
    doc: "Убирает дубликаты по ключу, сохраняя первое вхождение.",
    example: "const unique = uniqBy(items, 'id')",
  },
  {
    name: "chunk",
    category: "Коллекции",
    signature: "chunk(arr, n): any[][]",
    doc: "Разбивает массив на подмассивы длиной n (последний может быть короче).",
    example: "const pages = chunk(items, 20)",
  },
  {
    name: "sample",
    category: "Коллекции",
    signature: "sample(arr, n?): any[]",
    doc: "n случайных элементов без повторов (n по умолчанию 1).",
    example: "const picked = sample(items, 3)",
  },

  // ── Сеть (handler) ──
  {
    name: "sendJsonRequest",
    category: "Сеть (handler)",
    signature: "sendJsonRequest(req?): TrawlJsonResponse",
    doc: "Выполняет запрос и парсит JSON-ответ в поле .data (автодополняется по структуре прошлых ответов). Только handler-фаза.",
    example: "const res = sendJsonRequest(request); return json(res.data);",
    phase: "handler",
  },
  {
    name: "sendWithRetry",
    category: "Сеть (handler)",
    signature: "sendWithRetry(req?, { retries?, delay? }): TrawlResponse",
    doc: "Отправляет запрос с автоматическим повтором при 429 и 5xx. retries по умолчанию 3, delay между попытками — 1000 мс. Только handler-фаза.",
    example: "return sendWithRetry(request, { retries: 5, delay: 500 })",
    phase: "handler",
  },

  // ── Прочее ──
  {
    name: "secret",
    category: "Прочее",
    signature: "secret(name): string | null",
    doc: "Читает именованный секрет уровня приложения (Настройки → Секреты, Keychain на macOS). null, если не найден.",
    example: "bearer(secret('api_token'))",
  },
  {
    name: "notify",
    category: "Прочее",
    signature: "notify(text, opts?): void",
    doc: "Ставит уведомление в очередь на отправку (например, в Telegram через плагин уведомлений); отправляется как событие шины notify:send после выполнения правила.",
    example: "notify('429 от upstream', { channel: 'ops' })",
  },
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
