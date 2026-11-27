//! Strongly typed configuration models matching `app.json`, `services.json`,
//! and `recovery-rules.json`.

use serde::{Deserialize, Serialize};

/// Supported target operating systems for platform-specific commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    Linux,
}

impl TargetOs {
    /// Returns the operating system the binary was compiled for.
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::Linux
        }
    }
}

/// Top-level structure of `config/app.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfigFile {
    pub application: ApplicationConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub startup_timeout_seconds: u64,
    pub reload_after_recovery: bool,
}

/// Top-level structure of `config/services.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicesConfigFile {
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDefinition {
    pub id: String,
    pub name: String,
    pub required: bool,
    pub check: ServiceCheck,
    pub commands: ServiceCommands,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceCheck {
    Http {
        url: String,
        expected_status: Vec<u16>,
        timeout_seconds: u64,
    },
    Tcp {
        host: String,
        port: u16,
        timeout_seconds: u64,
    },
    Process {
        process_name: String,
        timeout_seconds: u64,
    },
    Docker {
        container_name: String,
        timeout_seconds: u64,
    },
}

impl ServiceCheck {
    pub fn timeout_seconds(&self) -> u64 {
        match self {
            Self::Http { timeout_seconds, .. }
            | Self::Tcp { timeout_seconds, .. }
            | Self::Process { timeout_seconds, .. }
            | Self::Docker { timeout_seconds, .. } => *timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceCommands {
    #[serde(default)]
    pub start: Option<CommandDefinition>,
    #[serde(default)]
    pub stop: Option<CommandDefinition>,
    #[serde(default)]
    pub restart: Option<CommandDefinition>,
}

/// Concrete command specification with separate executable and argument list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCommandSpec {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformCommandMap {
    #[serde(default)]
    pub windows: Option<PlatformCommandSpec>,
    #[serde(default)]
    pub linux: Option<PlatformCommandSpec>,
}

/// Supports either OS-keyed `platform_commands` or a direct command specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandDefinition {
    PlatformSpecific {
        platform_commands: PlatformCommandMap,
    },
    Direct(PlatformCommandSpec),
}

impl CommandDefinition {
    /// Resolves the command specification for the given target OS.
    pub fn resolve_for_os(&self, os: TargetOs) -> Option<&PlatformCommandSpec> {
        match self {
            Self::Direct(spec) => Some(spec),
            Self::PlatformSpecific { platform_commands } => match os {
                TargetOs::Windows => platform_commands.windows.as_ref(),
                TargetOs::Linux => platform_commands.linux.as_ref(),
            },
        }
    }
}

/// Top-level structure of `config/recovery-rules.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRulesConfigFile {
    pub rules: Vec<RecoveryRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: RecoveryTrigger,
    pub actions: Vec<RecoveryAction>,
    pub retry: RetryPolicy,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatusTriggerValue {
    Stopped,
    Unhealthy,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryTrigger {
    HttpStatus {
        service_id: String,
        status_codes: Vec<u16>,
    },
    ConnectionRefused {
        service_id: String,
    },
    ServiceStatus {
        service_id: String,
        value: ServiceStatusTriggerValue,
    },
    TcpUnavailable {
        service_id: String,
    },
    ProcessNotRunning {
        service_id: String,
    },
}

impl RecoveryTrigger {
    pub fn service_id(&self) -> &str {
        match self {
            Self::HttpStatus { service_id, .. }
            | Self::ConnectionRefused { service_id }
            | Self::ServiceStatus { service_id, .. }
            | Self::TcpUnavailable { service_id }
            | Self::ProcessNotRunning { service_id } => service_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryAction {
    Command {
        #[serde(default)]
        executable: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        working_dir: Option<String>,
        #[serde(default)]
        platform_commands: Option<PlatformCommandMap>,
        timeout_seconds: u64,
        #[serde(default)]
        continue_on_failure: bool,
    },
    StartService {
        service_id: String,
        timeout_seconds: u64,
        #[serde(default)]
        continue_on_failure: bool,
    },
    RestartService {
        service_id: String,
        timeout_seconds: u64,
        #[serde(default)]
        continue_on_failure: bool,
    },
    Wait {
        seconds: u64,
    },
    CheckService {
        service_id: String,
        timeout_seconds: u64,
    },
    ReloadApplication {
        enabled: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub delay_seconds: u64,
}

/// Complete validated configuration bundle loaded on application startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfigBundle {
    pub app: ApplicationConfig,
    pub services: Vec<ServiceDefinition>,
    pub rules: Vec<RecoveryRule>,
}
