//! SQLite storage: schema (ported from the Laravel migrations), migrations and JSON query helpers.
//!
//! One connection lives behind a mutex in managed state (`Db`, cheap to clone). Every query helper is
//! available on a locked connection or transaction (trait [`Query`]) and, for one-off calls, on `Db` itself:
//!
//! ```ignore
//! let papers = ctx.db.rows("SELECT * FROM papers WHERE user_id = ?1 ORDER BY updated_at DESC", [USER_ID])?;
//! let id = ctx.db.insert("deadlines", &json!({ "user_id": USER_ID, "title": "Camera ready", "due_on": "2026-12-01" }))?;
//! ctx.db.update("deadlines", id, &json!({ "done": true }))?;
//! ctx.db.tx(|tx| { tx.exec("UPDATE submissions SET is_current = 0 WHERE paper_id = ?1", [7])?; Ok(()) })?;
//! ```

use crate::error::{ApiError, Result};
use crate::util::now_ts;
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OptionalExtension, Params, Row, Transaction};
use serde_json::{Map, Number, Value};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// The single local user.
pub const USER_ID: i64 = 1;

/// Columns returned as JSON booleans when the column has no declared type (e.g. `SELECT s.is_current AS cur`
/// keeps its type, but expressions do not). Declared `BOOLEAN` columns are always converted.
const BOOL_NAMES: &[&str] = &["is_current", "corresponding", "deadline_expected", "watch", "encrypted", "is_admin"];
/// Columns parsed as JSON when the column has no declared type. Declared `JSON` columns are always parsed.
const JSON_NAMES: &[&str] = &["roles", "tags", "sources", "scholar_stats"];

/// Schema versions, applied in order and tracked with `PRAGMA user_version`.
/// To change the schema, APPEND a new entry (never edit an old one).
pub const MIGRATIONS: &[&str] = &[SCHEMA_V1, crate::research::MIGRATION, AVATAR, STEPS, STEPS_BACKFILL];

/// Profile photo: `avatars/<name>` relative to the app data folder (see `files::save_avatar`).
const AVATAR: &str = "ALTER TABLE users ADD COLUMN avatar_path TEXT NULL;";

/// Statuses that mean the paper is still with the journal (`Submission::WAITING`), as an SQL list.
macro_rules! waiting_sql {
    () => {
        "('submitted', 'with_editor', 'pending', 'awaiting_decision', 'returned')"
    };
}

