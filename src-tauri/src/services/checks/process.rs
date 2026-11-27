//! Cross-platform OS process presence checker.

use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};

use crate::config::ServiceDefinition;
use crate::services::models::{ServiceCheckResult, ServiceHealthState};

/// Checks whether a process matching `process_name` is currently running.
pub async fn check_process(
    service: &ServiceDefinition,
    process_name: &str,
    timeout_seconds: u64,
) -> ServiceCheckResult {
    let started = Instant::now();
    let target = process_name.trim().to_string();
    let timeout = Duration::from_secs(timeout_seconds);

    let lookup_fut = tokio::task::spawn_blocking(move || is_process_running(&target));
    let res = tokio::time::timeout(timeout, lookup_fut).await;
    let latency_ms = started.elapsed().as_millis() as u64;

    match res {
        Ok(Ok(true)) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Healthy,
            http_status: None,
            message: format!("Process '{process_name}' is running"),
            latency_ms,
        },
        Ok(Ok(false)) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Stopped,
            http_status: None,
            message: format!("Process '{process_name}' is not running"),
            latency_ms,
        },
        Ok(Err(join_err)) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::CheckFailed,
            http_status: None,
            message: format!("Process check task failed: {join_err}"),
            latency_ms,
        },
        Err(_) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Timeout,
            http_status: None,
            message: format!("Process check timed out after {timeout_seconds}s"),
            latency_ms,
        },
    }
}

fn is_process_running(target: &str) -> bool {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let target_lower = target.to_ascii_lowercase();
    #[cfg(windows)]
    let target_with_exe = if target_lower.ends_with(".exe") {
        target_lower.clone()
    } else {
        format!("{target_lower}.exe")
    };

    system.processes().values().any(|proc_info| {
        let name = proc_info.name().to_string_lossy().to_ascii_lowercase();
        #[cfg(windows)]
        {
            name == target_lower || name == target_with_exe
        }
        #[cfg(not(windows))]
        {
            name == target_lower
        }
    })
}
