// The host↔plugin contract. Plugins run in the main app context ("direct import"):
// the host exposes `window.React`, `window.ReactJSXRuntime` and `window.__TRAWL__`
// (this interface), and a plugin bundle (IIFE, built with react/react-dom/jsx as
// externals → those globals) calls `window.__TRAWL__.registerMode(...)` on load.

import type * as React from "react";
import type { AggBucket, FlowQuery, FlowRow, GroupBy, Report } from "@/db";
import type { SendRequest, SendResponse } from "@/http";
import type { Flow, HttpMessage, ResponseMessage } from "@/types";
import type { EventInfo, EventMeta } from "./bus";

export type { EventInfo, EventMeta, EventParam } from "./bus";

/** Version of the host↔plugin API this app provides (`window.__TRAWL__.version`).
 *  Bump when the contract below grows; plugin manifests declare the version they
 *  need via `apiVersion`, and the installer refuses plugins that need a newer one. */
export const HOST_API_VERSION = "1.14.0";

export interface RegisteredMode {
  id: string;
  label: string;
  /** Optional sidebar icon (e.g. a lucide icon component). */
  icon?: React.ComponentType<{ className?: string }>;
  /** The panel rendered when this mode is active. */
  component: React.ComponentType;
}

/** An action button injected into the request-detail toolbar. */
export interface FlowAction {
  id: string;
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
  run(flow: Flow): void;
  /** Stamped by the host at registration; a plugin does not set it. Without it
   *  an uninstalled plugin's buttons stay in the toolbar. */
  pluginId?: string;
}

/** A section a plugin renders inside the request-detail card, as its own tab. */
export interface FlowPanel {
  id: string;
  label: string;
  component: React.ComponentType<{ flow: Flow }>;
  /** Stamped by the host at registration; a plugin does not set it. */
  pluginId?: string;
}

/** Imperative handle on the host's code editor. */
export interface ScriptEditorApi {
  /** Insert text at the cursor (replacing the selection). */
  insert(text: string): void;
  /** Replace the whole document, keeping undo history. */
  replaceAll(text: string): void;
  getSelectionText(): string;
  getValue(): string;
  /** Mark lines (1-based) and scroll the first one into view. Pass an empty
   *  array to clear. Requires host API 1.11.0. */
  highlightLines?(lines: number[], kind?: "error" | "warning" | "info"): void;
  /** Insert whole lines before line `at` (1-based), keeping undo history. */
  insertLines?(at: number, text: string): void;
}

export interface CompletionItem {
  label: string;
  /** What gets inserted; defaults to `label`. `$0` marks the caret. */
  insert?: string;
  detail?: string;
  documentation?: string;
  kind?: "function" | "variable" | "file" | "snippet" | "keyword";
}

export interface CompletionContext {
  /** Text of the line up to the caret — enough to decide what to offer. */
  linePrefix: string;
  /** The whole document. */
  text: string;
}

/** Plugin-supplied completions in the host's editor (requires 1.9.0). */
export interface TrawlEditor {
  registerCompletions(spec: {
    language?: string;
    triggerCharacters?: string[];
    provide(context: CompletionContext): CompletionItem[];
  }): () => void;
}

/** Reusable host UI components (so plugins render bodies/headers consistently). */
export interface ToolbarItem {
  id: string;
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
  /** Higher stays visible longer when the row runs out of room. */
  priority?: number;
}

export interface TrawlUi {
  /** A row of buttons that moves what does not fit into a "⋯" menu instead of
   *  letting it be clipped. Requires host API 1.13.0. */
  Toolbar: React.ComponentType<{ items: ToolbarItem[]; className?: string }>;
  BodyViewer: React.ComponentType<{ msg: HttpMessage | ResponseMessage | null }>;
  HeadersTable: React.ComponentType<{ headers: [string, string][]; emptyText?: string }>;
  MethodBadge: React.ComponentType<{ method: string; className?: string }>;
  StatusBadge: React.ComponentType<{ status: number | undefined; className?: string }>;
  /** Monaco-backed code editor wired to the host's completion setup —
   *  the same component the rules editor uses. */
  ScriptEditor: React.ComponentType<{
    value: string;
    onChange: (v: string) => void;
    language?: string;
    /** Imperative handle: insert at the cursor, replace all, read selection. */
    apiRef?: React.MutableRefObject<ScriptEditorApi | null>;
  }>;
  /** Host's themed button (variant/size are loosely typed here to avoid leaking cva internals). */
  Button: React.ComponentType<
    React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: string; size?: string }
  >;
  /** Host's themed text input. */
  Input: React.ComponentType<React.InputHTMLAttributes<HTMLInputElement>>;
  /** Host's themed select. */
  Select: React.ComponentType<React.SelectHTMLAttributes<HTMLSelectElement>>;
}