/// Status history: one row each time a submission's status changes, kept by triggers so every path
/// (paper page, Desk, MCP tools, backup import) records it. Switching back within ten minutes undoes the step.
const STEPS: &str = concat!(
    r#"
CREATE TABLE submission_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER NOT NULL REFERENCES submissions (id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    on_date TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX submission_steps_submission_id_index ON submission_steps (submission_id);

CREATE TRIGGER submission_steps_on_insert AFTER INSERT ON submissions BEGIN
    INSERT INTO submission_steps (submission_id, status, on_date, created_at, updated_at)
        VALUES (NEW.id, 'submitted', NEW.submitted_on, datetime('now'), datetime('now'));
    INSERT INTO submission_steps (submission_id, status, on_date, created_at, updated_at)
        SELECT NEW.id, NEW.decision,
               CASE WHEN NEW.decision IN "#,
    waiting_sql!(),
    r#" THEN NEW.status_on ELSE COALESCE(NEW.decision_on, NEW.status_on) END,
               datetime('now'), datetime('now')
        WHERE NEW.decision <> 'submitted';
END;

CREATE TRIGGER submission_steps_on_status AFTER UPDATE OF decision ON submissions
WHEN NEW.decision IS NOT OLD.decision BEGIN
    DELETE FROM submission_steps
     WHERE id = (SELECT MAX(id) FROM submission_steps WHERE submission_id = NEW.id)
       AND created_at >= datetime('now', '-10 minutes')
       AND (SELECT status FROM submission_steps WHERE submission_id = NEW.id ORDER BY id DESC LIMIT 1 OFFSET 1) = NEW.decision;
    INSERT INTO submission_steps (submission_id, status, on_date, created_at, updated_at)
        SELECT NEW.id, NEW.decision,
               CASE
                   WHEN NEW.decision NOT IN "#,
    waiting_sql!(),
    r#" AND NEW.decision_on IS NOT NULL AND NEW.decision_on IS NOT OLD.decision_on THEN NEW.decision_on
                   WHEN NEW.status_on IS NOT NULL AND NEW.status_on IS NOT OLD.status_on THEN NEW.status_on
                   ELSE date('now', 'localtime')
               END,
               datetime('now'), datetime('now')
        WHERE (SELECT status FROM submission_steps WHERE submission_id = NEW.id ORDER BY id DESC LIMIT 1) IS NOT NEW.decision;
END;

CREATE TRIGGER submission_steps_on_dates AFTER UPDATE OF submitted_on, status_on, decision_on ON submissions
WHEN NEW.decision IS OLD.decision BEGIN
    UPDATE submission_steps SET on_date = NEW.submitted_on, updated_at = datetime('now')
     WHERE NEW.submitted_on IS NOT OLD.submitted_on
       AND id = (SELECT MIN(id) FROM submission_steps WHERE submission_id = NEW.id)
       AND status = 'submitted';
    UPDATE submission_steps
       SET on_date = CASE WHEN NEW.decision IN "#,
    waiting_sql!(),
    r#" THEN NEW.status_on ELSE NEW.decision_on END, updated_at = datetime('now')
     WHERE id = (SELECT MAX(id) FROM submission_steps WHERE submission_id = NEW.id)
       AND id > (SELECT MIN(id) FROM submission_steps WHERE submission_id = NEW.id)
       AND status = NEW.decision
       AND CASE WHEN NEW.decision IN "#,
    waiting_sql!(),
    r#"
                THEN NEW.status_on IS NOT NULL AND NEW.status_on IS NOT OLD.status_on
                ELSE NEW.decision_on IS NOT NULL AND NEW.decision_on IS NOT OLD.decision_on END;
END;
"#
);

/// Starts the history of submissions made before it existed: "Submitted", then the current status.
pub const STEPS_BACKFILL: &str = concat!(
    r#"
INSERT INTO submission_steps (submission_id, status, on_date, created_at, updated_at)
    SELECT id, 'submitted', submitted_on, COALESCE(created_at, datetime('now')), datetime('now') FROM submissions ORDER BY id;
INSERT INTO submission_steps (submission_id, status, on_date, created_at, updated_at)
    SELECT id, decision, CASE WHEN decision IN "#,
    waiting_sql!(),
    r#" THEN status_on ELSE COALESCE(decision_on, status_on) END,
           COALESCE(created_at, datetime('now')), datetime('now')
    FROM submissions WHERE decision <> 'submitted' ORDER BY id;
"#
);

const SCHEMA_V1: &str = r#"
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT NULL UNIQUE,
    is_admin BOOLEAN NOT NULL DEFAULT 0,
    email_verified_at TEXT NULL,
    password TEXT NULL,
    remember_token TEXT NULL,
    calendar_token TEXT NULL UNIQUE,
    mcp_token TEXT NULL UNIQUE,
    interests TEXT NULL,
    scholar_url TEXT NULL,
    scholar_stats JSON NULL,
    scholar_checked_at TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);

CREATE TABLE cache (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    expiration INTEGER NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NULL,
    encrypted BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);

