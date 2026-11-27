//! SQLite log retention and rotation policy enforcement.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use super::models::RetentionPolicy;

/// Prunes expired `recovery_events` (cascading to `command_executions`)
/// based on age and maximum event count. Returns total deleted event rows.
pub fn prune_old_records(
    conn: &Connection,
    policy: &RetentionPolicy,
    now_utc: DateTime<Utc>,
) -> Result<usize, rusqlite::Error> {
    let mut deleted = 0usize;

    if policy.max_age_days > 0 {
        let cutoff = (now_utc - Duration::days(i64::from(policy.max_age_days))).to_rfc3339();
        deleted += conn.execute(
            "DELETE FROM recovery_events WHERE started_at < ?1",
            params![cutoff],
        )?;
    }

    if policy.max_events > 0 {
        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM recovery_events", [], |row| row.get(0))?;
        let max_events_i64 = policy.max_events as i64;
        if total > max_events_i64 {
            let excess = total - max_events_i64;
            deleted += conn.execute(
                "DELETE FROM recovery_events WHERE id IN (
                    SELECT id FROM recovery_events ORDER BY started_at ASC, id ASC LIMIT ?1
                )",
                params![excess],
            )?;
        }
    }

    Ok(deleted)
}
