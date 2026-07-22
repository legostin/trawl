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
`;

/** One entry per built-in function, for the Function-library reference list. */
export interface StdFn {
  signature: string;
  doc: string;
  phase?: "handler";
}

export const STD_FUNCTIONS: StdFn[] = [
  { signature: "header(msg, name): string | undefined", doc: "Case-insensitive header lookup." },
  { signature: "hasHeader(msg, name): boolean", doc: "True when the header is present." },
  { signature: "setHeader(msg, name, value)", doc: "Set a header, replacing any same-named one." },
  { signature: "removeHeader(msg, name)", doc: "Remove a header (case-insensitive)." },
  { signature: "jsonBody(msg): any", doc: "Parse the body as JSON (null if empty/invalid)." },
  { signature: "setJsonBody(msg, obj)", doc: "Stringify obj into the body + set content-type." },
  { signature: "bearer(token)", doc: "Set Authorization: Bearer <token> on the request." },
  { signature: "queryParam(request, name): string | undefined", doc: "Read a decoded query param." },
  { signature: "ctx.breakpoint()", doc: "Pause the flow for live editing in the Traffic view." },
  {
    signature: "sendJsonRequest(request): { …response, data }",
    doc: "Send and parse the JSON response into .data (autocompletes by structure).",
    phase: "handler",
  },
  {
    signature: "sendWithRetry(request, { retries, delay })",
    doc: "Send with retry on 429/5xx.",
    phase: "handler",
  },
  { signature: "secret(name): string | null", doc: "Read an app-wide named secret (Keychain)." },
  {
    signature: "notify(text, { channel, title })",
    doc: "Queue a notification — delivered by the notifications plugin.",
  },
];
