//! ServiceManager orchestrating configured health checks and lifecycle commands.

use reqwest::redirect::Policy;

use crate::config::{CommandDefinition, ServiceCheck, ServiceDefinition, TargetOs};
use crate::executor::{CommandExecutor, CommandSpec};

use super::checks::{docker, http, process, tcp};
use super::models::{
    ServiceCheckResult, ServiceHealthState, ServiceLifecycleAction, ServiceLifecycleOutcome,
    SystemReadinessReport,
};

const DEFAULT_LIFECYCLE_TIMEOUT_SECONDS: u64 = 30;

/// Evaluates service health and executes configured start/stop/restart commands.
#[derive(Debug, Clone)]
pub struct ServiceManager {
    services: Vec<ServiceDefinition>,
    executor: CommandExecutor,
    http_client: reqwest::Client,
}

impl ServiceManager {
    /// Creates a `ServiceManager` from validated `services.json` definitions.
    pub fn new(services: Vec<ServiceDefinition>, executor: CommandExecutor) -> Result<Self, String> {
        let http_client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|err| format!("Failed to initialize HTTP health check client: {err}"))?;

        Ok(Self {
            services,
            executor,
            http_client,
        })
    }

    pub fn services(&self) -> &[ServiceDefinition] {
        &self.services
    }

    pub fn find_service(&self, service_id: &str) -> Option<&ServiceDefinition> {
        self.services.iter().find(|svc| svc.id == service_id)
    }

    /// Runs the configured health check for a single service.
    pub async fn check_service(&self, service_id: &str) -> Result<ServiceCheckResult, String> {
        let service = self
            .find_service(service_id)
            .ok_or_else(|| format!("Unknown service_id '{service_id}'"))?;

        Ok(self.run_check_for(service).await)
    }

    /// Evaluates all configured services and computes overall system readiness.
    /// Optional (`required: false`) service failures do not mark `is_ready` as `false`.
    pub async fn check_all_services(&self) -> SystemReadinessReport {
        let mut results = Vec::with_capacity(self.services.len());
        for service in &self.services {
            results.push(self.run_check_for(service).await);
        }

        let is_ready = results
            .iter()
            .filter(|res| res.required)
            .all(|res| res.state.is_healthy());

        SystemReadinessReport { is_ready, results }
    }

    /// Executes a configured `start`, `stop`, or `restart` command for `service_id`,
    /// then re-checks the service status.
    pub async fn execute_lifecycle_action(
        &self,
        service_id: &str,
        action: ServiceLifecycleAction,
        override_timeout_seconds: Option<u64>,
    ) -> Result<ServiceLifecycleOutcome, String> {
        let service = self
            .find_service(service_id)
            .ok_or_else(|| format!("Unknown service_id '{service_id}'"))?;

        let cmd_def_opt: Option<&CommandDefinition> = match action {
            ServiceLifecycleAction::Start => service.commands.start.as_ref(),
            ServiceLifecycleAction::Stop => service.commands.stop.as_ref(),
            ServiceLifecycleAction::Restart => service.commands.restart.as_ref(),
        };

        let Some(cmd_def) = cmd_def_opt else {
            return Ok(ServiceLifecycleOutcome {
                service_id: service.id.clone(),
                action,
                command_executed: false,
                command_result: None,
                post_check: None,
                success: false,
                message: format!(
                    "No '{action:?}' command configured for service '{}'",
                    service.id
                ),
            });
        };

        let Some(platform_spec) = cmd_def.resolve_for_os(TargetOs::current()) else {
            return Ok(ServiceLifecycleOutcome {
                service_id: service.id.clone(),
                action,
                command_executed: false,
                command_result: None,
                post_check: None,
                success: false,
                message: format!(
                    "No '{action:?}' command configured for current OS on service '{}'",
                    service.id
                ),
            });
        };

        let cmd_spec = CommandSpec::from_platform_spec(
            platform_spec,
            override_timeout_seconds.unwrap_or(DEFAULT_LIFECYCLE_TIMEOUT_SECONDS),
        );
        let cmd_result = self.executor.execute(&cmd_spec).await;

        let post_check = self.run_check_for(service).await;
        let check_matches_goal = match action {
            ServiceLifecycleAction::Start | ServiceLifecycleAction::Restart => {
                post_check.state.is_healthy()
            }
            ServiceLifecycleAction::Stop => matches!(
                post_check.state,
                ServiceHealthState::Stopped | ServiceHealthState::ConnectionRefused
            ),
        };

        let success = cmd_result.success && check_matches_goal;
        let message = if success {
            format!("Service '{}' {:?} succeeded", service.id, action)
        } else if !cmd_result.success {
            cmd_result
                .error_message
                .clone()
                .unwrap_or_else(|| format!("Service '{}' {:?} command failed", service.id, action))
        } else {
            format!(
                "Service '{}' {:?} command ran, but post-check state is {:?}",
                service.id, action, post_check.state
            )
        };

        Ok(ServiceLifecycleOutcome {
            service_id: service.id.clone(),
            action,
            command_executed: true,
            command_result: Some(cmd_result),
            post_check: Some(post_check),
            success,
            message,
        })
    }

    async fn run_check_for(&self, service: &ServiceDefinition) -> ServiceCheckResult {
        match &service.check {
            ServiceCheck::Http {
                url,
                expected_status,
                timeout_seconds,
            } => http::check_http(&self.http_client, service, url, expected_status, *timeout_seconds).await,
            ServiceCheck::Tcp {
                host,
                port,
                timeout_seconds,
            } => tcp::check_tcp(service, host, *port, *timeout_seconds).await,
            ServiceCheck::Process {
                process_name,
                timeout_seconds,
            } => process::check_process(service, process_name, *timeout_seconds).await,
            ServiceCheck::Docker {
                container_name,
                timeout_seconds,
            } => docker::check_docker(&self.executor, service, container_name, *timeout_seconds).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CommandDefinition, PlatformCommandSpec, ServiceCommands};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_http_server(status_line: &str, delay_ms: u64) -> (String, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let status = status_line.to_string();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let status_clone = status.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 512];
                    let _ = socket.read(&mut buf).await;
                    if delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    let resp = format!(
                        "HTTP/1.1 {status_clone}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                });
            }
        });

        (format!("http://127.0.0.1:{port}"), port)
    }

    async fn unused_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    fn http_service(id: &str, url: String, required: bool, timeout_seconds: u64) -> ServiceDefinition {
        ServiceDefinition {
            id: id.to_string(),
            name: id.to_string(),
            required,
            check: ServiceCheck::Http {
                url,
                expected_status: vec![200, 302],
                timeout_seconds,
            },
            commands: ServiceCommands::default(),
        }
    }

    #[tokio::test]
    async fn http_check_handles_expected_status_500_and_timeout() {
        let (url_200, _) = spawn_mock_http_server("200 OK", 0).await;
        let (url_302, _) = spawn_mock_http_server("302 Found", 0).await;
        let (url_500, _) = spawn_mock_http_server("500 Internal Server Error", 0).await;
        let (url_slow, _) = spawn_mock_http_server("200 OK", 2500).await;

        let manager = ServiceManager::new(
            vec![
                http_service("ok", url_200, true, 3),
                http_service("redir", url_302, true, 3),
                http_service("err500", url_500, true, 3),
                http_service("slow", url_slow, true, 1),
            ],
            CommandExecutor::new(),
        )
        .unwrap();

        assert_eq!(manager.check_service("ok").await.unwrap().state, ServiceHealthState::Healthy);
        assert_eq!(manager.check_service("redir").await.unwrap().state, ServiceHealthState::Healthy);

        let res_500 = manager.check_service("err500").await.unwrap();
        assert_eq!(res_500.state, ServiceHealthState::Unhealthy);
        assert_eq!(res_500.http_status, Some(500));

        assert_eq!(manager.check_service("slow").await.unwrap().state, ServiceHealthState::Timeout);
    }

    #[tokio::test]
    async fn tcp_and_http_checks_detect_connection_refused_and_healthy_port() {
        let (_, open_port) = spawn_mock_http_server("200 OK", 0).await;
        let closed_port = unused_port().await;

        let manager = ServiceManager::new(
            vec![
                ServiceDefinition {
                    id: "tcp-open".to_string(),
                    name: "TCP Open".to_string(),
                    required: true,
                    check: ServiceCheck::Tcp {
                        host: "127.0.0.1".to_string(),
                        port: open_port,
                        timeout_seconds: 2,
                    },
                    commands: ServiceCommands::default(),
                },
                ServiceDefinition {
                    id: "tcp-closed".to_string(),
                    name: "TCP Closed".to_string(),
                    required: true,
                    check: ServiceCheck::Tcp {
                        host: "127.0.0.1".to_string(),
                        port: closed_port,
                        timeout_seconds: 5,
                    },
                    commands: ServiceCommands::default(),
                },
                http_service(
                    "http-closed",
                    format!("http://127.0.0.1:{closed_port}"),
                    true,
                    5,
                ),
            ],
            CommandExecutor::new(),
        )
        .unwrap();

        assert_eq!(manager.check_service("tcp-open").await.unwrap().state, ServiceHealthState::Healthy);
        assert_eq!(
            manager.check_service("tcp-closed").await.unwrap().state,
            ServiceHealthState::ConnectionRefused
        );
        assert_eq!(
            manager.check_service("http-closed").await.unwrap().state,
            ServiceHealthState::ConnectionRefused
        );
    }

    #[tokio::test]
    async fn process_check_detects_running_and_missing_processes() {
        let current_exe_name = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let manager = ServiceManager::new(
            vec![
                ServiceDefinition {
                    id: "self-proc".to_string(),
                    name: "Self".to_string(),
                    required: true,
                    check: ServiceCheck::Process {
                        process_name: current_exe_name,
                        timeout_seconds: 5,
                    },
                    commands: ServiceCommands::default(),
                },
                ServiceDefinition {
                    id: "missing-proc".to_string(),
                    name: "Missing".to_string(),
                    required: false,
                    check: ServiceCheck::Process {
                        process_name: "non_existent_process_xyz_999".to_string(),
                        timeout_seconds: 5,
                    },
                    commands: ServiceCommands::default(),
                },
            ],
            CommandExecutor::new(),
        )
        .unwrap();

        let report = manager.check_all_services().await;
        // Required service is healthy and optional service is stopped -> system is ready.
        assert!(report.is_ready);
        assert_eq!(report.results[0].state, ServiceHealthState::Healthy);
        assert_eq!(report.results[1].state, ServiceHealthState::Stopped);
    }

    #[tokio::test]
    async fn lifecycle_action_handles_null_and_valid_commands_with_recheck() {
        let (url_200, _) = spawn_mock_http_server("200 OK", 0).await;

        #[cfg(windows)]
        let echo_cmd = PlatformCommandSpec {
            executable: "powershell.exe".to_string(),
            args: vec!["-NoProfile".to_string(), "-Command".to_string(), "Write-Output 'started'".to_string()],
            working_dir: None,
            timeout_seconds: Some(5),
        };
        #[cfg(not(windows))]
        let echo_cmd = PlatformCommandSpec {
            executable: "echo".to_string(),
            args: vec!["started".to_string()],
            working_dir: None,
            timeout_seconds: Some(5),
        };

        let manager = ServiceManager::new(
            vec![
                ServiceDefinition {
                    id: "with-cmd".to_string(),
                    name: "With Cmd".to_string(),
                    required: true,
                    check: ServiceCheck::Http {
                        url: url_200.clone(),
                        expected_status: vec![200],
                        timeout_seconds: 2,
                    },
                    commands: ServiceCommands {
                        start: Some(CommandDefinition::Direct(echo_cmd)),
                        stop: None,
                        restart: None,
                    },
                },
                http_service("no-cmd", url_200, true, 2),
            ],
            CommandExecutor::new(),
        )
        .unwrap();

        let start_outcome = manager
            .execute_lifecycle_action("with-cmd", ServiceLifecycleAction::Start, None)
            .await
            .unwrap();
        assert!(start_outcome.command_executed);
        assert!(start_outcome.success);
        assert_eq!(
            start_outcome.post_check.unwrap().state,
            ServiceHealthState::Healthy
        );

        let missing_cmd = manager
            .execute_lifecycle_action("no-cmd", ServiceLifecycleAction::Restart, None)
            .await
            .unwrap();
        assert!(!missing_cmd.command_executed);
        assert!(!missing_cmd.success);
    }
}
