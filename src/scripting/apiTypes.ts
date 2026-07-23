/** TypeScript declarations for the script API — fed into Monaco for autocomplete. */
export const API_DTS = `
/** HTTP request available in a rule. Mutate fields directly. */
interface TrawlRequest {
  /** Method: GET, POST, … */
  method: string;
  /** Full request URL. */
  url: string;
  /** Host, e.g. "api.example.com". */
  host: string;
  /** Path with query, e.g. "/v1/users?page=1". */
  path: string;
  /**
   * Headers as an object. Example:
   *   request.headers['Authorization'] = 'Bearer ' + token;
   */
  headers: Record<string, string>;
  /** Body as text. For JSON: JSON.parse(request.body). */
  body: string;
}

/** HTTP response available in the response phase. */
interface TrawlResponse {
  /** Status code, e.g. 200. */
  status: number;
  headers: Record<string, string>;
  /** Body as text. */
  body: string;
}

interface TrawlMock {
  status?: number;
  headers?: Record<string, string>;
  body?: string;
}

interface TrawlCtx {
  request: TrawlRequest;
  /** Present only in the response phase. */
  response?: TrawlResponse;
  /**
   * Immediately return a synthetic response (mock) without hitting the server.
   * Example: ctx.mock({ status: 200, body: JSON.stringify({ ok: true }) });
   */
  mock(response: TrawlMock): void;
  /** Abort the request with a 502 error. */
  abort(reason?: string): void;
  /**
   * Pause the flow on a breakpoint: it is held in-flight and surfaced in the
   * Traffic view for live editing until you Execute, Respond, or Abort it.
   * Works in the request and response phases.
   */
  breakpoint(): void;
}

/** Context of the current flow. */
declare const ctx: TrawlCtx;
/** Shortcut for ctx.request. */
declare const request: TrawlRequest;
/** Shortcut for ctx.response (in the response phase). */
declare const response: TrawlResponse;

/**
 * Environment variables: Global merged with the active project (project wins
 * on a key clash). Read and write — written values persist to the active
 * project (with no active project — to Global) and are available to later
 * requests. Example: env.token = JSON.parse(response.body).token;
 */
declare const env: Record<string, string>;

// ── handler phase ──

/**
 * Synchronously performs the real HTTP request and returns the response.
 * Available only in the handler phase. Without an argument it sends the current request.
 * Retry example:
 *   let r = send(request);
 *   while (r.status === 429) { sleep(1000); r = send(request); }
 *   return r;
 */
declare function send(req?: TrawlRequest): TrawlResponse;

/** Blocking pause (ms), for retries/polling in the handler phase. */
declare function sleep(ms: number): void;
`;
