//! Thread-safe SQLite repository (`LogManager`) for recovery events and command executions.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{params, Connection};
use thiserror::Error;

use crate::executor::{redact_sensitive, CommandResult, CommandStatus};

use super::migrations::run_migrations;
use super::models::{
    CommandExecutionRecord, NewRecoveryEvent, RecoveryEventRecord, RecoveryEventStatus,
    RecoveryEventUpdate, RetentionPolicy,
};
use super::retention::prune_old_records;

pub const LOG_DB_FILENAME: &str = "desktop-host-logs.db";

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Failed to create database directory '{path}': {source}")]
    DirectoryCreation {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Database mutex lock was poisoned")]
    LockPoisoned,
}

/// Manages local SQLite persistence for recovery events and command executions.
#[derive(Clone)]
pub struct LogManager {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for LogManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogManager").finish_non_exhaustive()
    }
}

impl LogManager {
    /// Opens (or creates) the SQLite database inside `app_data_dir` and runs migrations.
    pub fn open_in_dir(app_data_dir: &Path) -> Result<Self, DatabaseError> {
        std::fs::create_dir_all(app_data_dir).map_err(|source| DatabaseError::DirectoryCreation {
            path: app_data_dir.display().to_string(),
            source,
        })?;

        let db_path = app_data_dir.join(LOG_DB_FILENAME);
        let conn = Connection::open(db_path)?;
        run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory SQLite database (intended for unit/integration tests).
    pub fn open_in_memory() -> Result<Self, DatabaseError> {
        let conn = Connection::open_in_memory()?;
        run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Inserts a new recovery event with redacted `trigger_details` and returns its row ID.
    pub fn create_recovery_event(&self, event: NewRecoveryEvent) -> Result<i64, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let now = Utc::now().to_rfc3339();
        let redacted_details = redact_sensitive(&event.trigger_details);

        conn.execute(
            "INSERT INTO recovery_events (
                event_type, rule_id, service_id, trigger_type, trigger_details,
                status, attempt_count, webview_reloaded, started_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, NULL)",
            params![
                event.event_type,
                event.rule_id,
                event.service_id,
                event.trigger_type,
                redacted_details,
                event.status.as_str(),
                event.attempt_count,
                now,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Updates an existing recovery event upon completion or attempt progression.
    pub fn update_recovery_event(
        &self,
        event_id: i64,
        update: RecoveryEventUpdate,
    ) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let finished_at = match update.status {
            RecoveryEventStatus::InProgress => None,
            _ => Some(Utc::now().to_rfc3339()),
        };

        conn.execute(
            "UPDATE recovery_events
             SET status = ?1,
                 attempt_count = ?2,
                 webview_reloaded = ?3,
                 finished_at = ?4
             WHERE id = ?5",
            params![
                update.status.as_str(),
                update.attempt_count,
                i32::from(update.webview_reloaded),
                finished_at,
                event_id,
            ],
        )?;

        Ok(())
    }

    /// Records a command execution linked to `recovery_event_id` with sensitive output redacted.
    pub fn record_command_execution(
        &self,
        recovery_event_id: i64,
        result: &CommandResult,
    ) -> Result<i64, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let now = Utc::now().to_rfc3339();
        let args_json = serde_json::to_string(&result.args).unwrap_or_else(|_| "[]".to_string());
        let stdout_clean = redact_sensitive(&result.stdout);
        let stderr_clean = redact_sensitive(&result.stderr);
        let status_str = command_status_str(result.status);

        conn.execute(
            "INSERT INTO command_executions (
                recovery_event_id, executable, arguments, stdout, stderr,
                exit_code, status, duration_ms, executed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                recovery_event_id,
                result.executable,
                args_json,
                stdout_clean,
                stderr_clean,
                result.exit_code,
                status_str,
                result.duration_ms as i64,
                now,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Retrieves the most recent recovery events ordered newest-first.
    pub fn query_recent_events(&self, limit: usize) -> Result<Vec<RecoveryEventRecord>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, event_type, rule_id, service_id, trigger_type, trigger_details,
                    status, attempt_count, webview_reloaded, started_at, finished_at
             FROM recovery_events
             ORDER BY id DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let status_raw: String = row.get(6)?;
            let reloaded_int: i32 = row.get(8)?;
            Ok(RecoveryEventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                rule_id: row.get(2)?,
                service_id: row.get(3)?,
                trigger_type: row.get(4)?,
                trigger_details: row.get(5)?,
                status: RecoveryEventStatus::from_db(&status_raw),
                attempt_count: row.get(7)?,
                webview_reloaded: reloaded_int != 0,
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(DatabaseError::from)
    }

    /// Retrieves all command executions linked to `recovery_event_id` in execution order.
    pub fn query_executions_for_event(
        &self,
        recovery_event_id: i64,
    ) -> Result<Vec<CommandExecutionRecord>, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT id, recovery_event_id, executable, arguments, stdout, stderr,
                    exit_code, status, duration_ms, executed_at
             FROM command_executions
             WHERE recovery_event_id = ?1
             ORDER BY id ASC",
        )?;

        let rows = stmt.query_map(params![recovery_event_id], |row| {
            let args_raw: String = row.get(3)?;
            let arguments: Vec<String> = serde_json::from_str(&args_raw).unwrap_or_default();
            let duration_raw: i64 = row.get(8)?;
            Ok(CommandExecutionRecord {
                id: row.get(0)?,
                recovery_event_id: row.get(1)?,
                executable: row.get(2)?,
                arguments,
                stdout: row.get(4)?,
                stderr: row.get(5)?,
                exit_code: row.get(6)?,
                status: row.get(7)?,
                duration_ms: duration_raw.max(0) as u64,
                executed_at: row.get(9)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(DatabaseError::from)
    }

    /// Applies the retention policy to prune expired or excess records.
    pub fn apply_retention(&self, policy: &RetentionPolicy) -> Result<usize, DatabaseError> {
        let conn = self.conn.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        Ok(prune_old_records(&conn, policy, Utc::now())?)
    }
}

fn command_status_str(status: CommandStatus) -> &'static str {
    match status {
        CommandStatus::Succeeded => "succeeded",
        CommandStatus::Failed => "failed",
        CommandStatus::TimedOut => "timed_out",
        CommandStatus::SpawnError => "spawn_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn opens_in_dir_and_runs_idempotent_migrations() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("desktop-host-db-{nanos}"));

        let manager1 = LogManager::open_in_dir(&dir).expect("first open");
        drop(manager1);
        let manager2 = LogManager::open_in_dir(&dir).expect("idempotent second open");
        assert!(manager2.query_recent_events(10).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persists_events_and_linked_executions_with_redaction() {
        let log = LogManager::open_in_memory().expect("in-memory db");

        let event_id = log
            .create_recovery_event(NewRecoveryEvent {
                event_type: "recovery".to_string(),
                rule_id: Some("sales-container-stopped".to_string()),
                service_id: Some("sales-app".to_string()),
                trigger_type: "service_status".to_string(),
                trigger_details: "Bearer secret-jwt-123".to_string(),
                status: RecoveryEventStatus::InProgress,
                attempt_count: 1,
            })
            .expect("create event");

        let cmd_res = CommandResult {
            executable: "docker".to_string(),
            args: vec!["compose".to_string(), "restart".to_string(), "app".to_string()],
            stdout: "DB_PASSWORD=topsecret restarted".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            status: CommandStatus::Succeeded,
            success: true,
            timed_out: false,
            duration_ms: 412,
            error_message: None,
        };

        log.record_command_execution(event_id, &cmd_res)
            .expect("record execution");

        log.update_recovery_event(
            event_id,
            RecoveryEventUpdate {
                status: RecoveryEventStatus::Succeeded,
                attempt_count: 1,
                webview_reloaded: true,
            },
        )
        .expect("update event");

        let events = log.query_recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, RecoveryEventStatus::Succeeded);
        assert!(events[0].webview_reloaded);
        assert!(events[0].finished_at.is_some());
        assert!(!events[0].trigger_details.contains("secret-jwt-123"));
        assert!(events[0].trigger_details.contains("Bearer [REDACTED]"));

        let execs = log.query_executions_for_event(event_id).unwrap();
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].executable, "docker");
        assert_eq!(execs[0].duration_ms, 412);
        assert_eq!(execs[0].exit_code, Some(0));
        assert!(!execs[0].stdout.contains("topsecret"));
        assert!(execs[0].stdout.contains("DB_PASSWORD=[REDACTED]"));
    }

    #[test]
    fn retention_policy_prunes_expired_events_and_cascades_executions() {
        let log = LogManager::open_in_memory().expect("in-memory db");

        let old_id = log
            .create_recovery_event(NewRecoveryEvent {
                event_type: "recovery".to_string(),
                rule_id: Some("r1".to_string()),
                service_id: Some("sales-app".to_string()),
                trigger_type: "tcp_unavailable".to_string(),
                trigger_details: "old".to_string(),
                status: RecoveryEventStatus::Failed,
                attempt_count: 3,
            })
            .unwrap();

        log.record_command_execution(
            old_id,
            &CommandResult {
                executable: "docker".to_string(),
                args: vec![],
                stdout: String::new(),
                stderr: "err".to_string(),
                exit_code: Some(1),
                status: CommandStatus::Failed,
                success: false,
                timed_out: false,
                duration_ms: 50,
                error_message: None,
            },
        )
        .unwrap();

        // Backdate the old event by 45 days.
        {
            let conn = log.conn.lock().unwrap();
            let old_ts = (Utc::now() - Duration::days(45)).to_rfc3339();
            conn.execute(
                "UPDATE recovery_events SET started_at = ?1 WHERE id = ?2",
                params![old_ts, old_id],
            )
            .unwrap();
        }

        let recent_id = log
            .create_recovery_event(NewRecoveryEvent {
                event_type: "recovery".to_string(),
                rule_id: Some("r2".to_string()),
                service_id: Some("sales-app".to_string()),
                trigger_type: "http_status".to_string(),
                trigger_details: "recent".to_string(),
                status: RecoveryEventStatus::Succeeded,
                attempt_count: 1,
            })
            .unwrap();

        let deleted = log
            .apply_retention(&RetentionPolicy {
                max_age_days: 30,
                max_events: 100,
            })
            .unwrap();

        assert_eq!(deleted, 1);
        let remaining = log.query_recent_events(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, recent_id);
        // Foreign-key cascade must remove the child command_executions row.
        assert!(log.query_executions_for_event(old_id).unwrap().is_empty());
    }
}
