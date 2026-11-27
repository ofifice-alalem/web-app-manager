//! Configuration management module for loading and validating
//! `app.json`, `services.json`, and `recovery-rules.json`.

pub mod loader;
pub mod models;
pub mod validator;

pub use loader::{load_config_bundle_from_dir, resolve_config_dir, ConfigError};
pub use models::{
    AppConfigBundle, ApplicationConfig, CommandDefinition, PlatformCommandMap, PlatformCommandSpec,
    RecoveryAction, RecoveryRule, RecoveryTrigger, RetryPolicy, ServiceCheck, ServiceCommands,
    ServiceDefinition, ServiceStatusTriggerValue, TargetOs,
};
pub use validator::{validate_bundle, ConfigDiagnostic};
