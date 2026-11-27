//! Unified service health and lifecycle models.

use serde::{Deserialize, Serialize};

use crate::executor::CommandResult;

/// Normalized health state for any monitored service regardless of check type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceHealthState {
    Healthy,
    Stopped,
    Unhealthy,
    ConnectionRefused,
    Timeout,
    CheckFailed,
}

impl ServiceHealthState {
    pub fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// Result of running a single service health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceCheckResult {
    pub service_id: String,
    pub required: bool,
    pub state: ServiceHealthState,
    pub http_status: Option<u16>,
    pub message: String,
    pub latency_ms: u64,
}

/// Aggregated readiness across all configured services.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemReadinessReport {
    pub is_ready: bool,
    pub results: Vec<ServiceCheckResult>,
}

/// Supported service lifecycle commands defined in `services.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLifecycleAction {
    Start,
    Stop,
    Restart,
}

/// Outcome of executing a service lifecycle command and re-checking the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLifecycleOutcome {
    pub service_id: String,
    pub action: ServiceLifecycleAction,
    pub command_executed: bool,
    pub command_result: Option<CommandResult>,
    pub post_check: Option<ServiceCheckResult>,
    pub success: bool,
    pub message: String,
}
