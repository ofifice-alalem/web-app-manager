//! Startup readiness coordinator using repeated health checks instead of fixed sleeps.

use std::time::Duration;
use tokio::time::Instant;

use crate::services::ServiceManager;

use super::detector::collect_conditions_from_report;
use super::models::StartupReadinessOutcome;

/// Repeatedly polls `service_manager.check_all_services()` until all required services
/// are healthy or `startup_timeout_seconds` elapses.
pub async fn wait_for_startup_readiness(
    service_manager: &ServiceManager,
    startup_timeout_seconds: u64,
    poll_interval: Duration,
) -> StartupReadinessOutcome {
    let deadline = Instant::now() + Duration::from_secs(startup_timeout_seconds);

    loop {
        let report = service_manager.check_all_services().await;
        if report.is_ready {
            return StartupReadinessOutcome::Ready(report);
        }

        let now = Instant::now();
        if now >= deadline {
            let conditions =
                collect_conditions_from_report(service_manager, &report, true, None);
            return StartupReadinessOutcome::TimedOut { report, conditions };
        }

        let remaining = deadline.saturating_duration_since(now);
        tokio::time::sleep(poll_interval.min(remaining)).await;
    }
}
