//! TCP port availability health checker.

use std::io::ErrorKind;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

use crate::config::ServiceDefinition;
use crate::services::models::{ServiceCheckResult, ServiceHealthState};

/// Attempts a TCP connection to `host:port` within `timeout_seconds`.
pub async fn check_tcp(
    service: &ServiceDefinition,
    host: &str,
    port: u16,
    timeout_seconds: u64,
) -> ServiceCheckResult {
    let started = Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);
    let addr = format!("{host}:{port}");

    let res = tokio::time::timeout(timeout, TcpStream::connect(&addr)).await;
    let latency_ms = started.elapsed().as_millis() as u64;

    match res {
        Ok(Ok(_stream)) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Healthy,
            http_status: None,
            message: format!("TCP port {addr} is reachable"),
            latency_ms,
        },
        Ok(Err(io_err)) => {
            let state = match io_err.kind() {
                ErrorKind::ConnectionRefused => ServiceHealthState::ConnectionRefused,
                ErrorKind::TimedOut => ServiceHealthState::Timeout,
                _ => ServiceHealthState::Stopped,
            };
            ServiceCheckResult {
                service_id: service.id.clone(),
                required: service.required,
                state,
                http_status: None,
                message: format!("TCP check failed for {addr}: {io_err}"),
                latency_ms,
            }
        }
        Err(_) => ServiceCheckResult {
            service_id: service.id.clone(),
            required: service.required,
            state: ServiceHealthState::Timeout,
            http_status: None,
            message: format!("TCP check timed out for {addr} after {timeout_seconds}s"),
            latency_ms,
        },
    }
}
