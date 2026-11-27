//! Automated Recovery Engine executing ordered rule actions with retries,
//! cooldowns, per-service locks, post-recovery verification, and SQLite logging.

use std::sync::Arc;
use std::time::Duration;

use crate::config::{RecoveryAction, RecoveryRule, TargetOs};
use crate::database::{
    LogManager, NewRecoveryEvent, RecoveryEventStatus, RecoveryEventUpdate,
};
use crate::executor::{CommandExecutor, CommandSpec};
use crate::monitoring::DetectedCondition;
use crate::services::{ServiceLifecycleAction, ServiceManager};

use super::matcher::find_matching_rule;
use super::state::RecoveryState;

/// Result of handling a single `DetectedCondition`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryOutcome {
    Succeeded {
        rule_id: String,
        attempt_count: u32,
        should_reload_webview: bool,
    },
    Failed {
        rule_id: String,
        attempt_count: u32,
    },
    SkippedCooldown {
        rule_id: String,
    },
    SkippedAlreadyActive {
        service_id: String,
    },
    NoMatchingRule,
}

/// Orchestrates rule-based recovery for detected service faults.
#[derive(Clone)]
pub struct RecoveryEngine {
    rules: Vec<RecoveryRule>,
    reload_after_recovery: bool,
    service_manager: Arc<ServiceManager>,
    executor: CommandExecutor,
    log_manager: LogManager,
    state: RecoveryState,
}

impl RecoveryEngine {
    pub fn new(
        rules: Vec<RecoveryRule>,
        reload_after_recovery: bool,
        service_manager: Arc<ServiceManager>,
        executor: CommandExecutor,
        log_manager: LogManager,
        state: RecoveryState,
    ) -> Self {
        Self {
            rules,
            reload_after_recovery,
            service_manager,
            executor,
            log_manager,
            state,
        }
    }

    pub fn state(&self) -> &RecoveryState {
        &self.state
    }

    /// Matches `condition` to an enabled rule and executes recovery safely.
    pub async fn handle_condition(&self, condition: &DetectedCondition) -> RecoveryOutcome {
        let Some(rule) = find_matching_rule(&self.rules, condition) else {
            return RecoveryOutcome::NoMatchingRule;
        };

        if self.state.is_in_cooldown(&rule.id, rule.cooldown_seconds) {
            return RecoveryOutcome::SkippedCooldown {
                rule_id: rule.id.clone(),
            };
        }

        let Some(_active_guard) = self.state.try_begin_service_recovery(&condition.service_id) else {
            return RecoveryOutcome::SkippedAlreadyActive {
                service_id: condition.service_id.clone(),
            };
        };

        let service_mutex = self.state.service_mutex(&condition.service_id);
        let _service_lock = service_mutex.lock().await;

        // Re-check cooldown after acquiring lock in case another queued task just finished.
        if self.state.is_in_cooldown(&rule.id, rule.cooldown_seconds) {
            return RecoveryOutcome::SkippedCooldown {
                rule_id: rule.id.clone(),
            };
        }

        let event_id = self
            .log_manager
            .create_recovery_event(NewRecoveryEvent {
                event_type: "recovery".to_string(),
                rule_id: Some(rule.id.clone()),
                service_id: Some(condition.service_id.clone()),
                trigger_type: trigger_type_name(&condition.trigger).to_string(),
                trigger_details: condition.details.clone(),
                status: RecoveryEventStatus::InProgress,
                attempt_count: 0,
            })
            .ok();

        let max_attempts = rule.retry.max_attempts.max(1);
        for attempt in 1..=max_attempts {
            // Each retry restarts the rule's action sequence from the beginning.
            let attempt_res = self
                .execute_rule_attempt(rule, &condition.service_id, event_id)
                .await;

            if attempt_res.succeeded {
                let verified_healthy = if attempt_res.checked_target_service {
                    true
                } else {
                    self.service_manager
                        .check_service(&condition.service_id)
                        .await
                        .map(|r| r.state.is_healthy())
                        .unwrap_or(false)
                };

                if verified_healthy {
                    let should_reload =
                        attempt_res.reload_requested && self.reload_after_recovery;

                    if let Some(id) = event_id {
                        let _ = self.log_manager.update_recovery_event(
                            id,
                            RecoveryEventUpdate {
                                status: RecoveryEventStatus::Succeeded,
                                attempt_count: attempt,
                                webview_reloaded: should_reload,
                            },
                        );
                    }
                    self.state.mark_cooldown(&rule.id);

                    return RecoveryOutcome::Succeeded {
                        rule_id: rule.id.clone(),
                        attempt_count: attempt,
                        should_reload_webview: should_reload,
                    };
                }
            }

            if attempt < max_attempts && rule.retry.delay_seconds > 0 {
                tokio::time::sleep(Duration::from_secs(rule.retry.delay_seconds)).await;
            }
        }

        if let Some(id) = event_id {
            let _ = self.log_manager.update_recovery_event(
                id,
                RecoveryEventUpdate {
                    status: RecoveryEventStatus::Failed,
                    attempt_count: max_attempts,
                    webview_reloaded: false,
                },
            );
        }
        self.state.mark_cooldown(&rule.id);

        RecoveryOutcome::Failed {
            rule_id: rule.id.clone(),
            attempt_count: max_attempts,
        }
    }

