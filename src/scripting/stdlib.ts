// Declarations + docs for the built-in standard library. The implementations
// live in the Rust prelude (src-tauri/src/scripting.rs STD_LIB); these
// declarations power Monaco autocomplete and the read-only "Standard library"
// reference shown in the Function library view.

export const STD_DTS = `
/** A request or response — both have text \`body\` and object \`headers\`. */
type TrawlMessage = TrawlRequest | TrawlResponse;

/** Response of sendJsonRequest(): the raw response plus its parsed JSON in \`data\`. */
interface TrawlJsonResponse extends TrawlResponse {
  /** response.body parsed as JSON (null if empty/invalid). Autocompletes from
   *  the structure of past responses matching this rule's pattern. */
  data: TrawlResponseData;
}

/** Case-insensitive header lookup. Returns undefined when the header is absent. */
declare function header(msg: TrawlMessage, name: string): string | undefined;
/** True when \`msg\` has the given header (case-insensitive). */
declare function hasHeader(msg: TrawlMessage, name: string): boolean;
/** Set a header, replacing any existing one with the same name (case-insensitive). */
declare function setHeader(msg: TrawlMessage, name: string, value: string): void;
/** Remove a header (case-insensitive). */
declare function removeHeader(msg: TrawlMessage, name: string): void;

/** Parse a message body as JSON. Returns null on an empty or invalid body. */
declare function jsonBody(msg: TrawlMessage): any;
/** JSON.stringify \`obj\` into the body and set content-type: application/json if missing. */
declare function setJsonBody(msg: TrawlMessage, obj: any): void;

/** Set \`Authorization: Bearer <token>\` on the current request. */
declare function bearer(token: string): void;
/** Read a query parameter from request.path (decoded). Undefined if absent. */
declare function queryParam(req: TrawlRequest, name: string): string | undefined;

/**
 * Handler phase: perform the request and parse the JSON response into \`.data\`.
 * Example:
 *   const res = sendJsonRequest(request);
 *   res.data.  // ← autocompletes from past response structure
 */
declare function sendJsonRequest(req?: TrawlRequest): TrawlJsonResponse;

/**
 * Handler phase: send with automatic retry on 429 and 5xx.
 * @param opts.retries max attempts (default 3)
 * @param opts.delay ms between attempts (default 1000)
 */
declare function sendWithRetry(
  req?: TrawlRequest,
  opts?: { retries?: number; delay?: number },
): TrawlResponse;

/** Read an app-wide named secret (Settings → Secrets, macOS Keychain). Null when missing. */
declare function secret(name: string): string | null;
/**
 * Queue a notification for delivery (e.g. Telegram via the notifications
 * plugin). Emitted to plugins as the "notify:send" bus event after the rule runs.
 */
declare function notify(text: string, opts?: { channel?: string; title?: string }): void;

/** Write a value (or apply a function to the current value) at every JSONPath match. 0 matches → throws. */
declare function patch(target: TrawlMessage | object, path: string, valueOrFn: any): number;
/** Like patch(), but 0 matches is not an error. */
declare function tryPatch(target: TrawlMessage | object, path: string, valueOrFn: any): number;
/** All matched values as an array. */
declare function pick(target: TrawlMessage | object, path: string): any[];
/** First matched value, or null when there is no match. */
declare function pickOne(target: TrawlMessage | object, path: string): any | null;
/** Remove every matched node (array element or object key). Root is never removed. */
declare function removeAt(target: TrawlMessage | object, path: string): number;
/** Deep-merge \`obj\` into every matched node. 0 matches → throws. */
declare function mergeAt(target: TrawlMessage | object, path: string, obj: object): number;

/** Set a query param (add or replace), keeping req.path/req.url in sync. */
declare function setQueryParam(req: TrawlRequest, name: string, value: string | number): void;
/** Remove a query param if present. */
declare function removeQueryParam(req: TrawlRequest, name: string): void;
/** Rewrite the host in req.host/req.url. The Host header is left untouched. */
declare function rewriteHost(req: TrawlRequest, host: string): void;
/** Rewrite part of the path (from: literal or RegExp). The query string is untouched. */
declare function rewritePath(req: TrawlRequest, from: string | RegExp, to: string): void;
/** Path (without query) split into decoded, non-empty segments. */
declare function pathSegments(req: TrawlRequest): string[];

/** JSON response in one line: status defaults to 200. Mocks in request/response phase, returns the object in handler phase. */
declare function json(obj: any): TrawlMock;
declare function json(status: number, obj: any): TrawlMock;
/** Plain-text response; contentType defaults to text/plain; charset=utf-8. */
declare function textResponse(status: number, body: string, contentType?: string): TrawlMock;
/** JSON error response \`{ error: msg }\`; msg defaults to "HTTP <status>". */
declare function httpError(status: number, msg?: string): TrawlMock;
/** Blocking pause to emulate a slow network. Handler phase only. */
declare function delay(ms: number): void;

/** Random UUID v4. */
declare function uuid(): string;
/** Random integer in [a, b] inclusive. */
declare function randomInt(a: number, b: number): number;
/** Random element of an array. */
declare function randomFrom(arr: any[]): any;
/** Random float in [a, b). */
declare function randomFloat(a: number, b: number): number;
/** true with probability p (default 0.5). */
declare function randomBool(p?: number): boolean;
/** Random realistic full name, e.g. "Anna Clark". */
declare function fakeName(): string;
/** Random realistic email on a test domain, e.g. "anna.clark7@example.com". */
declare function fakeEmail(): string;
/** Random phone number; each '#' in the format becomes a digit. Default format: "+1-555-###-####". */
declare function fakePhone(format?: string): string;
/** nWords of lorem-ipsum text. */
declare function lorem(nWords: number): string;
/** Array of n items built by fn(i) — for list mocks. */
declare function fakeList(n: number, fn: (i: number) => any): any[];
/** Current time as ISO 8601. shift: "+2d"/"-30m"/"+1h"/"+10s"; tz: "+05:00" (default UTC, "Z"). */
declare function nowISO(shift?: string | null, tz?: string | null): string;

/** Base64-encode a string (standard alphabet, with padding). */
declare function base64Encode(s: string): string;
/** Decode standard or url-safe base64, with or without padding. Invalid input → throws. */
declare function base64Decode(s: string): string;
/** Decode a JWT without verifying the signature. Accepts a bare token or "Bearer <token>". */
declare function jwtDecode(token: string): { header: any; payload: any };
/** SHA-256 of a string, lowercase hex. */
declare function sha256(s: string): string;
/** MD5 of a string, lowercase hex. */
declare function md5(s: string): string;
/** HMAC-SHA256 signature of \`s\` with \`key\`, lowercase hex. */
declare function hmacSha256(key: string, s: string): string;

/** All cookies as an object. Request: from the Cookie header; response: the leading name=value of Set-Cookie. */
declare function cookies(msg: TrawlMessage): Record<string, string>;
/** One cookie value, or undefined when absent. */
declare function cookie(msg: TrawlMessage, name: string): string | undefined;
/**
 * Request: add/replace the pair in the Cookie header. Response: write a Set-Cookie
 * header with the given attributes. Note: one Set-Cookie per scripted response.
 */
declare function setCookie(
  msg: TrawlMessage,
  name: string,
  value: string,
  attrs?: {
    path?: string;
    domain?: string;
    maxAge?: number;
    expires?: string;
    secure?: boolean;
    httpOnly?: boolean;
    sameSite?: "Strict" | "Lax" | "None";
  },
): void;
/** Request: drop the cookie pair. Response: write a deletion instruction (Max-Age=0). */
declare function removeCookie(msg: TrawlMessage, name: string, attrs?: { path?: string }): void;
/** Parse an application/x-www-form-urlencoded body into an object (decoded). */
declare function formBody(msg: TrawlMessage): Record<string, string>;
/** One form field from the urlencoded body, or undefined when absent. */
declare function formParam(msg: TrawlMessage, name: string): string | undefined;
/** Encode \`obj\` as an urlencoded body; sets the content-type if missing. */
declare function setFormBody(msg: TrawlMessage, obj: Record<string, string | number | boolean>): void;
/** Add/replace one field in the urlencoded body. */
declare function setFormParam(msg: TrawlMessage, name: string, value: string | number): void;

/** Increment the named in-memory counter and return the new value (first call → 1). Resets on app restart; dry-run uses an isolated store. */
declare function counter(name: string): number;
/** Reset the named counter so the next counter() call returns 1. */
declare function resetCounter(name: string): void;
/** true only on the first call for this name (per app session). */
declare function once(name: string): boolean;
/** true on every n-th call for this name (n, 2n, 3n, …). */
declare function everyNth(name: string, n: number): boolean;

/** Read a variable from env; fallback (default null) when absent. */
declare function getVariable(name: string, fallback?: any): any;
/** Write a variable to env and return the value. Persisted to the active project (or global env) after a real run; never persisted by dry-run. Non-strings are stringified on writeback. */
declare function setVariable(name: string, value: any): any;
/** Delete a variable from env. Deleting a global variable while a project is active does not persist. */
declare function deleteVariable(name: string): void;

/** Group an array by a field name or key function. */
declare function groupBy(arr: any[], key: string | ((x: any) => unknown)): Record<string, any[]>;
/** Sorted copy of the array (does not mutate the input). */
declare function sortBy(arr: any[], key: string | ((x: any) => unknown)): any[];
/** Deduplicate by key, keeping the first occurrence. */
declare function uniqBy(arr: any[], key: string | ((x: any) => unknown)): any[];
/** Split the array into chunks of length n. */
declare function chunk(arr: any[], n: number): any[][];
/** n random elements without repeats (default n = 1). */
declare function sample(arr: any[], n?: number): any[];
`;
