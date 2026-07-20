//! Persistent shared request database (SQLite).
//!
//! Captured flows are mirrored into `trawl.db` so history survives restarts and
//! can be queried/aggregated for analytics. Writes go through an actor thread
//! (`DbHandle`) so the async proxy never blocks on SQLite; reads open their own
//! connection (`Db`) — WAL mode lets readers run concurrently with the writer.

use anyhow::{Context, Result};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::model::{Flow, FlowState};

/// Keep at most this many most-recent flows in the DB.
const MAX_FLOWS: u64 = 200_000;
/// Prune after this many inserts.
const PRUNE_INTERVAL: u64 = 500;

/// Flattened, analytics-friendly projection of a `Flow` (no headers/body blobs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRow {
    pub id: u64,
    pub ts: u64,
    pub method: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub status: Option<u16>,
    pub req_size: u64,
    pub resp_size: u64,
    pub duration_ms: Option<u64>,
    pub ttfb_ms: Option<u64>,
    pub project_id: Option<String>,
    pub state: String,
    pub error: Option<String>,
}

fn state_str(s: &FlowState) -> String {
    match s {
        FlowState::Pending => "pending",
        FlowState::Completed => "completed",
        FlowState::Error => "error",
        FlowState::Paused => "paused",
    }
    .to_string()
}

impl FlowRow {
    pub fn from_flow(f: &Flow, project_id: Option<&str>) -> FlowRow {
        // Timings are ms since proxy-session start; derive durations relative to `sent`.
        let duration_ms = match (f.timings.sent, f.timings.done) {
            (Some(s), Some(d)) => Some(d.saturating_sub(s)),
            _ => None,
        };
        let ttfb_ms = match (f.timings.sent, f.timings.ttfb) {
            (Some(s), Some(t)) => Some(t.saturating_sub(s)),
            _ => None,
        };
        FlowRow {
            id: f.id,
            ts: f.timestamp,
            method: f.method.clone(),
            scheme: f.url.scheme.clone(),
            host: f.url.host.clone(),
            port: f.url.port,
            path: f.url.path.clone(),
            status: f.response.as_ref().map(|r| r.status),
            req_size: f.request.body.len() as u64,
            resp_size: f.response.as_ref().map(|r| r.body.len() as u64).unwrap_or(0),
            duration_ms,
            ttfb_ms,
            project_id: project_id.map(|s| s.to_string()),
            state: state_str(&f.state),
            error: f.error.clone(),
        }
    }
}

/// Filter mirroring `src/filter.ts` plus an optional time range / host / project.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub method: String,
    /// "", "any", "2xx".."5xx".
    #[serde(default)]
    pub status_class: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub start_ts: Option<u64>,
    #[serde(default)]
    pub end_ts: Option<u64>,
}

/// One aggregation bucket (host, status-class, time bucket, or duration bucket).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggBucket {
    pub key: String,
    pub count: u64,
    pub avg_duration_ms: Option<f64>,
}

/// A saved analytics report: a named filter + a JSON snapshot of its aggregates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub name: String,
    pub created_ts: u64,
    pub filter_json: String,
    pub snapshot_json: String,
}

