//! End-to-end fault-injection integration tests exercising the full
//! Config -> ServiceManager -> Monitoring -> RecoveryEngine -> SQLite LogManager pipeline.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use desktop_application_host_lib::config::{
    RecoveryAction, RecoveryRule, RecoveryTrigger, RetryPolicy, ServiceCheck, ServiceCommands,
    ServiceDefinition,
};
use desktop_application_host_lib::database::{LogManager, RecoveryEventStatus};
use desktop_application_host_lib::executor::CommandExecutor;
use desktop_application_host_lib::monitoring::{
    detect_conditions, wait_for_startup_readiness, ActiveRecoveryTracker, DetectedTrigger,
    StartupReadinessOutcome,
};
use desktop_application_host_lib::recovery::{RecoveryEngine, RecoveryOutcome, RecoveryState};
use desktop_application_host_lib::services::{ServiceHealthState, ServiceManager};

async fn spawn_toggle_http_server(healthy: Arc<AtomicBool>, delay_ms: u64) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let healthy_flag = Arc::clone(&healthy);
            tokio::spawn(async move {
                let mut buf = [0u8; 512];
                let _ = socket.read(&mut buf).await;
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                let status = if healthy_flag.load(Ordering::SeqCst) {
                    "200 OK"
                } else {
                    "500 Internal Server Error"
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                );
                let _ = socket.write_all(resp.as_bytes()).await;
            });
        }
    });

    port
}

async fn unused_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

fn ok_command_action() -> RecoveryAction {
    #[cfg(windows)]
    {
        RecoveryAction::Command {
            executable: Some("powershell.exe".to_string()),
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Write-Output 'restarted'".to_string(),
            ],
            working_dir: None,
            platform_commands: None,
            timeout_seconds: 5,
            continue_on_failure: false,
        }
    }
    #[cfg(not(windows))]
    {
        RecoveryAction::Command {
            executable: Some("echo".to_string()),
            args: vec!["restarted".to_string()],
            working_dir: None,
            platform_commands: None,
            timeout_seconds: 5,
            continue_on_failure: false,
        }
    }
}

fn failing_command_action() -> RecoveryAction {
    #[cfg(windows)]
    {
        RecoveryAction::Command {
            executable: Some("powershell.exe".to_string()),
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "exit 1".to_string(),
            ],
            working_dir: None,
            platform_commands: None,
            timeout_seconds: 5,
            continue_on_failure: false,
        }
    }
    #[cfg(not(windows))]
    {
        RecoveryAction::Command {
            executable: Some("sh".to_string()),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            working_dir: None,
            platform_commands: None,
            timeout_seconds: 5,
            continue_on_failure: false,
        }
    }
}

#[tokio::test]
async fn e2e_http_500_recovery_reloads_webview_and_suppresses_repeat_loop() {
    let healthy = Arc::new(AtomicBool::new(false));
    let port = spawn_toggle_http_server(Arc::clone(&healthy), 0).await;

    let svc = ServiceDefinition {
        id: "sales-app".to_string(),
        name: "Sales App".to_string(),
        required: true,
        check: ServiceCheck::Http {
            url: format!("http://127.0.0.1:{port}"),
            expected_status: vec![200],
            timeout_seconds: 3,
        },
        commands: ServiceCommands::default(),
    };

    let service_manager =
        Arc::new(ServiceManager::new(vec![svc.clone()], CommandExecutor::new()).unwrap());
    let log_manager = LogManager::open_in_memory().unwrap();
    let tracker = ActiveRecoveryTracker::new();

    // 1. Startup check detects HTTP 500 -> TimedOut with HttpStatus(500) condition.
    let startup = wait_for_startup_readiness(&service_manager, 0, Duration::from_millis(20)).await;
    let conditions = match startup {
        StartupReadinessOutcome::TimedOut { conditions, .. } => conditions,
        StartupReadinessOutcome::Ready(_) => panic!("expected startup timeout on HTTP 500"),
    };
    assert!(conditions
        .iter()
        .any(|c| matches!(c.trigger, DetectedTrigger::HttpStatus { status_code: 500 })));

    // 2. Flip server healthy to simulate successful recovery action effect.
    healthy.store(true, Ordering::SeqCst);

    let engine = RecoveryEngine::new(
        vec![RecoveryRule {
            id: "fix-http-500".to_string(),
            name: "Fix HTTP 500".to_string(),
            enabled: true,
            trigger: RecoveryTrigger::HttpStatus {
                service_id: "sales-app".to_string(),
                status_codes: vec![500],
            },
            actions: vec![
                ok_command_action(),
                RecoveryAction::CheckService {
                    service_id: "sales-app".to_string(),
                    timeout_seconds: 3,
                },
                RecoveryAction::ReloadApplication { enabled: true },
            ],
            retry: RetryPolicy {
                max_attempts: 2,
                delay_seconds: 0,
            },
            cooldown_seconds: 60,
        }],
        true,
        Arc::clone(&service_manager),
        CommandExecutor::new(),
        log_manager.clone(),
        RecoveryState::new(tracker),
    );

    let http_500_cond = conditions
        .into_iter()
        .find(|c| matches!(c.trigger, DetectedTrigger::HttpStatus { status_code: 500 }))
        .unwrap();

    let outcome = engine.handle_condition(&http_500_cond).await;
    assert_eq!(
        outcome,
        RecoveryOutcome::Succeeded {
            rule_id: "fix-http-500".to_string(),
            attempt_count: 1,
            should_reload_webview: true,
        }
    );

    // 3. Repeat fault within cooldown must not loop.
    let repeat = engine.handle_condition(&http_500_cond).await;
    assert_eq!(
        repeat,
        RecoveryOutcome::SkippedCooldown {
            rule_id: "fix-http-500".to_string(),
        }
    );

    let events = log_manager.query_recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, RecoveryEventStatus::Succeeded);
    assert!(events[0].webview_reloaded);
}