CREATE TABLE notifications (
    id TEXT PRIMARY KEY NOT NULL,
    type TEXT NOT NULL,
    notifiable_type TEXT NOT NULL,
    notifiable_id INTEGER NOT NULL,
    data JSON NOT NULL,
    read_at TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX notifications_notifiable_type_notifiable_id_index ON notifications (notifiable_type, notifiable_id);
CREATE INDEX notifications_read_at_index ON notifications (read_at);

CREATE TABLE venues (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'journal',
    field TEXT NULL,
    publisher TEXT NULL,
    rank TEXT NULL,
    quartile TEXT NULL,
    acceptance_rate TEXT NULL,
    first_decision TEXT NULL,
    proceedings TEXT NULL,
    next_edition TEXT NULL,
    deadline_on TEXT NULL,
    deadline_label TEXT NULL,
    deadline_expected BOOLEAN NOT NULL DEFAULT 0,
    review_type TEXT NOT NULL DEFAULT 'double',
    word_limit TEXT NULL,
    decision_days INTEGER NULL,
    fee TEXT NULL,
    preprint_policy TEXT NOT NULL DEFAULT 'unknown',
    portal_url TEXT NULL,
    website_url TEXT NULL,
    watch BOOLEAN NOT NULL DEFAULT 0,
    checked_at TEXT NULL,
    sources JSON NULL,
    notes TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX venues_user_id_index ON venues (user_id);

CREATE TABLE papers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    short_title TEXT NULL,
    stage TEXT NOT NULL DEFAULT 'drafting',
    started_on TEXT NULL,
    tags JSON NULL,
    abstract TEXT NULL,
    method TEXT NULL,
    dataset TEXT NULL,
    keywords TEXT NULL,
    overleaf_url TEXT NULL,
    drive_url TEXT NULL,
    preprint_server TEXT NULL,
    preprint_url TEXT NULL,
    preprint_on TEXT NULL,
    notes TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX papers_user_id_index ON papers (user_id);

CREATE TABLE submissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id INTEGER NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    venue_id INTEGER NULL REFERENCES venues (id) ON DELETE SET NULL,
    venue_name TEXT NOT NULL,
    manuscript_id TEXT NULL,
    portal_url TEXT NULL,
    submitted_on TEXT NULL,
    status_on TEXT NULL,
    decision TEXT NOT NULL DEFAULT 'pending',
    is_current BOOLEAN NOT NULL DEFAULT 0,
    decision_on TEXT NULL,
    revision_due_on TEXT NULL,
    note TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX submissions_paper_id_index ON submissions (paper_id);
CREATE INDEX submissions_venue_id_index ON submissions (venue_id);

CREATE TABLE queue_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id INTEGER NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    venue_id INTEGER NOT NULL REFERENCES venues (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE UNIQUE INDEX queue_items_paper_id_venue_id_unique ON queue_items (paper_id, venue_id);
CREATE INDEX queue_items_venue_id_index ON queue_items (venue_id);

CREATE TABLE authors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id INTEGER NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    affiliation TEXT NULL,
    email TEXT NULL,
    orcid TEXT NULL,
    corresponding BOOLEAN NOT NULL DEFAULT 0,
    roles JSON NULL,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX authors_paper_id_index ON authors (paper_id);

CREATE TABLE deadlines (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    paper_id INTEGER NULL REFERENCES papers (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    due_on TEXT NOT NULL,
    done BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX deadlines_user_id_index ON deadlines (user_id);
CREATE INDEX deadlines_paper_id_index ON deadlines (paper_id);

CREATE TABLE revision_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER NOT NULL REFERENCES submissions (id) ON DELETE CASCADE,
    comment TEXT NOT NULL,
    response TEXT NULL,
    done BOOLEAN NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX revision_items_submission_id_index ON revision_items (submission_id);

CREATE TABLE paper_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id INTEGER NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    submission_id INTEGER NULL REFERENCES submissions (id) ON DELETE SET NULL,
    kind TEXT NOT NULL DEFAULT 'manuscript',
    label TEXT NULL,
    path TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime TEXT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX paper_files_paper_id_index ON paper_files (paper_id);
CREATE INDEX paper_files_submission_id_index ON paper_files (submission_id);

CREATE TABLE scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'running',
    "trigger" TEXT NOT NULL DEFAULT 'manual',
    candidates INTEGER NOT NULL DEFAULT 0,
    found INTEGER NOT NULL DEFAULT 0,
    error TEXT NULL,
    finished_at TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL,
    provider TEXT NULL,
    log TEXT NULL
);
CREATE INDEX scans_user_id_index ON scans (user_id);

CREATE TABLE opportunities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    scan_id INTEGER NULL REFERENCES scans (id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'conference',
    venue TEXT NULL,
    publisher TEXT NULL,
    outlet TEXT NULL,
    deadline_on TEXT NULL,
    event TEXT NULL,
    url TEXT NOT NULL,
    url_hash TEXT NOT NULL,
    summary TEXT NULL,
    why TEXT NULL,
    fit INTEGER NOT NULL DEFAULT 3,
    status TEXT NOT NULL DEFAULT 'new',
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE UNIQUE INDEX opportunities_user_id_url_hash_unique ON opportunities (user_id, url_hash);
CREATE INDEX opportunities_scan_id_index ON opportunities (scan_id);
CREATE INDEX opportunities_status_created_at_index ON opportunities (status, created_at);

CREATE TABLE publications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    paper_id INTEGER NULL REFERENCES papers (id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    authors TEXT NULL,
    kind TEXT NOT NULL DEFAULT 'article',
    venue TEXT NULL,
    volume TEXT NULL,
    issue TEXT NULL,
    pages TEXT NULL,
    year INTEGER NULL,
    publisher TEXT NULL,
    quartile TEXT NULL,
    sjr TEXT NULL,
    abs TEXT NULL,
    abdc TEXT NULL,
    citations INTEGER NULL,
    scholar_url TEXT NULL,
    cites_url TEXT NULL,
    doi TEXT NULL,
    url TEXT NULL,
    notes TEXT NULL,
    sources JSON NULL,
    checked_at TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL
);
CREATE INDEX publications_user_id_index ON publications (user_id);
CREATE INDEX publications_paper_id_index ON publications (paper_id);

CREATE TABLE ai_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    total INTEGER NOT NULL DEFAULT 0,
    done INTEGER NOT NULL DEFAULT 0,
    step TEXT NULL,
    url TEXT NULL,
    error TEXT NULL,
    finished_at TEXT NULL,
    created_at TEXT NULL,
    updated_at TEXT NULL,
    provider TEXT NULL,
    log TEXT NULL
);
CREATE INDEX ai_runs_user_id_status_index ON ai_runs (user_id, status);
"#;

/// Shared handle to the database (managed state). Clone freely.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

impl Db {
    /// Open (or create) the database file, apply pragmas and run migrations.
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// An in-memory database with the full schema (tests).
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -16000;",
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.set_prepared_statement_cache_capacity(128);
        migrate(&conn)?;
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    /// Lock the connection. Keep the guard short-lived (never hold it across network calls).
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `PRAGMA data_version`: changes when another connection (e.g. the `--mcp` server process) commits.
    pub fn data_version(&self) -> Result<i64> {
        Ok(self.conn().query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    /// Run a closure with the locked connection.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        f(&self.conn())
    }

    /// Run a closure in a transaction: committed on `Ok`, rolled back on `Err`.
    pub fn tx<T>(&self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn rows<P: Params>(&self, sql: &str, params: P) -> Result<Vec<Value>> {
        self.conn().rows(sql, params)
    }
    pub fn first<P: Params>(&self, sql: &str, params: P) -> Result<Option<Value>> {
        self.conn().first(sql, params)
    }
    pub fn scalar<T: rusqlite::types::FromSql, P: Params>(&self, sql: &str, params: P) -> Result<Option<T>> {
        self.conn().scalar(sql, params)
    }
    pub fn count<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        self.conn().count(sql, params)
    }
    pub fn exec<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        self.conn().exec(sql, params)
    }
    pub fn find(&self, table: &str, id: i64) -> Result<Option<Value>> {
        self.conn().find(table, id)
    }
    pub fn insert(&self, table: &str, data: &Value) -> Result<i64> {
        self.conn().insert(table, data)
    }
    pub fn update(&self, table: &str, id: i64, data: &Value) -> Result<usize> {
        self.conn().update(table, id, data)
    }
    pub fn delete(&self, table: &str, id: i64) -> Result<usize> {
        self.conn().delete(table, id)
    }
    pub fn cache_get(&self, key: &str) -> Result<Option<String>> {
        self.conn().cache_get(key)
    }
    pub fn cache_put(&self, key: &str, value: &str) -> Result<()> {
        self.conn().cache_put(key, value)
    }
}

/// Apply pending migrations (`PRAGMA user_version` = number applied).
pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version.max(0) as usize) {
        conn.execute_batch(&format!("BEGIN; {sql}\nPRAGMA user_version = {}; COMMIT;", i + 1)).inspect_err(|_| {
            let _ = conn.execute_batch("ROLLBACK;");
        })?;
    }
    Ok(())
}

/// Convert a JSON value to an SQLite value (booleans -> 0/1, arrays/objects -> JSON text).
pub fn sql_value(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => SqlValue::Integer(i),
            None => SqlValue::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Array(_) | Value::Object(_) => SqlValue::Text(v.to_string()),
    }
}

/// Bind a dynamic list of JSON values: `db.rows(sql, params_json(&[json!(1), json!("x")]))`.
pub fn params_json(values: &[Value]) -> rusqlite::ParamsFromIter<Vec<SqlValue>> {
    rusqlite::params_from_iter(values.iter().map(sql_value).collect::<Vec<_>>())
}

fn check_ident(name: &str) -> Result<()> {
    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        Ok(())
    } else {
        Err(ApiError::msg(format!("Invalid name: {name}")))
    }
}