fn build_where(q: &FlowQuery) -> (String, Vec<SqlValue>) {
    let mut conds: Vec<String> = Vec::new();
    let mut params: Vec<SqlValue> = Vec::new();
    if !q.method.is_empty() {
        conds.push("method = ?".into());
        params.push(SqlValue::Text(q.method.clone()));
    }
    if let Some(base) = status_class_base(&q.status_class) {
        conds.push("status >= ? AND status < ?".into());
        params.push(SqlValue::Integer(base));
        params.push(SqlValue::Integer(base + 100));
    }
    if !q.query.is_empty() {
        conds.push("LOWER(host || path) LIKE ?".into());
        params.push(SqlValue::Text(format!("%{}%", q.query.to_lowercase())));
    }
    if let Some(h) = q.host.as_ref().filter(|h| !h.is_empty()) {
        conds.push("host = ?".into());
        params.push(SqlValue::Text(h.clone()));
    }
    if let Some(p) = q.project_id.as_ref().filter(|p| !p.is_empty()) {
        conds.push("project_id = ?".into());
        params.push(SqlValue::Text(p.clone()));
    }
    if let Some(s) = q.start_ts {
        conds.push("ts >= ?".into());
        params.push(SqlValue::Integer(s as i64));
    }
    if let Some(e) = q.end_ts {
        conds.push("ts <= ?".into());
        params.push(SqlValue::Integer(e as i64));
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    (where_sql, params)
}

fn status_class_base(sc: &str) -> Option<i64> {
    match sc {
        "2xx" | "3xx" | "4xx" | "5xx" => Some(((sc.as_bytes()[0] - b'0') as i64) * 100),
        _ => None,
    }
}

fn row_to_flow(r: &Row) -> rusqlite::Result<FlowRow> {
    Ok(FlowRow {
        id: r.get::<_, i64>("id")? as u64,
        ts: r.get::<_, i64>("ts")? as u64,
        method: r.get("method")?,
        scheme: r.get("scheme")?,
        host: r.get("host")?,
        port: r.get::<_, i64>("port")? as u16,
        path: r.get("path")?,
        status: r.get::<_, Option<i64>>("status")?.map(|v| v as u16),
        req_size: r.get::<_, i64>("req_size")? as u64,
        resp_size: r.get::<_, i64>("resp_size")? as u64,
        duration_ms: r.get::<_, Option<i64>>("duration_ms")?.map(|v| v as u64),
        ttfb_ms: r.get::<_, Option<i64>>("ttfb_ms")?.map(|v| v as u64),
        project_id: r.get("project_id")?,
        state: r.get("state")?,
        error: r.get("error")?,
    })
}

/// A synchronous connection to the flow DB. Used by the writer thread and by
/// read commands (each opens its own — SQLite/WAL allows concurrent readers).
pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path).context("open trawl.db")?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .context("set pragmas")?;
        Ok(Db { conn })
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS flows (
                    id INTEGER PRIMARY KEY,
                    ts INTEGER NOT NULL,
                    method TEXT NOT NULL,
                    scheme TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    path TEXT NOT NULL,
                    status INTEGER,
                    req_size INTEGER NOT NULL,
                    resp_size INTEGER NOT NULL,
                    duration_ms INTEGER,
                    ttfb_ms INTEGER,
                    project_id TEXT,
                    state TEXT NOT NULL,
                    error TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_flows_ts ON flows(ts);
                CREATE INDEX IF NOT EXISTS idx_flows_host ON flows(host);
                CREATE INDEX IF NOT EXISTS idx_flows_status ON flows(status);
                CREATE TABLE IF NOT EXISTS reports (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    created_ts INTEGER NOT NULL,
                    filter_json TEXT NOT NULL,
                    snapshot_json TEXT NOT NULL
                );",
            )
            .context("init schema")?;
        Ok(())
    }

    /// Insert or update a flow row (keyed by id; response phase overwrites request phase).
    pub fn record(&self, f: &FlowRow) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO flows
                (id, ts, method, scheme, host, port, path, status, req_size, resp_size, duration_ms, ttfb_ms, project_id, state, error)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                f.id as i64, f.ts as i64, f.method, f.scheme, f.host, f.port as i64, f.path,
                f.status.map(|v| v as i64), f.req_size as i64, f.resp_size as i64,
                f.duration_ms.map(|v| v as i64), f.ttfb_ms.map(|v| v as i64),
                f.project_id, f.state, f.error,
            ],
        )?;
        Ok(())
    }

    pub fn query(&self, q: &FlowQuery, limit: u32, offset: u32) -> Result<Vec<FlowRow>> {
        let (where_sql, mut params) = build_where(q);
        let sql = format!("SELECT * FROM flows{where_sql} ORDER BY ts DESC, id DESC LIMIT ? OFFSET ?");
        params.push(SqlValue::Integer(limit as i64));
        params.push(SqlValue::Integer(offset as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), row_to_flow)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn count(&self, q: &FlowQuery) -> Result<u64> {
        let (where_sql, params) = build_where(q);
        let sql = format!("SELECT COUNT(*) FROM flows{where_sql}");
        let mut stmt = self.conn.prepare(&sql)?;
        let n: i64 = stmt.query_row(params_from_iter(params.iter()), |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Aggregate by `group_by`: "host" | "status" | "time" | "duration".
    /// `bucket` is the bucket width (ms) for "time"/"duration".
    pub fn aggregate(
        &self,
        q: &FlowQuery,
        group_by: &str,
        bucket: u64,
        limit: u32,
    ) -> Result<Vec<AggBucket>> {
        let b = bucket.max(1) as i64;
        let (mut where_sql, mut params) = build_where(q);
        let (key_expr, order) = match group_by {
            "status" => (
                "CASE WHEN status IS NULL THEN 'none' ELSE (status/100) || 'xx' END".to_string(),
                "c DESC",
            ),
            "time" => (format!("(ts/{b})*{b}"), "k ASC"),
            "duration" => {
                where_sql = if where_sql.is_empty() {
                    " WHERE duration_ms IS NOT NULL".into()
                } else {
                    format!("{where_sql} AND duration_ms IS NOT NULL")
                };
                (format!("(duration_ms/{b})*{b}"), "k ASC")
            }
            _ => ("host".to_string(), "c DESC"),
        };
        let sql = format!(
            "SELECT {key_expr} AS k, COUNT(*) AS c, AVG(duration_ms) AS d FROM flows{where_sql} GROUP BY k ORDER BY {order} LIMIT ?"
        );
        params.push(SqlValue::Integer(limit as i64));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), |r| {
            let key = match r.get_ref("k")? {
                rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                rusqlite::types::ValueRef::Real(f) => f.to_string(),
                _ => "none".to_string(),
            };
            Ok(AggBucket {
                key,
                count: r.get::<_, i64>("c")? as u64,
                avg_duration_ms: r.get::<_, Option<f64>>("d")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Delete all but the newest `max` flows. Returns rows removed.
    pub fn prune(&self, max: u64) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM flows WHERE id IN (SELECT id FROM flows ORDER BY ts DESC, id DESC LIMIT -1 OFFSET ?)",
            params![max as i64],
        )?;
        Ok(n)
    }

    pub fn save_report(&self, r: &Report) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO reports (id, name, created_ts, filter_json, snapshot_json) VALUES (?1,?2,?3,?4,?5)",
            params![r.id, r.name, r.created_ts as i64, r.filter_json, r.snapshot_json],
        )?;
        Ok(())
    }

    pub fn list_reports(&self) -> Result<Vec<Report>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_ts, filter_json, snapshot_json FROM reports ORDER BY created_ts DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Report {
                id: r.get("id")?,
                name: r.get("name")?,
                created_ts: r.get::<_, i64>("created_ts")? as u64,
                filter_json: r.get("filter_json")?,
                snapshot_json: r.get("snapshot_json")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete_report(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM reports WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ── Async write path (actor thread) ──

enum WriteMsg {
    Record(Box<FlowRow>),
}

/// Cloneable, Send+Sync handle used by the proxy to persist flows without blocking.
#[derive(Clone)]
pub struct DbHandle {
    tx: tokio::sync::mpsc::UnboundedSender<WriteMsg>,
    path: PathBuf,
}

impl DbHandle {
    pub fn open(path: PathBuf) -> Result<DbHandle> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        // Initialize schema up front so readers see the tables immediately.
        Db::open(&path)?.init_schema()?;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<WriteMsg>();
        let writer_path = path.clone();
        std::thread::Builder::new()
            .name("db-writer".into())
            .spawn(move || {
                let db = match Db::open(&writer_path) {
                    Ok(d) => d,
                    Err(_) => return,
                };
                let mut since_prune: u64 = 0;
                while let Some(msg) = rx.blocking_recv() {
                    match msg {
                        WriteMsg::Record(row) => {
                            let _ = db.record(&row);
                            since_prune += 1;
                            if since_prune >= PRUNE_INTERVAL {
                                since_prune = 0;
                                let _ = db.prune(MAX_FLOWS);
                            }
                        }
                    }
                }
            })
            .context("spawn db-writer")?;
        Ok(DbHandle { tx, path })
    }

    /// Fire-and-forget persist of a captured/updated flow.
    pub fn record(&self, flow: &Flow, project_id: Option<&str>) {
        let row = FlowRow::from_flow(flow, project_id);
        let _ = self.tx.send(WriteMsg::Record(Box::new(row)));
    }

    /// Open a fresh read connection for a query command.
    pub fn reader(&self) -> Result<Db> {
        Db::open(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> Db {
        let path = std::env::temp_dir().join(format!(
            "trawl-db-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        db.init_schema().unwrap();
        db
    }

    fn row(id: u64, host: &str, method: &str, status: Option<u16>, ts: u64, dur: Option<u64>) -> FlowRow {
        FlowRow {
            id,
            ts,
            method: method.into(),
            scheme: "https".into(),
            host: host.into(),
            port: 443,
            path: "/api".into(),
            status,
            req_size: 0,
            resp_size: 0,
            duration_ms: dur,
            ttfb_ms: None,
            project_id: None,
            state: "completed".into(),
            error: None,
        }
    }

    #[test]
    fn record_and_query_roundtrip() {
        let db = tmp_db();
        db.record(&row(1, "a.com", "GET", Some(200), 100, Some(10))).unwrap();
        db.record(&row(2, "b.com", "POST", Some(404), 200, Some(50))).unwrap();
        db.record(&row(3, "a.com", "GET", Some(500), 300, Some(90))).unwrap();

        let all = db.query(&FlowQuery::default(), 100, 0).unwrap();
        assert_eq!(all.len(), 3);
        // newest first (ts DESC)
        assert_eq!(all[0].id, 3);

        let q = FlowQuery { method: "GET".into(), ..Default::default() };
        assert_eq!(db.query(&q, 100, 0).unwrap().len(), 2);

        let q = FlowQuery { status_class: "4xx".into(), ..Default::default() };
        assert_eq!(db.query(&q, 100, 0).unwrap()[0].id, 2);

        let q = FlowQuery { query: "b.com".into(), ..Default::default() };
        assert_eq!(db.query(&q, 100, 0).unwrap().len(), 1);

        assert_eq!(db.count(&FlowQuery::default()).unwrap(), 3);
    }

    #[test]
    fn insert_or_replace_updates_row() {
        let db = tmp_db();
        db.record(&row(1, "a.com", "GET", None, 100, None)).unwrap();
        db.record(&row(1, "a.com", "GET", Some(200), 100, Some(10))).unwrap();
        let all = db.query(&FlowQuery::default(), 100, 0).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, Some(200));
    }

    #[test]
    fn aggregate_by_host_and_status() {
        let db = tmp_db();
        db.record(&row(1, "a.com", "GET", Some(200), 100, Some(10))).unwrap();
        db.record(&row(2, "a.com", "GET", Some(200), 200, Some(30))).unwrap();
        db.record(&row(3, "b.com", "GET", Some(500), 300, Some(50))).unwrap();

        let by_host = db.aggregate(&FlowQuery::default(), "host", 0, 10).unwrap();
        assert_eq!(by_host[0].key, "a.com");
        assert_eq!(by_host[0].count, 2);
        assert_eq!(by_host[0].avg_duration_ms, Some(20.0));

        let by_status = db.aggregate(&FlowQuery::default(), "status", 0, 10).unwrap();
        let keys: Vec<_> = by_status.iter().map(|b| b.key.clone()).collect();
        assert!(keys.contains(&"2xx".to_string()));
        assert!(keys.contains(&"5xx".to_string()));
    }

    #[test]
    fn aggregate_time_and_duration_buckets() {
        let db = tmp_db();
        db.record(&row(1, "a.com", "GET", Some(200), 1000, Some(5))).unwrap();
        db.record(&row(2, "a.com", "GET", Some(200), 1500, Some(15))).unwrap();
        db.record(&row(3, "a.com", "GET", Some(200), 5000, Some(120))).unwrap();

        let by_time = db.aggregate(&FlowQuery::default(), "time", 1000, 100).unwrap();
        // ts 1000 & 1500 fall in bucket 1000; 5000 in bucket 5000
        let bucket_1000 = by_time.iter().find(|b| b.key == "1000").unwrap();
        assert_eq!(bucket_1000.count, 2);

        let by_dur = db.aggregate(&FlowQuery::default(), "duration", 10, 100).unwrap();
        // durations 5,15,120 → buckets 0,10,120
        assert_eq!(by_dur.len(), 3);
    }

    #[test]
    fn prune_keeps_newest() {
        let db = tmp_db();
        for i in 1..=10 {
            db.record(&row(i, "a.com", "GET", Some(200), i * 100, Some(1))).unwrap();
        }
        let removed = db.prune(3).unwrap();
        assert_eq!(removed, 7);
        let rest = db.query(&FlowQuery::default(), 100, 0).unwrap();
        assert_eq!(rest.len(), 3);
        assert_eq!(rest[0].id, 10);
    }

    #[test]
    fn reports_crud() {
        let db = tmp_db();
        let r = Report {
            id: "r1".into(),
            name: "Daily".into(),
            created_ts: 123,
            filter_json: "{}".into(),
            snapshot_json: "[]".into(),
        };
        db.save_report(&r).unwrap();
        assert_eq!(db.list_reports().unwrap().len(), 1);
        assert_eq!(db.list_reports().unwrap()[0].name, "Daily");
        db.delete_report("r1").unwrap();
        assert!(db.list_reports().unwrap().is_empty());
    }
}
