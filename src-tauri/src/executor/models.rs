//! Data models for secure background command execution.

use serde::{Deserialize, Serialize};

use crate::config::PlatformCommandSpec;

/// Specification for a single OS command executed directly without a shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub timeout_seconds: u64,
}

impl CommandSpec {
    /// Builds a `CommandSpec` from a validated `PlatformCommandSpec` and fallback timeout.
    pub fn from_platform_spec(spec: &PlatformCommandSpec, default_timeout_seconds: u64) -> Self {
        Self {
            executable: spec.executable.clone(),
            args: spec.args.clone(),
            working_dir: spec.working_dir.clone(),
            timeout_seconds: spec.timeout_seconds.unwrap_or(default_timeout_seconds),
        }
    }
}

/// A single step inside a sequential command chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandStep {
    pub spec: CommandSpec,
    pub continue_on_failure: bool,
}

/// Outcome classification of a command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Succeeded,
    Failed,
    TimedOut,
    SpawnError,
}

/// Captured, redacted result of executing a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub executable: String,
    pub args: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub status: CommandStatus,
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub error_message: Option<String>,
}
