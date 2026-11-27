//! Data models for error detection and background monitoring.

use serde::{Deserialize, Serialize};

use crate::config::ServiceStatusTriggerValue;
use crate::services::SystemReadinessReport;

/// Normalized fault condition emitted by the Error Detector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedCondition {
    pub service_id: String,
    pub trigger: DetectedTrigger,
    pub details: String,
}

/// Trigger variants corresponding 1-to-1 with `recovery-rules.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetectedTrigger {
    HttpStatus { status_code: u16 },
    ConnectionRefused,
    ServiceStatus { value: ServiceStatusTriggerValue },
    TcpUnavailable,
    ProcessNotRunning,
}

/// Result of the startup readiness polling loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupReadinessOutcome {
    Ready(SystemReadinessReport),
    TimedOut {
        report: SystemReadinessReport,
        conditions: Vec<DetectedCondition>,
    },
}
