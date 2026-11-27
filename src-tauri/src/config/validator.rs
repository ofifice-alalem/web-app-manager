//! Semantic validation for loaded configuration files.

use std::collections::HashSet;
use std::fmt;

use crate::browser::NavigationPolicy;

use super::models::{
    AppConfigBundle, AppConfigFile, CommandDefinition, PlatformCommandSpec, RecoveryAction,
    RecoveryRulesConfigFile, RecoveryTrigger, ServiceCheck, ServicesConfigFile, TargetOs,
};

/// Single structured diagnostic entry describing a configuration error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub file: String,
    pub field: String,
    pub message: String,
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.file, self.field, self.message)
    }
}

/// Validates all three parsed configuration files together and returns a unified bundle.
pub fn validate_bundle(
    app_file: AppConfigFile,
    services_file: ServicesConfigFile,
    rules_file: RecoveryRulesConfigFile,
    target_os: TargetOs,
) -> Result<AppConfigBundle, Vec<ConfigDiagnostic>> {
    let mut diagnostics = Vec::new();

    validate_app(&app_file, &mut diagnostics);
    let known_services = validate_services(&services_file, target_os, &mut diagnostics);
    validate_rules(&rules_file, &known_services, target_os, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(AppConfigBundle {
            app: app_file.application,
            services: services_file.services,
            rules: rules_file.rules,
        })
    } else {
        Err(diagnostics)
    }
}

fn validate_app(app_file: &AppConfigFile, diagnostics: &mut Vec<ConfigDiagnostic>) {
    let app = &app_file.application;
    if app.id.trim().is_empty() {
        diagnostics.push(ConfigDiagnostic {
            file: "app.json".to_string(),
            field: "application.id".to_string(),
            message: "Application id must not be empty".to_string(),
        });
    }
    if app.name.trim().is_empty() {
        diagnostics.push(ConfigDiagnostic {
            file: "app.json".to_string(),
            field: "application.name".to_string(),
            message: "Application name must not be empty".to_string(),
        });
    }
    if let Err(err) = NavigationPolicy::validate_http_url(&app.url) {
        diagnostics.push(ConfigDiagnostic {
            file: "app.json".to_string(),
            field: "application.url".to_string(),
            message: err.to_string(),
        });
    }
    if app.startup_timeout_seconds == 0 {
        diagnostics.push(ConfigDiagnostic {
            file: "app.json".to_string(),
            field: "application.startup_timeout_seconds".to_string(),
            message: "startup_timeout_seconds must be greater than 0".to_string(),
        });
    }
}