#[tokio::test]
async fn e2e_connection_refused_and_command_failure_exhaust_retries() {
    let closed_port = unused_port().await;
    let svc = ServiceDefinition {
        id: "print-server".to_string(),
        name: "Print Server".to_string(),
        required: true,
        check: ServiceCheck::Http {
            url: format!("http://127.0.0.1:{closed_port}/health"),
            expected_status: vec![200],
            timeout_seconds: 5,
        },
        commands: ServiceCommands::default(),
    };

    let service_manager =
        Arc::new(ServiceManager::new(vec![svc.clone()], CommandExecutor::new()).unwrap());
    let log_manager = LogManager::open_in_memory().unwrap();

    let check = service_manager.check_service("print-server").await.unwrap();
    assert_eq!(check.state, ServiceHealthState::ConnectionRefused);

    let conds = detect_conditions(&svc, &check);
    let refused_cond = conds
        .into_iter()
        .find(|c| matches!(c.trigger, DetectedTrigger::ConnectionRefused))
        .expect("ConnectionRefused trigger");

    let engine = RecoveryEngine::new(
        vec![RecoveryRule {
            id: "fix-print".to_string(),
            name: "Fix Print".to_string(),
            enabled: true,
            trigger: RecoveryTrigger::ConnectionRefused {
                service_id: "print-server".to_string(),
            },
            actions: vec![
                failing_command_action(),
                RecoveryAction::ReloadApplication { enabled: true },
            ],
            retry: RetryPolicy {
                max_attempts: 3,
                delay_seconds: 0,
            },
            cooldown_seconds: 60,
        }],
        true,
        service_manager,
        CommandExecutor::new(),
        log_manager.clone(),
        RecoveryState::new(ActiveRecoveryTracker::new()),
    );

    let outcome = engine.handle_condition(&refused_cond).await;
    assert_eq!(
        outcome,
        RecoveryOutcome::Failed {
            rule_id: "fix-print".to_string(),
            attempt_count: 3,
        }
    );

    let events = log_manager.query_recent_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, RecoveryEventStatus::Failed);
    assert!(!events[0].webview_reloaded);
    assert_eq!(
        log_manager.query_executions_for_event(events[0].id).unwrap().len(),
        3
    );
}

#[tokio::test]
async fn e2e_timeout_and_optional_service_failure_do_not_block_system() {
    let healthy = Arc::new(AtomicBool::new(true));
    let fast_port = spawn_toggle_http_server(Arc::clone(&healthy), 0).await;
    let slow_port = spawn_toggle_http_server(Arc::clone(&healthy), 2000).await;

    let manager = ServiceManager::new(
        vec![
            ServiceDefinition {
                id: "main-app".to_string(),
                name: "Main App".to_string(),
                required: true,
                check: ServiceCheck::Http {
                    url: format!("http://127.0.0.1:{fast_port}"),
                    expected_status: vec![200],
                    timeout_seconds: 2,
                },
                commands: ServiceCommands::default(),
            },
            ServiceDefinition {
                id: "optional-telemetry".to_string(),
                name: "Optional".to_string(),
                required: false,
                check: ServiceCheck::Http {
                    url: format!("http://127.0.0.1:{slow_port}"),
                    expected_status: vec![200],
                    timeout_seconds: 1,
                },
                commands: ServiceCommands::default(),
            },
        ],
        CommandExecutor::new(),
    )
    .unwrap();

    let report = manager.check_all_services().await;
    assert!(report.is_ready);
    assert_eq!(report.results[0].state, ServiceHealthState::Healthy);
    assert_eq!(report.results[1].state, ServiceHealthState::Timeout);
}
