//! Error detection and background monitoring module.

pub mod detector;
pub mod models;
pub mod monitor;
pub mod startup;
pub mod webview_signals;

pub use detector::detect_conditions;
pub use models::{DetectedCondition, DetectedTrigger, StartupReadinessOutcome};
pub use monitor::{ActiveRecoveryTracker, BackgroundMonitor};
pub use startup::wait_for_startup_readiness;
pub use webview_signals::{verify_webview_signal, WebViewSignal};