fn validate_services(
    services_file: &ServicesConfigFile,
    target_os: TargetOs,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> HashSet<String> {
    let mut seen_ids = HashSet::new();

    if services_file.services.is_empty() {
        diagnostics.push(ConfigDiagnostic {
            file: "services.json".to_string(),
            field: "services".to_string(),
            message: "At least one service must be configured".to_string(),
        });
    }

    for (idx, svc) in services_file.services.iter().enumerate() {
        let prefix = format!("services[{idx}]");
        if svc.id.trim().is_empty() {
            diagnostics.push(ConfigDiagnostic {
                file: "services.json".to_string(),
                field: format!("{prefix}.id"),
                message: "Service id must not be empty".to_string(),
            });
        } else if !seen_ids.insert(svc.id.clone()) {
            diagnostics.push(ConfigDiagnostic {
                file: "services.json".to_string(),
                field: format!("{prefix}.id"),
                message: format!("Duplicate service id '{}'", svc.id),
            });
        }

        if svc.name.trim().is_empty() {
            diagnostics.push(ConfigDiagnostic {
                file: "services.json".to_string(),
                field: format!("{prefix}.name"),
                message: "Service name must not be empty".to_string(),
            });
        }

        validate_service_check(&svc.check, &format!("{prefix}.check"), diagnostics);

        for (cmd_name, cmd_opt) in [
            ("start", svc.commands.start.as_ref()),
            ("stop", svc.commands.stop.as_ref()),
            ("restart", svc.commands.restart.as_ref()),
        ] {
            if let Some(cmd_def) = cmd_opt {
                validate_command_def(
                    cmd_def,
                    target_os,
                    "services.json",
                    &format!("{prefix}.commands.{cmd_name}"),
                    diagnostics,
                );
            }
        }
    }

    seen_ids
}

fn validate_service_check(
    check: &ServiceCheck,
    field_prefix: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    if check.timeout_seconds() == 0 {
        diagnostics.push(ConfigDiagnostic {
            file: "services.json".to_string(),
            field: format!("{field_prefix}.timeout_seconds"),
            message: "timeout_seconds must be greater than 0".to_string(),
        });
    }

    match check {
        ServiceCheck::Http {
            url,
            expected_status,
            ..
        } => {
            if let Err(err) = NavigationPolicy::validate_http_url(url) {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.url"),
                    message: err.to_string(),
                });
            }
            if expected_status.is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.expected_status"),
                    message: "expected_status must contain at least one HTTP status code".to_string(),
                });
            }
            for &code in expected_status {
                if !(100..=599).contains(&code) {
                    diagnostics.push(ConfigDiagnostic {
                        file: "services.json".to_string(),
                        field: format!("{field_prefix}.expected_status"),
                        message: format!("Invalid HTTP status code: {code}"),
                    });
                }
            }
        }
        ServiceCheck::Tcp { host, port, .. } => {
            if host.trim().is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.host"),
                    message: "TCP host must not be empty".to_string(),
                });
            }
            if *port == 0 {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.port"),
                    message: "TCP port must be between 1 and 65535".to_string(),
                });
            }
        }
        ServiceCheck::Process { process_name, .. } => {
            if process_name.trim().is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.process_name"),
                    message: "process_name must not be empty".to_string(),
                });
            }
        }
        ServiceCheck::Docker { container_name, .. } => {
            if container_name.trim().is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    file: "services.json".to_string(),
                    field: format!("{field_prefix}.container_name"),
                    message: "container_name must not be empty".to_string(),
                });
            }
        }
    }
}

fn validate_command_spec(
    spec: &PlatformCommandSpec,
    file: &str,
    field_prefix: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    if spec.executable.trim().is_empty() {
        diagnostics.push(ConfigDiagnostic {
            file: file.to_string(),
            field: format!("{field_prefix}.executable"),
            message: "executable must not be empty".to_string(),
        });
    }
    if spec.timeout_seconds == Some(0) {
        diagnostics.push(ConfigDiagnostic {
            file: file.to_string(),
            field: format!("{field_prefix}.timeout_seconds"),
            message: "timeout_seconds must be greater than 0".to_string(),
        });
    }
}

fn validate_command_def(
    cmd_def: &CommandDefinition,
    target_os: TargetOs,
    file: &str,
    field_prefix: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    match cmd_def {
        CommandDefinition::Direct(spec) => {
            validate_command_spec(spec, file, field_prefix, diagnostics);
        }
        CommandDefinition::PlatformSpecific { platform_commands } => {
            if let Some(win) = &platform_commands.windows {
                validate_command_spec(win, file, &format!("{field_prefix}.windows"), diagnostics);
            }
            if let Some(lin) = &platform_commands.linux {
                validate_command_spec(lin, file, &format!("{field_prefix}.linux"), diagnostics);
            }
            if cmd_def.resolve_for_os(target_os).is_none()
                && platform_commands.windows.is_none()
                && platform_commands.linux.is_none()
            {
                diagnostics.push(ConfigDiagnostic {
                    file: file.to_string(),
                    field: format!("{field_prefix}.platform_commands"),
                    message: "platform_commands must define at least one platform entry".to_string(),
                });
            }
        }
    }
}

