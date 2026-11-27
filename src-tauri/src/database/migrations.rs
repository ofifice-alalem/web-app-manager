//! Idempotent SQLite schema migrations and connection pragmas.

use rusqlite::Connection;

/// Configures SQLite pragmas and creates required tables and indexes if missing.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS recovery_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type TEXT NOT NULL,
            rule_id TEXT,
            service_id TEXT,
            trigger_type TEXT NOT NULL,
            trigger_details TEXT NOT NULL,
            status TEXT NOT NULL,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            webview_reloaded INTEGER NOT NULL DEFAULT 0,
            started_at TEXT NOT NULL,
            finished_at TEXT
        );

        CREATE TABLE IF NOT EXISTS command_executions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            recovery_event_id INTEGER NOT NULL,
            executable TEXT NOT NULL,
            arguments TEXT NOT NULL,
            stdout TEXT NOT NULL,
            stderr TEXT NOT NULL,
            exit_code INTEGER,
            status TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            executed_at TEXT NOT NULL,
            FOREIGN KEY (recovery_event_id) REFERENCES recovery_events(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_recovery_events_started_at
            ON recovery_events(started_at);
        CREATE INDEX IF NOT EXISTS idx_recovery_events_service_id
            ON recovery_events(service_id);
        CREATE INDEX IF NOT EXISTS idx_command_executions_event_id
            ON command_executions(recovery_event_id);
        ",
    )?;
    Ok(())
}
