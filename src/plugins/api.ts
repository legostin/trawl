// The host↔plugin contract. Plugins run in the main app context ("direct import"):
// the host exposes `window.React`, `window.ReactJSXRuntime` and `window.__TRAWL__`
// (this interface), and a plugin bundle (IIFE, built with react/react-dom/jsx as
// externals → those globals) calls `window.__TRAWL__.registerMode(...)` on load.

import type * as React from "react";
import type { AggBucket, FlowQuery, FlowRow, GroupBy, Report } from "@/db";

export interface RegisteredMode {
  id: string;
  label: string;
  /** Optional sidebar icon (e.g. a lucide icon component). */
  icon?: React.ComponentType<{ className?: string }>;
  /** The panel rendered when this mode is active. */
  component: React.ComponentType;
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

/** The host object exposed to plugins as `window.__TRAWL__`. */
export interface TrawlHost {
  version: string;
  react: typeof React;
  events: PluginEvents;
  flows: PluginFlows;
  reports: PluginReports;
  registerMode(mode: RegisteredMode): void;
  log(...args: unknown[]): void;
}

declare global {
  interface Window {
    __TRAWL__?: TrawlHost;
    React?: typeof React;
    ReactJSXRuntime?: unknown;
  }
}