fn validate_rules(
    rules_file: &RecoveryRulesConfigFile,
    known_services: &HashSet<String>,
    target_os: TargetOs,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    let mut seen_rule_ids = HashSet::new();

    for (idx, rule) in rules_file.rules.iter().enumerate() {
        let prefix = format!("rules[{idx}]");
        if rule.id.trim().is_empty() {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.id"),
                message: "Rule id must not be empty".to_string(),
            });
        } else if !seen_rule_ids.insert(rule.id.clone()) {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.id"),
                message: format!("Duplicate rule id '{}'", rule.id),
            });
        }

        if rule.name.trim().is_empty() {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.name"),
                message: "Rule name must not be empty".to_string(),
            });
        }

        let trigger_service = rule.trigger.service_id();
        if !known_services.contains(trigger_service) {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.trigger.service_id"),
                message: format!("Unknown service_id '{trigger_service}' not found in services.json"),
            });
        }

        if let RecoveryTrigger::HttpStatus { status_codes, .. } = &rule.trigger {
            if status_codes.is_empty() {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: format!("{prefix}.trigger.status_codes"),
                    message: "http_status trigger requires at least one status code".to_string(),
                });
            }
            for &code in status_codes {
                if !(100..=599).contains(&code) {
                    diagnostics.push(ConfigDiagnostic {
                        file: "recovery-rules.json".to_string(),
                        field: format!("{prefix}.trigger.status_codes"),
                        message: format!("Invalid HTTP status code: {code}"),
                    });
                }
            }
        }

        if rule.actions.is_empty() {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.actions"),
                message: "Rule must define at least one action".to_string(),
            });
        }

        for (action_idx, action) in rule.actions.iter().enumerate() {
            let a_prefix = format!("{prefix}.actions[{action_idx}]");
            validate_action(action, known_services, target_os, &a_prefix, diagnostics);
        }

        if rule.retry.max_attempts == 0 {
            diagnostics.push(ConfigDiagnostic {
                file: "recovery-rules.json".to_string(),
                field: format!("{prefix}.retry.max_attempts"),
                message: "retry.max_attempts must be at least 1".to_string(),
            });
        }
    }
}