    async fn execute_rule_attempt(
        &self,
        rule: &RecoveryRule,
        target_service_id: &str,
        event_id: Option<i64>,
    ) -> AttemptResult {
        let mut reload_requested = false;
        let mut checked_target_service = false;

        for action in &rule.actions {
            match action {
                RecoveryAction::Command {
                    executable,
                    args,
                    working_dir,
                    platform_commands,
                    timeout_seconds,
                    continue_on_failure,
                } => {
                    let spec = resolve_action_command(
                        executable.as_deref(),
                        args,
                        working_dir.as_deref(),
                        platform_commands.as_ref(),
                        *timeout_seconds,
                    );
                    let Some(cmd_spec) = spec else {
                        if !continue_on_failure {
                            return AttemptResult::failed();
                        }
                        continue;
                    };

                    let cmd_result = self.executor.execute(&cmd_spec).await;
                    if let Some(id) = event_id {
                        let _ = self.log_manager.record_command_execution(id, &cmd_result);
                    }

                    if !cmd_result.success && !continue_on_failure {
                        return AttemptResult::failed();
                    }
                }
                RecoveryAction::StartService {
                    service_id,
                    timeout_seconds,
                    continue_on_failure,
                } => {
                    let ok = self
                        .run_service_action(
                            service_id,
                            ServiceLifecycleAction::Start,
                            *timeout_seconds,
                            event_id,
                        )
                        .await;
                    if !ok && !continue_on_failure {
                        return AttemptResult::failed();
                    }
                }
                RecoveryAction::RestartService {
                    service_id,
                    timeout_seconds,
                    continue_on_failure,
                } => {
                    let ok = self
                        .run_service_action(
                            service_id,
                            ServiceLifecycleAction::Restart,
                            *timeout_seconds,
                            event_id,
                        )
                        .await;
                    if !ok && !continue_on_failure {
                        return AttemptResult::failed();
                    }
                }
                RecoveryAction::Wait { seconds } => {
                    tokio::time::sleep(Duration::from_secs(*seconds)).await;
                }
                RecoveryAction::CheckService { service_id, .. } => {
                    let healthy = self
                        .service_manager
                        .check_service(service_id)
                        .await
                        .map(|r| r.state.is_healthy())
                        .unwrap_or(false);
                    if !healthy {
                        return AttemptResult::failed();
                    }
                    if service_id == target_service_id {
                        checked_target_service = true;
                    }
                }
                RecoveryAction::ReloadApplication { enabled } => {
                    reload_requested = *enabled;
                }
            }
        }

        AttemptResult {
            succeeded: true,
            reload_requested,
            checked_target_service,
        }
    }

    async fn run_service_action(
        &self,
        service_id: &str,
        action: ServiceLifecycleAction,
        timeout_seconds: u64,
        event_id: Option<i64>,
    ) -> bool {
        let Ok(outcome) = self
            .service_manager
            .execute_lifecycle_action(service_id, action, Some(timeout_seconds))
            .await
        else {
            return false;
        };

        if let (Some(id), Some(cmd_res)) = (event_id, outcome.command_result.as_ref()) {
            let _ = self.log_manager.record_command_execution(id, cmd_res);
        }

        outcome.success
    }
}

struct AttemptResult {
    succeeded: bool,
    reload_requested: bool,
    checked_target_service: bool,
}

impl AttemptResult {
    fn failed() -> Self {
        Self {
            succeeded: false,
            reload_requested: false,
            checked_target_service: false,
        }
    }
}

fn resolve_action_command(
    executable: Option<&str>,
    args: &[String],
    working_dir: Option<&str>,
    platform_commands: Option<&crate::config::PlatformCommandMap>,
    timeout_seconds: u64,
) -> Option<CommandSpec> {
    if let Some(pm) = platform_commands {
        let os_spec = match TargetOs::current() {
            TargetOs::Windows => pm.windows.as_ref(),
            TargetOs::Linux => pm.linux.as_ref(),
        };
        if let Some(spec) = os_spec {
            return Some(CommandSpec::from_platform_spec(spec, timeout_seconds));
        }
    }

    executable.map(|exe| CommandSpec {
        executable: exe.to_string(),
        args: args.to_vec(),
        working_dir: working_dir.map(str::to_string),
        timeout_seconds,
    })
}

