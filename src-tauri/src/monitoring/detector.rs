//! Deterministic mapping from `ServiceCheckResult` to predefined `DetectedCondition`s.

use crate::config::{ServiceCheck, ServiceDefinition, ServiceStatusTriggerValue};
use crate::services::{ServiceCheckResult, ServiceHealthState, ServiceManager, SystemReadinessReport};

use super::models::{DetectedCondition, DetectedTrigger};
use super::monitor::ActiveRecoveryTracker;

/// Collects `DetectedCondition`s for unhealthy services in `report`,
/// optionally filtering to required-only services or skipping active recoveries.
pub fn collect_conditions_from_report(
    service_manager: &ServiceManager,
    report: &SystemReadinessReport,
    required_only: bool,
    active_recoveries: Option<&ActiveRecoveryTracker>,
) -> Vec<DetectedCondition> {
    let mut conditions = Vec::new();
    for check_res in &report.results {
        if check_res.state.is_healthy() {
            continue;
        }
        if required_only && !check_res.required {
            continue;
        }
        if let Some(tracker) = active_recoveries {
            if tracker.is_active(&check_res.service_id) {
                continue;
            }
        }
        if let Some(svc) = service_manager.find_service(&check_res.service_id) {
            conditions.extend(detect_conditions(svc, check_res));
        }
    }
    conditions
}

/// Converts a failed `ServiceCheckResult` into supported `DetectedCondition`s.
/// Returns an empty list if the service is `Healthy`.
/// Never infers unrelated causes (e.g., HTTP 500 is never treated as Docker stopped).
pub fn detect_conditions(
    service: &ServiceDefinition,
    check: &ServiceCheckResult,
) -> Vec<DetectedCondition> {
    if check.state.is_healthy() {
        return Vec::new();
    }

    let mut conditions = Vec::new();
    let details = check.message.clone();

    match (&service.check, check.state) {
        (ServiceCheck::Http { .. }, ServiceHealthState::Unhealthy) => {
            if let Some(code) = check.http_status {
                conditions.push(DetectedCondition {
                    service_id: service.id.clone(),
                    trigger: DetectedTrigger::HttpStatus { status_code: code },
                    details: details.clone(),
                });
            }
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Unhealthy,
                },
                details,
            });
        }
        (ServiceCheck::Http { .. }, ServiceHealthState::ConnectionRefused) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ConnectionRefused,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details,
            });
        }
        (ServiceCheck::Tcp { .. }, ServiceHealthState::ConnectionRefused) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ConnectionRefused,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::TcpUnavailable,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details,
            });
        }
        (ServiceCheck::Tcp { .. }, ServiceHealthState::Timeout) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::TcpUnavailable,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Timeout,
                },
                details,
            });
        }
        (ServiceCheck::Tcp { .. }, _) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::TcpUnavailable,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details,
            });
        }
        (ServiceCheck::Process { .. }, ServiceHealthState::Stopped) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ProcessNotRunning,
                details: details.clone(),
            });
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details,
            });
        }
        (_, ServiceHealthState::Timeout) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Timeout,
                },
                details,
            });
        }
        (_, ServiceHealthState::Stopped) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details,
            });
        }
        (_, _) => {
            conditions.push(DetectedCondition {
                service_id: service.id.clone(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Unhealthy,
                },
                details,
            });
        }
    }

    conditions
}
