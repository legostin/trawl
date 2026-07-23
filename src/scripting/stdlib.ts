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
/** Current time as ISO 8601. shift: "+2d"/"-30m"/"+1h"/"+10s"; tz: "+05:00" (default UTC, "Z"). */
declare function nowISO(shift?: string | null, tz?: string | null): string;

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
