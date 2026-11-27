//! HTTP endpoint health checker.

use std::error::Error;
use std::time::{Duration, Instant};

use crate::config::ServiceDefinition;
use crate::services::models::{ServiceCheckResult, ServiceHealthState};

/// Performs an HTTP GET health check without following redirects so status codes
/// such as 301/302 can be matched directly against `expected_status`.
pub async fn check_http(
    client: &reqwest::Client,
    service: &ServiceDefinition,
    url: &str,
    expected_status: &[u16],
    timeout_seconds: u64,
) -> ServiceCheckResult {
    let started = Instant::now();
    let timeout = Duration::from_secs(timeout_seconds);

    let response_res = client.get(url).timeout(timeout).send().await;
    let latency_ms = started.elapsed().as_millis() as u64;

    match response_res {
        Ok(resp) => {
            let code = resp.status().as_u16();
            if expected_status.contains(&code) {
                ServiceCheckResult {
                    service_id: service.id.clone(),
                    required: service.required,
                    state: ServiceHealthState::Healthy,
                    http_status: Some(code),
                    message: format!("HTTP {code} matched expected status"),
                    latency_ms,
                }
            } else {
                ServiceCheckResult {
                    service_id: service.id.clone(),
                    required: service.required,
                    state: ServiceHealthState::Unhealthy,
                    http_status: Some(code),
                    message: format!("Unexpected HTTP status {code}"),
                    latency_ms,
                }
            }
        }
        Err(err) => {
            let state = classify_reqwest_error(&err);
            ServiceCheckResult {
                service_id: service.id.clone(),
                required: service.required,
                state,
                http_status: None,
                message: format!("HTTP check error: {err}"),
                latency_ms,
            }
        }
    }
}

fn classify_reqwest_error(err: &reqwest::Error) -> ServiceHealthState {
    if err.is_timeout() {
        return ServiceHealthState::Timeout;
    }

    if err.is_connect() {
        // Walk the error source chain to distinguish ConnectionRefused from other connect errors.
        let mut current: Option<&(dyn Error + 'static)> = Some(err);
        while let Some(source) = current {
            if let Some(io_err) = source.downcast_ref::<std::io::Error>() {
                match io_err.kind() {
                    std::io::ErrorKind::ConnectionRefused => {
                        return ServiceHealthState::ConnectionRefused;
                    }
                    std::io::ErrorKind::TimedOut => return ServiceHealthState::Timeout,
                    _ => {}
                }
            }
            current = source.source();
        }
        return ServiceHealthState::ConnectionRefused;
    }

    ServiceHealthState::CheckFailed
}
