//! Data models for local SQLite recovery and command execution logging.

use serde::{Deserialize, Serialize};

/// Lifecycle status of a persisted recovery event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEventStatus {
    InProgress,
    Succeeded,
    Failed,
    SkippedCooldown,
}

impl RecoveryEventStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::SkippedCooldown => "skipped_cooldown",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "in_progress" => Self::InProgress,
            "succeeded" => Self::Succeeded,
            "skipped_cooldown" => Self::SkippedCooldown,
            _ => Self::Failed,
        }
    }
}

/// Input payload for creating a new `recovery_events` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRecoveryEvent {
    pub event_type: String,
    pub rule_id: Option<String>,
    pub service_id: Option<String>,
    pub trigger_type: String,
    pub trigger_details: String,
    pub status: RecoveryEventStatus,
    pub attempt_count: u32,
}

/// Update payload applied when a recovery event completes or advances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEventUpdate {
    pub status: RecoveryEventStatus,
    pub attempt_count: u32,
    pub webview_reloaded: bool,
}

/// Persisted `recovery_events` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEventRecord {
    pub id: i64,
    pub event_type: String,
    pub rule_id: Option<String>,
    pub service_id: Option<String>,
    pub trigger_type: String,
    pub trigger_details: String,
    pub status: RecoveryEventStatus,
    pub attempt_count: u32,
    pub webview_reloaded: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Persisted `command_executions` record linked to a `recovery_events` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandExecutionRecord {
    pub id: i64,
    pub recovery_event_id: i64,
    pub executable: String,
    pub arguments: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub status: String,
    pub duration_ms: u64,
    pub executed_at: String,
}

/// Retention/rotation limits preventing unbounded SQLite database growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub max_age_days: u32,
    pub max_events: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_days: 30,
            max_events: 10_000,
        }
    }
}