export interface TrawlUtil {
  bodyText(msg: HttpMessage | ResponseMessage | null): string;
  buildCurl(flow: Flow): string;
  /** TS type expression inferred from sample values (for setPayloadType). */
  inferTypeBody(samples: unknown[]): string;
  /** Flat field list (path/type/example) inferred from sample values. */
  inferFields(samples: unknown[]): { path: string; type: string; example?: string }[];
  /** Type the global `payload` in Monaco editors (subscription condition hints). */
  setPayloadType(typeBody: string): void;
}

export interface TrawlHttp {
  send(req: SendRequest, viaProxy?: boolean): Promise<SendResponse>;
}

export interface EnvVar {
  key: string;
  value: string;
}

export interface ActiveProject {
  id: string;
  name: string;
  env: EnvVar[];
}

/** Access to the active project + its environment variables (shared with capture/scripts). */
export interface TrawlProjects {
  /** The active project (or null when capturing all domains). */
  active(): ActiveProject | null;
  /** Persist the active project's env vars. No-op if no active project. */
  setEnv(env: EnvVar[]): Promise<void>;
  /** Subscribe to active-project changes; returns an unsubscribe fn. */
  onChange(cb: (project: ActiveProject | null) => void): () => void;
}

/** A rule to be created by a plugin (id/enabled/project are filled by the host). */
export interface RuleDraft {
  name: string;
  pattern: string;
  phase: "request" | "response" | "both" | "handler";
  script: string;
}

export interface TrawlRules {
  /** Create a rule in the active project; resolves with its id. By default the
   *  rules editor opens on it — pass `{ open: false }` for automation. */
  create(draft: RuleDraft, options?: { open?: boolean }): Promise<string>;
  /** Delete a rule. Unknown ids are a no-op. Requires 1.10.0. */
  remove(id: string): Promise<void>;
  /** Rules of the active project plus global ones. Requires 1.10.0. */
  list(): Promise<(RuleDraft & { id: string; enabled: boolean; projectId: string | null })[]>;
}

/** Per-host git access tokens (entered once at plugin-install time). Plugins
 *  run with full app access, so browsing logic lives in plugins; the host only
 *  stores tokens and hands them out. */
export interface TrawlGitHosts {
  token(host: string): Promise<string | null>;
  hasToken(host: string): Promise<boolean>;
  setToken(host: string, token: string): Promise<void>;
}

/** Project-scoped JSON key/value storage for plugins (persisted to disk). */
export interface TrawlStorage {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
}

/** App-wide named secrets (macOS Keychain). Shared with rule scripts (secret()). */
export interface TrawlSecrets {
  list(): Promise<string[]>;
  get(name: string): Promise<string | null>;
  set(name: string, value: string): Promise<void>;
  remove(name: string): Promise<void>;
}

export interface PluginEvents {
  /** Subscribe to an event; returns an unsubscribe fn. */
  on(type: string, cb: (payload: unknown) => void): () => void;
  off(type: string, cb: (payload: unknown) => void): void;
  emit(type: string, payload?: unknown): void;
  /** Declare an event + payload type so other plugins get hints for it. */
  describe(type: string, meta: EventMeta): void;
  /** Declared and observed events (with last payloads) for subscription UIs. */
  known(): EventInfo[];
}

export interface PluginFlows {
  query(filter: FlowQuery, limit?: number, offset?: number): Promise<FlowRow[]>;
  count(filter: FlowQuery): Promise<number>;
  aggregate(
    filter: FlowQuery,
    groupBy: GroupBy,
    bucket?: number,
    limit?: number,
  ): Promise<AggBucket[]>;
  /** Live capture: fires on every new/updated flow. Returns an unsubscribe fn. */
  subscribe(cb: (flow: unknown) => void): () => void;
}

export interface PluginReports {
  save(report: Report): Promise<void>;
  list(): Promise<Report[]>;
  remove(id: string): Promise<void>;
}

