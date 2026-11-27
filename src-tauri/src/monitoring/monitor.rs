//! Background runtime monitor and shared active-recovery suppression tracker.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::services::ServiceManager;

use super::detector::collect_conditions_from_report;
use super::models::DetectedCondition;

/// Tracks which `service_id`s currently have an active recovery running,
/// allowing the monitoring loop to suppress overlapping triggers.
#[derive(Debug, Default, Clone)]
pub struct ActiveRecoveryTracker {
    active_services: Arc<RwLock<HashSet<String>>>,
}

impl ActiveRecoveryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `service_id` as actively recovering. Returns `false` if already active.
    pub fn try_acquire(&self, service_id: &str) -> bool {
        let Ok(mut guard) = self.active_services.write() else {
            return false;
        };
        guard.insert(service_id.to_string())
    }

    pub fn release(&self, service_id: &str) {
        if let Ok(mut guard) = self.active_services.write() {
            guard.remove(service_id);
        }
    }

    pub fn is_active(&self, service_id: &str) -> bool {
        self.active_services
            .read()
            .map(|guard| guard.contains(service_id))
            .unwrap_or(false)
    }
}

/// Periodically checks configured services and emits `DetectedCondition`s
/// for services that are not currently undergoing recovery.
#[derive(Debug, Clone)]
pub struct BackgroundMonitor {
    service_manager: Arc<ServiceManager>,
    active_recoveries: ActiveRecoveryTracker,
    poll_interval: Duration,
}

impl BackgroundMonitor {
    pub fn new(
        service_manager: Arc<ServiceManager>,
        active_recoveries: ActiveRecoveryTracker,
        poll_interval: Duration,
    ) -> Self {
        Self {
            service_manager,
            active_recoveries,
            poll_interval,
        }
    }

    /// Runs one monitoring pass, skipping services with an active recovery in progress.
    pub async fn poll_once(&self) -> Vec<DetectedCondition> {
        let report = self.service_manager.check_all_services().await;
        collect_conditions_from_report(
            &self.service_manager,
            &report,
            false,
            Some(&self.active_recoveries),
        )
    }