fn row_to_json(row: &Row, cols: &[(String, Option<String>)]) -> rusqlite::Result<Value> {
    let mut obj = Map::with_capacity(cols.len());
    for (i, (name, decl)) in cols.iter().enumerate() {
        let decl = decl.as_deref().map(str::to_ascii_uppercase);
        let is_bool = match decl.as_deref() {
            Some(d) => d == "BOOLEAN",
            None => BOOL_NAMES.contains(&name.as_str()),
        };
        let is_json = match decl.as_deref() {
            Some(d) => d == "JSON",
            None => JSON_NAMES.contains(&name.as_str()),
        };
        let v = match row.get_ref(i)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) if is_bool => Value::Bool(n != 0),
            ValueRef::Integer(n) => Value::Number(n.into()),
            ValueRef::Real(f) => Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null),
            ValueRef::Text(t) => {
                let s = String::from_utf8_lossy(t);
                if is_json {
                    serde_json::from_str(&s).unwrap_or_else(|_| Value::String(s.into_owned()))
                } else {
                    Value::String(s.into_owned())
                }
            }
            ValueRef::Blob(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
        };
        obj.insert(name.clone(), v);
    }
    Ok(Value::Object(obj))
}

/// Query helpers on a connection or transaction (`Transaction` derefs to `Connection`).
pub trait Query {
    /// All rows as JSON objects (column order kept; `BOOLEAN` -> bool, `JSON` -> parsed).
    fn rows<P: Params>(&self, sql: &str, params: P) -> Result<Vec<Value>>;
    /// The first row, if any.
    fn first<P: Params>(&self, sql: &str, params: P) -> Result<Option<Value>>;
    /// The first column of the first row (`None` when there is no row or it is NULL).
    fn scalar<T: rusqlite::types::FromSql, P: Params>(&self, sql: &str, params: P) -> Result<Option<T>>;
    /// `SELECT COUNT(*) ...` helper.
    fn count<P: Params>(&self, sql: &str, params: P) -> Result<i64>;
    /// INSERT/UPDATE/DELETE; returns affected rows.
    fn exec<P: Params>(&self, sql: &str, params: P) -> Result<usize>;
    /// Column names of a table.
    fn columns(&self, table: &str) -> Result<Vec<String>>;
    /// `SELECT * FROM table WHERE id = ?`.
    fn find(&self, table: &str, id: i64) -> Result<Option<Value>>;
    /// Insert a JSON object; unknown columns are rejected; `created_at`/`updated_at` filled in. Returns the rowid.
    fn insert(&self, table: &str, data: &Value) -> Result<i64>;
    /// Update by id; `updated_at` set to now unless given. Returns affected rows.
    fn update(&self, table: &str, id: i64, data: &Value) -> Result<usize>;
    /// Update rows matching `where_sql` (e.g. `"paper_id = ?"`), binding `params` after the SET values.
    fn update_where(&self, table: &str, data: &Value, where_sql: &str, params: &[Value]) -> Result<usize>;
    /// Delete by id.
    fn delete(&self, table: &str, id: i64) -> Result<usize>;
    /// Read a key from the `cache` table (scheduler stamps and similar).
    fn cache_get(&self, key: &str) -> Result<Option<String>>;
    /// Write a key to the `cache` table forever.
    fn cache_put(&self, key: &str, value: &str) -> Result<()>;
}