fn trigger_type_name(trigger: &crate::monitoring::DetectedTrigger) -> &'static str {
    match trigger {
        crate::monitoring::DetectedTrigger::HttpStatus { .. } => "http_status",
        crate::monitoring::DetectedTrigger::ConnectionRefused => "connection_refused",
        crate::monitoring::DetectedTrigger::ServiceStatus { .. } => "service_status",
        crate::monitoring::DetectedTrigger::TcpUnavailable => "tcp_unavailable",
        crate::monitoring::DetectedTrigger::ProcessNotRunning => "process_not_running",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        RecoveryAction, RecoveryRule, RecoveryTrigger, RetryPolicy, ServiceCheck, ServiceCommands,
        ServiceDefinition, ServiceStatusTriggerValue,
    };
    use crate::monitoring::{ActiveRecoveryTracker, DetectedTrigger};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_ok_server() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 256];
                let _ = s.read(&mut buf).await;
                let _ = s
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                    .await;
            }
        });
        port
    }

    async fn unused_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    fn echo_action(continue_on_failure: bool) -> RecoveryAction {
        #[cfg(windows)]
        {
            RecoveryAction::Command {
                executable: Some("powershell.exe".to_string()),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    "Write-Output 'recovered'".to_string(),
                ],
                working_dir: None,
                platform_commands: None,
                timeout_seconds: 5,
                continue_on_failure,
            }
        }
        #[cfg(not(windows))]
        {
            RecoveryAction::Command {
                executable: Some("echo".to_string()),
                args: vec!["recovered".to_string()],
                working_dir: None,
                platform_commands: None,
                timeout_seconds: 5,
                continue_on_failure,
            }
        }
    }

    fn fail_action(continue_on_failure: bool) -> RecoveryAction {
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
                continue_on_failure,
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
                continue_on_failure,
            }
        }
    }

    #[tokio::test]
    async fn ignores_disabled_rules() {
        let port = spawn_ok_server().await;
        let manager = Arc::new(
            ServiceManager::new(
                vec![ServiceDefinition {
                    id: "web".to_string(),
                    name: "Web".to_string(),
                    required: true,
                    check: ServiceCheck::Http {
                        url: format!("http://127.0.0.1:{port}"),
                        expected_status: vec![200],
                        timeout_seconds: 2,
                    },
                    commands: ServiceCommands::default(),
                }],
                CommandExecutor::new(),
            )
            .unwrap(),
        );
        let engine = RecoveryEngine::new(
            vec![RecoveryRule {
                id: "disabled-rule".to_string(),
                name: "Disabled".to_string(),
                enabled: false,
                trigger: RecoveryTrigger::ServiceStatus {
                    service_id: "web".to_string(),
                    value: ServiceStatusTriggerValue::Stopped,
                },
                actions: vec![echo_action(false)],
                retry: RetryPolicy {
                    max_attempts: 1,
                    delay_seconds: 0,
                },
                cooldown_seconds: 60,
            }],
            true,
            manager,
            CommandExecutor::new(),
            LogManager::open_in_memory().unwrap(),
            RecoveryState::new(ActiveRecoveryTracker::new()),
        );

        let outcome = engine
            .handle_condition(&DetectedCondition {
                service_id: "web".to_string(),
                trigger: DetectedTrigger::ServiceStatus {
                    value: ServiceStatusTriggerValue::Stopped,
                },
                details: "stopped".to_string(),
            })
            .await;

        assert_eq!(outcome, RecoveryOutcome::NoMatchingRule);
    }

    #[tokio::test]
    async fn executes_ordered_actions_logs_to_sqlite_and_enforces_cooldown() {
        let port = spawn_ok_server().await;
        let log_manager = LogManager::open_in_memory().unwrap();
        let manager = Arc::new(
            ServiceManager::new(
                vec![ServiceDefinition {
                    id: "web".to_string(),
                    name: "Web".to_string(),
                    required: true,
                    check: ServiceCheck::Http {
                        url: format!("http://127.0.0.1:{port}"),
                        expected_status: vec![200],
                        timeout_seconds: 2,
                    },
                    commands: ServiceCommands::default(),
                }],
                CommandExecutor::new(),
            )
            .unwrap(),
        );

        let engine = RecoveryEngine::new(
            vec![RecoveryRule {
                id: "recover-web".to_string(),
                name: "Recover Web".to_string(),
                enabled: true,
                trigger: RecoveryTrigger::HttpStatus {
                    service_id: "web".to_string(),
                    status_codes: vec![500],
                },
                actions: vec![
                    echo_action(false),
                    RecoveryAction::CheckService {
                        service_id: "web".to_string(),
                        timeout_seconds: 2,
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
            manager,
            CommandExecutor::new(),
            log_manager.clone(),
            RecoveryState::new(ActiveRecoveryTracker::new()),
        );

        let cond = DetectedCondition {
            service_id: "web".to_string(),
            trigger: DetectedTrigger::HttpStatus { status_code: 500 },
            details: "HTTP 500".to_string(),
        };

        let first = engine.handle_condition(&cond).await;
        assert_eq!(
            first,
            RecoveryOutcome::Succeeded {
                rule_id: "recover-web".to_string(),
                attempt_count: 1,
                should_reload_webview: true,
            }
        );

        let events = log_manager.query_recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, RecoveryEventStatus::Succeeded);
        assert!(events[0].webview_reloaded);
        assert_eq!(log_manager.query_executions_for_event(events[0].id).unwrap().len(), 1);

        // Immediate second trigger must be suppressed by cooldown.
        let second = engine.handle_condition(&cond).await;
        assert_eq!(
            second,
            RecoveryOutcome::SkippedCooldown {
                rule_id: "recover-web".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn exhausts_retries_from_beginning_and_never_reloads_when_check_fails() {
        let closed_port = unused_port().await;
        let log_manager = LogManager::open_in_memory().unwrap();
        let manager = Arc::new(
            ServiceManager::new(
                vec![ServiceDefinition {
                    id: "down-svc".to_string(),
                    name: "Down Svc".to_string(),
                    required: true,
                    check: ServiceCheck::Tcp {
                        host: "127.0.0.1".to_string(),
                        port: closed_port,
                        timeout_seconds: 1,
                    },
                    commands: ServiceCommands::default(),
                }],
                CommandExecutor::new(),
            )
            .unwrap(),
        );

        let engine = RecoveryEngine::new(
            vec![RecoveryRule {
                id: "retry-fail".to_string(),
                name: "Retry Fail".to_string(),
                enabled: true,
                trigger: RecoveryTrigger::TcpUnavailable {
                    service_id: "down-svc".to_string(),
                },
                actions: vec![
                    fail_action(false),
                    RecoveryAction::ReloadApplication { enabled: true },
                ],
                retry: RetryPolicy {
                    max_attempts: 2,
                    delay_seconds: 0,
                },
                cooldown_seconds: 60,
            }],
            true,
            manager,
            CommandExecutor::new(),
            log_manager.clone(),
            RecoveryState::new(ActiveRecoveryTracker::new()),
        );

        let outcome = engine
            .handle_condition(&DetectedCondition {
                service_id: "down-svc".to_string(),
                trigger: DetectedTrigger::TcpUnavailable,
                details: "tcp down".to_string(),
            })
            .await;

        assert_eq!(
            outcome,
            RecoveryOutcome::Failed {
                rule_id: "retry-fail".to_string(),
                attempt_count: 2,
            }
        );

        let events = log_manager.query_recent_events(10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, RecoveryEventStatus::Failed);
        assert!(!events[0].webview_reloaded);
        assert_eq!(events[0].attempt_count, 2);
        // Each of the 2 attempts restarted from the beginning and logged its command execution.
        assert_eq!(log_manager.query_executions_for_event(events[0].id).unwrap().len(), 2);
    }

    #[tokio::test]
    async fn prevents_overlapping_recoveries_for_same_service() {
        let port = spawn_ok_server().await;
        let tracker = ActiveRecoveryTracker::new();
        let manager = Arc::new(
            ServiceManager::new(
                vec![ServiceDefinition {
                    id: "web".to_string(),
                    name: "Web".to_string(),
                    required: true,
                    check: ServiceCheck::Http {
                        url: format!("http://127.0.0.1:{port}"),
                        expected_status: vec![200],
                        timeout_seconds: 2,
                    },
                    commands: ServiceCommands::default(),
                }],
                CommandExecutor::new(),
            )
            .unwrap(),
        );

        let engine = RecoveryEngine::new(
            vec![RecoveryRule {
                id: "recover-web".to_string(),
                name: "Recover Web".to_string(),
                enabled: true,
                trigger: RecoveryTrigger::ConnectionRefused {
                    service_id: "web".to_string(),
                },
                actions: vec![echo_action(false)],
                retry: RetryPolicy {
                    max_attempts: 1,
                    delay_seconds: 0,
                },
                cooldown_seconds: 0,
            }],
            true,
            manager,
            CommandExecutor::new(),
            LogManager::open_in_memory().unwrap(),
            RecoveryState::new(tracker.clone()),
        );

        // Simulate an active recovery already holding the service.
        assert!(tracker.try_acquire("web"));
        let outcome = engine
            .handle_condition(&DetectedCondition {
                service_id: "web".to_string(),
                trigger: DetectedTrigger::ConnectionRefused,
                details: "refused".to_string(),
            })
            .await;

        assert_eq!(
            outcome,
            RecoveryOutcome::SkippedAlreadyActive {
                service_id: "web".to_string(),
            }
        );
        tracker.release("web");
    }
}