    /// Runs the background monitoring loop until `shutdown_rx` signals `true`.
    pub async fn run(
        self,
        mut shutdown_rx: watch::Receiver<bool>,
        condition_tx: mpsc::Sender<DetectedCondition>,
    ) {
        let mut interval = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                _ = interval.tick() => {
                    let conditions = self.poll_once().await;
                    for cond in conditions {
                        if condition_tx.send(cond).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ServiceCheck, ServiceCommands, ServiceDefinition};
    use crate::executor::CommandExecutor;
    use crate::monitoring::detector::detect_conditions;
    use crate::monitoring::models::{DetectedTrigger, StartupReadinessOutcome};
    use crate::monitoring::startup::wait_for_startup_readiness;
    use crate::services::{ServiceCheckResult, ServiceHealthState};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sample_http_service(id: &str, url: &str, required: bool) -> ServiceDefinition {
        ServiceDefinition {
            id: id.to_string(),
            name: id.to_string(),
            required,
            check: ServiceCheck::Http {
                url: url.to_string(),
                expected_status: vec![200],
                timeout_seconds: 2,
            },
            commands: ServiceCommands::default(),
        }
    }

    #[test]
    fn maps_failures_to_expected_triggers() {
        let http_svc = sample_http_service("web", "http://127.0.0.1:8000", true);
        let http_500 = ServiceCheckResult {
            service_id: "web".to_string(),
            required: true,
            state: ServiceHealthState::Unhealthy,
            http_status: Some(500),
            message: "HTTP 500".to_string(),
            latency_ms: 10,
        };
        let conds = detect_conditions(&http_svc, &http_500);
        assert!(conds.iter().any(|c| matches!(
            c.trigger,
            DetectedTrigger::HttpStatus { status_code: 500 }
        )));

        let tcp_svc = ServiceDefinition {
            id: "db".to_string(),
            name: "DB".to_string(),
            required: true,
            check: ServiceCheck::Tcp {
                host: "127.0.0.1".to_string(),
                port: 5432,
                timeout_seconds: 2,
            },
            commands: ServiceCommands::default(),
        };
        let tcp_refused = ServiceCheckResult {
            service_id: "db".to_string(),
            required: true,
            state: ServiceHealthState::ConnectionRefused,
            http_status: None,
            message: "refused".to_string(),
            latency_ms: 5,
        };
        let tcp_conds = detect_conditions(&tcp_svc, &tcp_refused);
        assert!(tcp_conds
            .iter()
            .any(|c| matches!(c.trigger, DetectedTrigger::ConnectionRefused)));
        assert!(tcp_conds
            .iter()
            .any(|c| matches!(c.trigger, DetectedTrigger::TcpUnavailable)));

        let proc_svc = ServiceDefinition {
            id: "printer".to_string(),
            name: "Printer".to_string(),
            required: true,
            check: ServiceCheck::Process {
                process_name: "print-server".to_string(),
                timeout_seconds: 2,
            },
            commands: ServiceCommands::default(),
        };
        let proc_stopped = ServiceCheckResult {
            service_id: "printer".to_string(),
            required: true,
            state: ServiceHealthState::Stopped,
            http_status: None,
            message: "stopped".to_string(),
            latency_ms: 5,
        };
        let proc_conds = detect_conditions(&proc_svc, &proc_stopped);
        assert!(proc_conds
            .iter()
            .any(|c| matches!(c.trigger, DetectedTrigger::ProcessNotRunning)));
    }

    #[tokio::test]
    async fn startup_readiness_succeeds_even_if_optional_service_fails() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open_port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 256];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                    .await;
            }
        });

        let unused = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };

        let manager = ServiceManager::new(
            vec![
                sample_http_service("req-ok", &format!("http://127.0.0.1:{open_port}"), true),
                sample_http_service("opt-down", &format!("http://127.0.0.1:{unused}"), false),
            ],
            CommandExecutor::new(),
        )
        .unwrap();

        let outcome =
            wait_for_startup_readiness(&manager, 2, Duration::from_millis(50)).await;
        assert!(matches!(outcome, StartupReadinessOutcome::Ready(_)));
    }

    #[tokio::test]
    async fn startup_readiness_times_out_when_required_service_is_down() {
        let unused = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };

        let manager = ServiceManager::new(
            vec![ServiceDefinition {
                id: "req-down".to_string(),
                name: "Req Down".to_string(),
                required: true,
                check: ServiceCheck::Tcp {
                    host: "127.0.0.1".to_string(),
                    port: unused,
                    timeout_seconds: 1,
                },
                commands: ServiceCommands::default(),
            }],
            CommandExecutor::new(),
        )
        .unwrap();

        let outcome =
            wait_for_startup_readiness(&manager, 1, Duration::from_millis(100)).await;
        match outcome {
            StartupReadinessOutcome::TimedOut { conditions, .. } => {
                assert!(!conditions.is_empty());
                assert_eq!(conditions[0].service_id, "req-down");
            }
            StartupReadinessOutcome::Ready(_) => panic!("expected timeout"),
        }
    }

    #[tokio::test]
    async fn monitor_suppresses_triggers_while_recovery_is_active() {
        let unused = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };

        let manager = Arc::new(
            ServiceManager::new(
                vec![ServiceDefinition {
                    id: "svc-a".to_string(),
                    name: "Svc A".to_string(),
                    required: true,
                    check: ServiceCheck::Tcp {
                        host: "127.0.0.1".to_string(),
                        port: unused,
                        timeout_seconds: 1,
                    },
                    commands: ServiceCommands::default(),
                }],
                CommandExecutor::new(),
            )
            .unwrap(),
        );

        let tracker = ActiveRecoveryTracker::new();
        let monitor =
            BackgroundMonitor::new(manager, tracker.clone(), Duration::from_secs(10));

        // While recovery is active, poll_once must suppress conditions for svc-a.
        assert!(tracker.try_acquire("svc-a"));
        assert!(monitor.poll_once().await.is_empty());

        // Once released, poll_once detects the fault again.
        tracker.release("svc-a");
        assert!(!monitor.poll_once().await.is_empty());
    }
}
