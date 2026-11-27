//! Safe WebView error signal verification bridge.
//!
//! WebView load/connection signals are treated only as hints that trigger
//! immediate Rust-side `ServiceManager` verification—never as command inputs.

use serde::{Deserialize, Serialize};

use crate::services::ServiceManager;

use super::detector::collect_conditions_from_report;
use super::models::DetectedCondition;
use super::monitor::ActiveRecoveryTracker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebViewSignal {
    LoadFailed,
    Offline,
}

/// Handles a WebView failure signal by verifying all services from Rust
/// and returning any newly detected conditions (excluding active recoveries).
pub async fn verify_webview_signal(
    service_manager: &ServiceManager,
    active_recoveries: &ActiveRecoveryTracker,
    _signal: WebViewSignal,
) -> Vec<DetectedCondition> {
    let report = service_manager.check_all_services().await;
    collect_conditions_from_report(service_manager, &report, false, Some(active_recoveries))
}