fn validate_action(
    action: &RecoveryAction,
    known_services: &HashSet<String>,
    target_os: TargetOs,
    field_prefix: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    match action {
        RecoveryAction::Command {
            executable,
            platform_commands,
            timeout_seconds,
            ..
        } => {
            if *timeout_seconds == 0 {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: format!("{field_prefix}.timeout_seconds"),
                    message: "timeout_seconds must be greater than 0".to_string(),
                });
            }
            let has_direct = executable
                .as_ref()
                .map(|e| !e.trim().is_empty())
                .unwrap_or(false);
            let has_platform = platform_commands
                .as_ref()
                .and_then(|pm| match target_os {
                    TargetOs::Windows => pm.windows.as_ref(),
                    TargetOs::Linux => pm.linux.as_ref(),
                })
                .map(|spec| !spec.executable.trim().is_empty())
                .unwrap_or(false);

            if !has_direct && !has_platform {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: field_prefix.to_string(),
                    message: "command action must provide a non-empty executable or platform_commands entry".to_string(),
                });
            }
        }
        RecoveryAction::StartService {
            service_id,
            timeout_seconds,
            ..
        }
        | RecoveryAction::RestartService {
            service_id,
            timeout_seconds,
            ..
        }
        | RecoveryAction::CheckService {
            service_id,
            timeout_seconds,
        } => {
            if !known_services.contains(service_id) {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: format!("{field_prefix}.service_id"),
                    message: format!("Unknown service_id '{service_id}' not found in services.json"),
                });
            }
            if *timeout_seconds == 0 {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: format!("{field_prefix}.timeout_seconds"),
                    message: "timeout_seconds must be greater than 0".to_string(),
                });
            }
        }
        RecoveryAction::Wait { seconds } => {
            if *seconds == 0 {
                diagnostics.push(ConfigDiagnostic {
                    file: "recovery-rules.json".to_string(),
                    field: format!("{field_prefix}.seconds"),
                    message: "wait seconds must be greater than 0".to_string(),
                });
            }
        }
        RecoveryAction::ReloadApplication { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::{
        ApplicationConfig, PlatformCommandMap, RetryPolicy, ServiceCommands, ServiceDefinition,
        ServiceStatusTriggerValue,
    };

    fn sample_valid_files() -> (AppConfigFile, ServicesConfigFile, RecoveryRulesConfigFile) {
        let app = AppConfigFile {
            application: ApplicationConfig {
                id: "sales-system".to_string(),
                name: "Sales System".to_string(),
                url: "http://127.0.0.1:8000".to_string(),
                startup_timeout_seconds: 60,
                reload_after_recovery: true,
            },
        };
        let services = ServicesConfigFile {
            services: vec![ServiceDefinition {
                id: "sales-app".to_string(),
                name: "Sales App".to_string(),
                required: true,
                check: ServiceCheck::Http {
                    url: "http://127.0.0.1:8000".to_string(),
                    expected_status: vec![200, 302],
                    timeout_seconds: 5,
                },
                commands: ServiceCommands {
                    start: Some(CommandDefinition::PlatformSpecific {
                        platform_commands: PlatformCommandMap {
                            windows: Some(PlatformCommandSpec {
                                executable: "docker.exe".to_string(),
                                args: vec!["compose".to_string(), "up".to_string(), "-d".to_string()],
                                working_dir: None,
                                timeout_seconds: Some(30),
                            }),
                            linux: Some(PlatformCommandSpec {
                                executable: "docker".to_string(),
                                args: vec!["compose".to_string(), "up".to_string(), "-d".to_string()],
                                working_dir: None,
                                timeout_seconds: Some(30),
                            }),
                        },
                    }),
                    stop: None,
                    restart: None,
                },
            }],
        };
        let rules = RecoveryRulesConfigFile {
            rules: vec![crate::config::models::RecoveryRule {
                id: "restart-sales".to_string(),
                name: "Restart Sales".to_string(),
                enabled: true,
                trigger: RecoveryTrigger::ServiceStatus {
                    service_id: "sales-app".to_string(),
                    value: ServiceStatusTriggerValue::Stopped,
                },
                actions: vec![RecoveryAction::StartService {
                    service_id: "sales-app".to_string(),
                    timeout_seconds: 30,
                    continue_on_failure: false,
                }],
                retry: RetryPolicy {
                    max_attempts: 3,
                    delay_seconds: 5,
                },
                cooldown_seconds: 60,
            }],
        };
        (app, services, rules)
    }

    #[test]
    fn validates_complete_valid_bundle() {
        let (app, services, rules) = sample_valid_files();
        let bundle = validate_bundle(app, services, rules, TargetOs::Linux)
            .expect("valid bundle should pass");
        assert_eq!(bundle.app.id, "sales-system");
        assert_eq!(bundle.services.len(), 1);
        assert_eq!(bundle.rules.len(), 1);
    }

    #[test]
    fn resolves_platform_commands_for_windows_and_linux() {
        let (_, services, _) = sample_valid_files();
        let start_cmd = services.services[0].commands.start.as_ref().unwrap();
        assert_eq!(
            start_cmd.resolve_for_os(TargetOs::Windows).unwrap().executable,
            "docker.exe"
        );
        assert_eq!(
            start_cmd.resolve_for_os(TargetOs::Linux).unwrap().executable,
            "docker"
        );
    }

    #[test]
    fn rejects_dangling_service_id_and_invalid_bounds() {
        let (mut app, services, mut rules) = sample_valid_files();
        app.application.startup_timeout_seconds = 0;
        rules.rules[0].retry.max_attempts = 0;
        rules.rules[0].trigger = RecoveryTrigger::ConnectionRefused {
            service_id: "non-existent".to_string(),
        };

        let errors = validate_bundle(app, services, rules, TargetOs::Windows)
            .expect_err("should fail validation");
        assert!(errors.iter().any(|e| e.field == "application.startup_timeout_seconds"));
        assert!(errors.iter().any(|e| e.field == "rules[0].retry.max_attempts"));
        assert!(errors.iter().any(|e| e.field == "rules[0].trigger.service_id"));
    }

    #[test]
    fn rejects_duplicate_service_ids() {
        let (app, mut services, rules) = sample_valid_files();
        let dup = services.services[0].clone();
        services.services.push(dup);

        let errors = validate_bundle(app, services, rules, TargetOs::Linux)
            .expect_err("duplicate service id should fail");
        assert!(errors.iter().any(|e| e.message.contains("Duplicate service id")));
    }
}
