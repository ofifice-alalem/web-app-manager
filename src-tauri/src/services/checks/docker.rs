//! Docker container status checker executed safely via `CommandExecutor`.

use std::time::Instant;

use crate::config::{ServiceDefinition, TargetOs};
use crate::executor::{CommandExecutor, CommandSpec, CommandStatus};
use crate::services::models::{ServiceCheckResult, ServiceHealthState};

/// Inspects a Docker container's running state without crashing if Docker is
/// stopped, missing, or inaccessible due to permissions.
pub async fn check_docker(
    executor: &CommandExecutor,
    service: &ServiceDefinition,
    container_name: &str,
    timeout_seconds: u64,
) -> ServiceCheckResult {
    let started = Instant::now();
    let docker_exe = match TargetOs::current() {
        TargetOs::Windows => "docker.exe",
        TargetOs::Linux => "docker",
    };

    let spec = CommandSpec {
        executable: docker_exe.to_string(),
        args: vec![
            "inspect".to_string(),
            "-f".to_string(),
            "{{.State.Running}}".to_string(),
            container_name.to_string(),
        ],
        working_dir: None,
        timeout_seconds,
    };

    let cmd_res = executor.execute(&spec).await;
    let latency_ms = started.elapsed().as_millis() as u64;

    match cmd_res.status {
        CommandStatus::Succeeded => {
            let output = cmd_res.stdout.trim().to_ascii_lowercase();
            if output == "true" {
                ServiceCheckResult {
                    service_id: service.id.clone(),
                    required: service.required,
                    state: ServiceHealthState::Healthy,
                    http_status: None,
                    message: format!("Docker container '{container_name}' is running"),
                    latency_ms,
                }
            } else if output == "false" {
                ServiceCheckResult {
                    service_id: service.id.clone(),
                    required: service.required,
                    state: ServiceHealthState::Stopped,
                    http_status: None,
                    message: format!("Docker container '{container_name}' is stopped"),
                    latency_ms,
                }
            } else {
                ServiceCheckResult {
                    service_id: service.id.clone(),
                    required: service.required,
                    state: ServiceHealthState::Unhealthy,
                    http_status: None,
                    message: format!(
                        "Unexpected docker inspect output for '{container_name}': {output}"
                    ),
                    latency_ms,
                }
            }
        }
        CommandStatus::TimedOut => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Timeout,
            http_status: None,
            message: format!("Docker inspect timed out after {timeout_seconds}s"),
            latency_ms,
        },
        CommandStatus::SpawnError => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::CheckFailed,
            http_status: None,
            message: cmd_res
                .error_message
                .unwrap_or_else(|| "Docker executable is not available".to_string()),
            latency_ms,
        },
        CommandStatus::Failed => {
            let stderr_lower = cmd_res.stderr.to_ascii_lowercase();
            let is_daemon_or_perm_error = stderr_lower.contains("cannot connect to the docker daemon")
                || stderr_lower.contains("permission denied")
                || stderr_lower.contains("is the docker daemon running")
                || stderr_lower.contains("error during connect");

            let state = if is_daemon_or_perm_error {
                ServiceHealthState::CheckFailed
            } else {
                ServiceHealthState::Stopped
            };

            ServiceCheckResult {
                service_id: service.id.clone(),
                required: service.required,
                state,
                http_status: None,
                message: format!(
                    "Docker check failed for '{container_name}': {}",
                    cmd_res.stderr.trim()
                ),
                latency_ms,
            }
        }
    }
}