export interface McpToolSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (args: unknown) => unknown | Promise<unknown>;
  /** Call timeout, ms (default 60000). */
  timeoutMs?: number;
}

export interface TrawlMcp {
  /** Register the MCP tool `<pluginId>_<name>`. Only during plugin initialization. */
  registerTool(spec: McpToolSpec): Promise<void>;
  unregisterTool(name: string): Promise<void>;
}

export interface CaptureStatus {
  running: boolean;
  /** The proxy's port while it runs — what a plugin points a browser at. */
  port: number | null;
}

export interface TrawlCapture {
  status(): CaptureStatus;
  /** Start the proxy if it isn't running; resolves with the live status. */
  start(): Promise<CaptureStatus>;
  stop(): Promise<void>;
  onChange(cb: (status: CaptureStatus) => void): () => void;
}

export interface TrawlDialog {
  /** Native folder picker. Resolves to null when the user cancels. */
  pickFolder(options?: { title?: string; defaultPath?: string }): Promise<string | null>;
  /** Native file picker. `filters` is [{ name, extensions }]. */
  pickFile(options?: {
    title?: string;
    defaultPath?: string;
    filters?: { name: string; extensions: string[] }[];
  }): Promise<string | null>;
}

export interface ProcessInfo {
  id: string;
  pid: number;
  pluginId: string;
  command: string;
  startedAt: number;
}

export interface ProcessLine {
  stream: "stdout" | "stderr";
  text: string;
}

/** Child processes owned by the calling plugin. Killed when it is disabled,
 *  reloaded, removed, or the app exits — a plugin's process never outlives it.
 *
 *  The first spawn asks the user in a native dialog, once per plugin per run of
 *  the app. The prompt is not drawn in the page: a plugin shares the page with
 *  the host and could otherwise answer for the user. */
export interface TrawlProcess {
  /** Start a process. `command` is resolved against the user's login-shell PATH,
   *  so `npx` and friends work from a GUI app. */
  spawn(request: {
    command: string;
    args?: string[];
    cwd?: string;
    env?: Record<string, string>;
  }): Promise<ProcessInfo>;
  /** Subscribe to a process's output lines; returns an unsubscribe fn. */
  onOutput(id: string, cb: (line: ProcessLine) => void): () => void;
  /** Subscribe to a process's exit; returns an unsubscribe fn. */
  onExit(id: string, cb: (event: { code: number | null }) => void): () => void;
  kill(id: string): Promise<void>;
  /** This plugin's still-running processes. */
  list(): Promise<ProcessInfo[]>;
}

/** The host object exposed to plugins as `window.__TRAWL__`. */
export interface TrawlHost {
  version: string;
  react: typeof React;
  events: PluginEvents;
  flows: PluginFlows;
  reports: PluginReports;
  http: TrawlHttp;
  projects: TrawlProjects;
  gitHosts: TrawlGitHosts;
  rules: TrawlRules;
  storage: TrawlStorage;
  secrets: TrawlSecrets;
  mcp: TrawlMcp;
  /** Proxy control: point a spawned browser at Trawl. Requires 1.8.0. */
  capture: TrawlCapture;
  /** Completions in the host's code editor. Requires 1.9.0. */
  editor: TrawlEditor;
  /** Native dialogs. Requires host API 1.8.0 — feature-detect before use. */
  dialog: TrawlDialog;
  /** Child processes owned by this plugin. Requires host API 1.8.0. */
  process: TrawlProcess;
  ui: TrawlUi;
  util: TrawlUtil;
  registerMode(mode: RegisteredMode): void;
  /** Add an action button to the request-detail toolbar. */
  registerFlowAction(action: FlowAction): void;
  /** Add a tab to the request-detail card. Requires host API 1.12.0. */
  registerFlowPanel(panel: FlowPanel): void;
  /** Open a URL in the system's default browser. */
  openUrl(url: string): Promise<void>;
  /** Switch the active top-level mode (e.g. to open this plugin's mode). */
  setMode(id: string): void;
  /** Show one captured request: switches to the traffic view and selects it.
   *  A plugin that lists flows can then lead back to them. Requires 1.14.0. */
  openFlow(id: number): void;
  log(...args: unknown[]): void;
}

declare global {
  interface Window {
    __TRAWL__?: TrawlHost;
    React?: typeof React;
    ReactJSXRuntime?: unknown;
  }
}
