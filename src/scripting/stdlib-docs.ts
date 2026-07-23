// Single documentation manifest for the standard library (src-tauri/js/stdlib.js).
// Powers the Function library (RulesView) and is kept in sync by the test
// src/scripting/stdlib-docs.test.ts with STD_DTS (stdlib.ts) and stdlib.js itself —
// when changing a signature or adding a function, update all three places together.

export interface StdFnDoc {
  name: string;
  category: string;
  signature: string;
  doc: string;
  example: string;
  phase?: "handler";
}

export const DOC_CATEGORIES = [
  "Body (JSONPath)",
  "Headers",
  "URL & query",
  "Mocks & responses",
  "Data",
  "Collections",
  "Network (handler)",
  "Other",
] as const;

export const STD_FN_DOCS: StdFnDoc[] = [
  // ── Body (JSONPath) ──
  {
    name: "patch",
    category: "Body (JSONPath)",
    signature: "patch(target, path, valueOrFn): number",
    doc: "Writes a value (or applies a function to the current value) at every node matched by the JSONPath. 0 nodes is an error (use tryPatch if the field is optional). target — a message (the body is parsed/serialized automatically) or a plain object/array.",
    example: "patch(response, 'items[*].advertData.addDateFormatted', nowISO())",
  },
  {
    name: "tryPatch",
    category: "Body (JSONPath)",
    signature: "tryPatch(target, path, valueOrFn): number",
    doc: "Same as patch(), but no matches is not treated as an error — it just returns 0.",
    example: "tryPatch(response, 'items[*].discount', 0)",
  },
  {
    name: "pick",
    category: "Body (JSONPath)",
    signature: "pick(target, path): any[]",
    doc: "Returns an array of all values matched by the JSONPath.",
    example: "const prices = pick(response, 'items[*].price')",
  },
  {
    name: "pickOne",
    category: "Body (JSONPath)",
    signature: "pickOne(target, path): any | null",
    doc: "The first matched value, or null if there are no matches.",
    example: "const status = pickOne(response, 'meta.status')",
  },
  {
    name: "removeAt",
    category: "Body (JSONPath)",
    signature: "removeAt(target, path): number",
    doc: "Removes every matched node (array elements or object keys). The root is never removed. Returns the number of removed nodes.",
    example: "removeAt(response, 'items[?@.hidden]')",
  },
  {
    name: "mergeAt",
    category: "Body (JSONPath)",
    signature: "mergeAt(target, path, obj): number",
    doc: "Deep-merges an object into every matched node. 0 nodes is an error.",
    example: "mergeAt(response, 'items[*]', { promo: true })",
  },
  {
    name: "jsonBody",
    category: "Body (JSONPath)",
    signature: "jsonBody(msg): any",
    doc: "Parses the message body as JSON. Returns null for an empty or invalid body.",
    example: "const body = jsonBody(response)",
  },
  {
    name: "setJsonBody",
    category: "Body (JSONPath)",
    signature: "setJsonBody(msg, obj): void",
    doc: "Serializes obj into the body (JSON.stringify) and sets content-type: application/json if it isn't already set.",
    example: "setJsonBody(response, { ok: true })",
  },

  // ── Headers ──
  {
    name: "header",
    category: "Headers",
    signature: "header(msg, name): string | undefined",
    doc: "Case-insensitive header lookup. undefined if the header isn't present.",
    example: "const auth = header(request, 'authorization')",
  },
  {
    name: "hasHeader",
    category: "Headers",
    signature: "hasHeader(msg, name): boolean",
    doc: "true if the header is present (case-insensitive).",
    example: "if (hasHeader(request, 'x-debug')) { /* ... */ }",
  },
  {
    name: "setHeader",
    category: "Headers",
    signature: "setHeader(msg, name, value): void",
    doc: "Sets a header, replacing any existing one with the same name (case-insensitive).",
    example: "setHeader(request, 'x-request-id', uuid())",
  },
  {
    name: "removeHeader",
    category: "Headers",
    signature: "removeHeader(msg, name): void",
    doc: "Removes a header (case-insensitive). No-op if it isn't present.",
    example: "removeHeader(request, 'if-none-match')",
  },
  {
    name: "bearer",
    category: "Headers",
    signature: "bearer(token): void",
    doc: "Sets Authorization: Bearer <token> on the current request.",
    example: "bearer(secret('api_token'))",
  },

  // ── URL & query ──
  {
    name: "queryParam",
    category: "URL & query",
    signature: "queryParam(req, name): string | undefined",
    doc: "Reads a decoded query parameter from request.path. undefined if the parameter isn't present.",
    example: "const page = queryParam(request, 'page')",
  },
  {
    name: "setQueryParam",
    category: "URL & query",
    signature: "setQueryParam(req, name, value): void",
    doc: "Sets a query parameter (adds or replaces), updating req.path and req.url in sync.",
    example: "setQueryParam(request, 'debug', '1')",
  },
  {
    name: "removeQueryParam",
    category: "URL & query",
    signature: "removeQueryParam(req, name): void",
    doc: "Removes a query parameter if present.",
    example: "removeQueryParam(request, 'utm_source')",
  },
  {
    name: "rewriteHost",
    category: "URL & query",
    signature: "rewriteHost(req, host): void",
    doc: "Changes the host and authority in the url. Doesn't touch the Host header — that's managed by the proxy.",
    example: "rewriteHost(request, 'staging.example.com')",
  },
  {
    name: "rewritePath",
    category: "URL & query",
    signature: "rewritePath(req, from, to): void",
    doc: "Replaces part of the path (from — a string or RegExp; all occurrences are replaced); the query part is untouched.",
    example: "rewritePath(request, '/v1/', '/v2/')",
  },
  {
    name: "pathSegments",
    category: "URL & query",
    signature: "pathSegments(req): string[]",
    doc: "The path without the query, split into decoded non-empty segments.",
    example: "const [resource, id] = pathSegments(request)",
  },

  // ── Mocks & responses ──
  {
    name: "json",
    category: "Mocks & responses",
    signature: "json(obj) | json(status, obj): TrawlMock",
    doc: "One-line JSON response: status defaults to 200. In the request/response phase it's applied immediately as a mock (ctx.mock); in a handler it just returns the object.",
    example: "return json(404, { error: 'not found' })",
  },
  {
    name: "textResponse",
    category: "Mocks & responses",
    signature: "textResponse(status, body, contentType?): TrawlMock",
    doc: "A text response; contentType defaults to 'text/plain; charset=utf-8'.",
    example: "return textResponse(200, 'OK')",
  },
  {
    name: "httpError",
    category: "Mocks & responses",
    signature: "httpError(status, msg?): TrawlMock",
    doc: "A JSON response shaped like { error: msg }; msg defaults to 'HTTP <status>'.",
    example: "return httpError(500, 'upstream unavailable')",
  },
  {
    name: "delay",
    category: "Mocks & responses",
    signature: "delay(ms): void",
    doc: "A blocking pause to emulate a slow network. Handler phase only.",
    example: "delay(1500); return send(request);",
    phase: "handler",
  },

  // ── Data ──
  {
    name: "uuid",
    category: "Data",
    signature: "uuid(): string",
    doc: "A random UUID v4.",
    example: "setHeader(request, 'x-request-id', uuid())",
  },
  {
    name: "randomInt",
    category: "Data",
    signature: "randomInt(a, b): number",
    doc: "A random integer from the range [a, b] inclusive.",
    example: "patch(response, 'items[*].stock', () => randomInt(0, 100))",
  },
  {
    name: "randomFrom",
    category: "Data",
    signature: "randomFrom(arr): any",
    doc: "A random element from the array.",
    example: "patch(response, 'status', randomFrom(['ok', 'pending', 'failed']))",
  },
  {
    name: "nowISO",
    category: "Data",
    signature: "nowISO(shift?, tz?): string",
    doc: "The current time in ISO 8601. shift — an offset like '+2d', '-30m', '+1h', '+10s'. tz — an offset like '+05:00' (defaults to UTC, 'Z' suffix).",
    example: "patch(response, 'items[*].addDateFormatted', nowISO('+2d'))",
  },

  // ── Collections ──
  {
    name: "groupBy",
    category: "Collections",
    signature: "groupBy(arr, key): Record<string, any[]>",
    doc: "Groups an array by a key (field name or function). Returns an object { key_value: [items] }.",
    example: "const byType = groupBy(pick(response, 'items[*]'), 'type')",
  },
  {
    name: "sortBy",
    category: "Collections",
    signature: "sortBy(arr, key): any[]",
    doc: "Returns a sorted copy of the array (the original array is untouched). key — a field name or function.",
    example: "const sorted = sortBy(items, (x) => -x.price)",
  },
  {
    name: "uniqBy",
    category: "Collections",
    signature: "uniqBy(arr, key): any[]",
    doc: "Removes duplicates by key, keeping the first occurrence.",
    example: "const unique = uniqBy(items, 'id')",
  },
  {
    name: "chunk",
    category: "Collections",
    signature: "chunk(arr, n): any[][]",
    doc: "Splits an array into sub-arrays of length n (the last one may be shorter).",
    example: "const pages = chunk(items, 20)",
  },
  {
    name: "sample",
    category: "Collections",
    signature: "sample(arr, n?): any[]",
    doc: "n random elements with no repeats (n defaults to 1).",
    example: "const picked = sample(items, 3)",
  },

  // ── Network (handler) ──
  {
    name: "sendJsonRequest",
    category: "Network (handler)",
    signature: "sendJsonRequest(req?): TrawlJsonResponse",
    doc: "Performs the request and parses the JSON response into the .data field (autocompleted from the structure of past responses). Handler phase only.",
    example: "const res = sendJsonRequest(request); return json(res.data);",
    phase: "handler",
  },
  {
    name: "sendWithRetry",
    category: "Network (handler)",
    signature: "sendWithRetry(req?, { retries?, delay? }): TrawlResponse",
    doc: "Sends a request, automatically retrying on 429 and 5xx. retries defaults to 3, delay between attempts is 1000 ms. Handler phase only.",
    example: "return sendWithRetry(request, { retries: 5, delay: 500 })",
    phase: "handler",
  },

  // ── Other ──
  {
    name: "secret",
    category: "Other",
    signature: "secret(name): string | null",
    doc: "Reads an app-level named secret (Settings → Secrets, Keychain on macOS). null if not found.",
    example: "bearer(secret('api_token'))",
  },
  {
    name: "notify",
    category: "Other",
    signature: "notify(text, opts?): void",
    doc: "Queues a notification for sending (e.g. to Telegram via the notifications plugin); it's emitted as a notify:send bus event after the rule runs.",
    example: "notify('429 from upstream', { channel: 'ops' })",
  },
];

export const JSONPATH_CHEATSHEET = [
  { syntax: "$", doc: "document root (can be omitted: 'items' == '$.items')" },
  { syntax: "items[*]", doc: "all array elements" },
  { syntax: "items[0] / items[-1]", doc: "by index / from the end" },
  { syntax: "items[0:3]", doc: "slice [from:to)" },
  { syntax: "$..price", doc: "field at any depth" },
  { syntax: "items[?@.type=='advert']", doc: "conditional filter" },
  { syntax: "items[?@.price>1000 && @.isVip]", doc: "boolean conditions" },
  { syntax: "items[?length(@.tags)>2]", doc: "functions: length(), count(), match(), search(), value()" },
  { syntax: "$['key with space']", doc: "names in brackets/quotes" },
];