impl Query for Connection {
    fn rows<P: Params>(&self, sql: &str, params: P) -> Result<Vec<Value>> {
        let mut stmt = self.prepare_cached(sql)?;
        let cols: Vec<(String, Option<String>)> =
            stmt.columns().iter().map(|c| (c.name().to_string(), c.decl_type().map(str::to_string))).collect();
        let mut out = Vec::new();
        let mut rows = stmt.query(params)?;
        while let Some(row) = rows.next()? {
            out.push(row_to_json(row, &cols)?);
        }
        Ok(out)
    }

    fn first<P: Params>(&self, sql: &str, params: P) -> Result<Option<Value>> {
        let mut stmt = self.prepare_cached(sql)?;
        let cols: Vec<(String, Option<String>)> =
            stmt.columns().iter().map(|c| (c.name().to_string(), c.decl_type().map(str::to_string))).collect();
        let mut rows = stmt.query(params)?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_json(row, &cols)?)),
            None => Ok(None),
        }
    }

    fn scalar<T: rusqlite::types::FromSql, P: Params>(&self, sql: &str, params: P) -> Result<Option<T>> {
        let mut stmt = self.prepare_cached(sql)?;
        Ok(stmt.query_row(params, |r| r.get::<_, Option<T>>(0)).optional()?.flatten())
    }

    fn count<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        Ok(self.scalar::<i64, P>(sql, params)?.unwrap_or(0))
    }

    fn exec<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        Ok(self.prepare_cached(sql)?.execute(params)?)
    }

    fn columns(&self, table: &str) -> Result<Vec<String>> {
        check_ident(table)?;
        let mut stmt = self.prepare_cached(&format!("PRAGMA table_info({table})"))?;
        let cols = stmt.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if cols.is_empty() {
            return Err(ApiError::msg(format!("Unknown table: {table}")));
        }
        Ok(cols)
    }

    fn find(&self, table: &str, id: i64) -> Result<Option<Value>> {
        check_ident(table)?;
        self.first(&format!("SELECT * FROM {table} WHERE id = ?1"), [id])
    }

    fn insert(&self, table: &str, data: &Value) -> Result<i64> {
        let obj = data.as_object().ok_or_else(|| ApiError::msg("insert expects an object"))?;
        let cols = self.columns(table)?;
        let mut names: Vec<String> = Vec::with_capacity(obj.len() + 2);
        let mut values: Vec<SqlValue> = Vec::with_capacity(obj.len() + 2);
        for (k, v) in obj {
            if !cols.iter().any(|c| c == k) {
                return Err(ApiError::msg(format!("Unknown column {table}.{k}")));
            }
            names.push(format!("\"{k}\""));
            values.push(sql_value(v));
        }
        let now = now_ts();
        for ts in ["created_at", "updated_at"] {
            if cols.iter().any(|c| c == ts) && !obj.contains_key(ts) {
                names.push(ts.to_string());
                values.push(SqlValue::Text(now.clone()));
            }
        }
        let marks = (1..=names.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
        let sql = format!("INSERT INTO {table} ({}) VALUES ({marks})", names.join(", "));
        self.prepare_cached(&sql)?.execute(rusqlite::params_from_iter(values))?;
        Ok(self.last_insert_rowid())
    }

    fn update(&self, table: &str, id: i64, data: &Value) -> Result<usize> {
        self.update_where(table, data, "id = ?", &[Value::from(id)])
    }

    fn update_where(&self, table: &str, data: &Value, where_sql: &str, params: &[Value]) -> Result<usize> {
        let obj = data.as_object().ok_or_else(|| ApiError::msg("update expects an object"))?;
        let cols = self.columns(table)?;
        let mut sets = Vec::with_capacity(obj.len() + 1);
        let mut values: Vec<SqlValue> = Vec::with_capacity(obj.len() + 1 + params.len());
        for (k, v) in obj {
            if !cols.iter().any(|c| c == k) {
                return Err(ApiError::msg(format!("Unknown column {table}.{k}")));
            }
            sets.push(format!("\"{k}\" = ?"));
            values.push(sql_value(v));
        }
        if cols.iter().any(|c| c == "updated_at") && !obj.contains_key("updated_at") {
            sets.push("updated_at = ?".into());
            values.push(SqlValue::Text(now_ts()));
        }
        if sets.is_empty() {
            return Ok(0);
        }
        values.extend(params.iter().map(sql_value));
        let sql = format!("UPDATE {table} SET {} WHERE {where_sql}", sets.join(", "));
        Ok(self.prepare_cached(&sql)?.execute(rusqlite::params_from_iter(values))?)
    }

    fn delete(&self, table: &str, id: i64) -> Result<usize> {
        check_ident(table)?;
        self.exec(&format!("DELETE FROM {table} WHERE id = ?1"), [id])
    }

    fn cache_get(&self, key: &str) -> Result<Option<String>> {
        self.scalar("SELECT value FROM cache WHERE key = ?1", [key])
    }

    fn cache_put(&self, key: &str, value: &str) -> Result<()> {
        self.exec(
            "INSERT INTO cache (key, value, expiration) VALUES (?1, ?2, 0)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, expiration = 0",
            [key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
pub use tests::seeded as test_db;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn seeded() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.insert("users", &json!({ "id": 1, "name": "Test", "is_admin": true })).unwrap();
        db
    }

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let v: i64 = db.scalar("PRAGMA user_version", []).unwrap().unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        migrate(&db.conn()).unwrap();
        for t in [
            "users",
            "settings",
            "notifications",
            "ai_runs",
            "scans",
            "opportunities",
            "publications",
            "venues",
            "papers",
            "submissions",
            "queue_items",
            "authors",
            "deadlines",
            "revision_items",
            "paper_files",
            "cache",
        ] {
            assert!(!db.conn().columns(t).unwrap().is_empty(), "{t}");
        }
        let fk: i64 = db.scalar("PRAGMA foreign_keys", []).unwrap().unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn insert_update_and_casts() {
        let db = seeded();
        let pid = db.insert("papers", &json!({ "user_id": 1, "title": "A", "tags": ["x", "y"] })).unwrap();
        let p = db.find("papers", pid).unwrap().unwrap();
        assert_eq!(p["stage"], "drafting");
        assert_eq!(p["tags"], json!(["x", "y"]));
        assert!(p["created_at"].as_str().unwrap().len() == 19);
        let sid = db.insert("submissions", &json!({ "paper_id": pid, "venue_name": "J", "is_current": true })).unwrap();
        let s = db.find("submissions", sid).unwrap().unwrap();
        assert_eq!(s["is_current"], json!(true));
        assert_eq!(s["decision"], "pending");
        db.update("submissions", sid, &json!({ "is_current": false })).unwrap();
        let s = db.first("SELECT is_current, is_current + 0 AS n FROM submissions WHERE id = ?1", [sid]).unwrap().unwrap();
        assert_eq!(s["is_current"], json!(false));
        assert!(db.insert("papers", &json!({ "user_id": 1, "title": "B", "nope": 1 })).is_err());
        // Cascade
        db.delete("papers", pid).unwrap();
        assert_eq!(db.count("SELECT COUNT(*) FROM submissions", []).unwrap(), 0);
    }

    #[test]
    fn transactions_roll_back() {
        let db = seeded();
        let r: Result<()> = db.tx(|tx| {
            tx.insert("papers", &json!({ "user_id": 1, "title": "A" }))?;
            Err(ApiError::msg("stop"))
        });
        assert!(r.is_err());
        assert_eq!(db.count("SELECT COUNT(*) FROM papers", []).unwrap(), 0);
        db.cache_put("k", "v").unwrap();
        db.cache_put("k", "w").unwrap();
        assert_eq!(db.cache_get("k").unwrap().as_deref(), Some("w"));
    }
}
