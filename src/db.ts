import { invoke } from "@tauri-apps/api/core";

/** Filter for querying the persistent flow DB (mirrors src-tauri/src/db.rs). */
export interface FlowQuery {
  query?: string;
  method?: string;
  /** "", "2xx".."5xx" */
  statusClass?: string;
  host?: string;
  projectId?: string;
  startTs?: number;
  endTs?: number;
}

/** Flattened, analytics-oriented flow row (no headers/body). */
export interface FlowRow {
  id: number;
  ts: number;
  method: string;
  scheme: string;
  host: string;
  port: number;
  path: string;
  status: number | null;
  reqSize: number;
  respSize: number;
  durationMs: number | null;
  ttfbMs: number | null;
  projectId: string | null;
  state: string;
  error: string | null;
}

export interface AggBucket {
  key: string;
  count: number;
  avgDurationMs: number | null;
}

export interface Report {
  id: string;
  name: string;
  createdTs: number;
  filterJson: string;
  snapshotJson: string;
}

export type GroupBy = "host" | "status" | "time" | "duration";

export const queryFlows = (filter: FlowQuery, limit = 500, offset = 0): Promise<FlowRow[]> =>
  invoke("query_flows", { filter, limit, offset });

export const flowCount = (filter: FlowQuery): Promise<number> =>
  invoke("flow_count", { filter });

/** Server-side aggregation. `bucket` is the width (ms) for "time"/"duration". */
export const aggregateFlows = (
  filter: FlowQuery,
  groupBy: GroupBy,
  bucket = 0,
  limit = 200,
): Promise<AggBucket[]> => invoke("aggregate_flows", { filter, groupBy, bucket, limit });

export const saveReport = (report: Report): Promise<void> => invoke("save_report", { report });
export const listReports = (): Promise<Report[]> => invoke("list_reports");
export const deleteReport = (id: string): Promise<void> => invoke("delete_report", { id });
