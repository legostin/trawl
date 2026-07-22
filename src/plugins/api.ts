// The host↔plugin contract. Plugins run in the main app context ("direct import"):
// the host exposes `window.React`, `window.ReactJSXRuntime` and `window.__TRAWL__`
// (this interface), and a plugin bundle (IIFE, built with react/react-dom/jsx as
// externals → those globals) calls `window.__TRAWL__.registerMode(...)` on load.

import type * as React from "react";
import type { AggBucket, FlowQuery, FlowRow, GroupBy, Report } from "@/db";
import type { SendRequest, SendResponse } from "@/http";
import type { Flow, HttpMessage, ResponseMessage } from "@/types";

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
}

/** Reusable host UI components (so plugins render bodies/headers consistently). */
export interface TrawlUi {
  BodyViewer: React.ComponentType<{ msg: HttpMessage | ResponseMessage | null }>;
  HeadersTable: React.ComponentType<{ headers: [string, string][]; emptyText?: string }>;
  MethodBadge: React.ComponentType<{ method: string; className?: string }>;
  StatusBadge: React.ComponentType<{ status: number | undefined; className?: string }>;
}

export interface TrawlUtil {
  bodyText(msg: HttpMessage | ResponseMessage | null): string;
  buildCurl(flow: Flow): string;
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
  /** Create a rule in the active project and open it in the rules editor. */
  create(draft: RuleDraft): Promise<void>;
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

export interface PluginEvents {
  /** Subscribe to an event; returns an unsubscribe fn. */
  on(type: string, cb: (payload: unknown) => void): () => void;
  off(type: string, cb: (payload: unknown) => void): void;
  emit(type: string, payload?: unknown): void;
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
  /** Таймаут вызова, мс (по умолчанию 60000). */
  timeoutMs?: number;
}

export interface TrawlMcp {
  /** Зарегистрировать MCP-тул `<pluginId>_<name>`. Только при инициализации плагина. */
  registerTool(spec: McpToolSpec): Promise<void>;
  unregisterTool(name: string): Promise<void>;
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
  mcp: TrawlMcp;
  ui: TrawlUi;
  util: TrawlUtil;
  registerMode(mode: RegisteredMode): void;
  /** Add an action button to the request-detail toolbar. */
  registerFlowAction(action: FlowAction): void;
  /** Open a URL in the system's default browser. */
  openUrl(url: string): Promise<void>;
  /** Switch the active top-level mode (e.g. to open this plugin's mode). */
  setMode(id: string): void;
  log(...args: unknown[]): void;
}

declare global {
  interface Window {
    __TRAWL__?: TrawlHost;
    React?: typeof React;
    ReactJSXRuntime?: unknown;
  }
}
