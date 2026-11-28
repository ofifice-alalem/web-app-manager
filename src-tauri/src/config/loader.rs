//! Secure configuration directory resolution, permission checks, and JSON loading.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use super::models::{
    AppConfigBundle, AppConfigFile, RecoveryRulesConfigFile, ServicesConfigFile, TargetOs,
};
use super::validator::{validate_bundle, ConfigDiagnostic};

pub const CONFIG_DIR_ENV: &str = "DESKTOP_HOST_CONFIG_DIR";
const DEFAULT_APP_DIR_NAME: &str = "desktop-application-host";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration directory not found: {0}")]
    ConfigDirNotFound(String),

    #[error("Failed to read configuration file '{path}': {source}")]
    IoError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Insecure configuration file '{path}': {reason}")]
    InsecurePermissions { path: String, reason: String },

    #[error("Failed to parse JSON in '{file}': {source}")]
    ParseError {
        file: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Configuration validation failed with {} error(s)", .0.len())]
    ValidationFailed(Vec<ConfigDiagnostic>),
}

/// Resolves the configuration directory using protected OS paths first,
/// falling back to the local workspace `config/` directory in debug builds.
pub fn resolve_config_dir() -> Result<PathBuf, ConfigError> {
    if let Ok(explicit_dir) = std::env::var(CONFIG_DIR_ENV) {
        let path = PathBuf::from(explicit_dir);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(ConfigError::ConfigDirNotFound(path.display().to_string()));
    }

    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    if let Ok(program_data) = std::env::var("ProgramData") {
        candidates.push(
            PathBuf::from(program_data)
                .join(DEFAULT_APP_DIR_NAME)
                .join("config"),
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        candidates.push(PathBuf::from("/etc").join(DEFAULT_APP_DIR_NAME));
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(home).join(".config").join(DEFAULT_APP_DIR_NAME));
        }
    }

    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from("config"));
        candidates.push(PathBuf::from("../config"));
    }

    for candidate in &candidates {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }

    let searched = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ConfigError::ConfigDirNotFound(searched))
}

/// Verifies that a configuration file is a regular file (not a symlink)
/// and does not have unsafe world-writable permissions.
pub fn verify_file_security(path: &Path) -> Result<(), ConfigError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| ConfigError::IoError {
        path: path.display().to_string(),
        source,
    })?;

    if metadata.file_type().is_symlink() {
        return Err(ConfigError::InsecurePermissions {
            path: path.display().to_string(),
            reason: "Configuration files must not be symbolic links".to_string(),
        });
    }

    if !metadata.is_file() {
        return Err(ConfigError::InsecurePermissions {
            path: path.display().to_string(),
            reason: "Configuration path is not a regular file".to_string(),
        });
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o002 != 0 {
            return Err(ConfigError::InsecurePermissions {
                path: path.display().to_string(),
                reason: "Configuration file must not be world-writable".to_string(),
            });
        }
    }

    Ok(())
}

fn read_and_parse<T: serde::de::DeserializeOwned>(
    dir: &Path,
    filename: &str,
) -> Result<T, ConfigError> {
    let path = dir.join(filename);
    verify_file_security(&path)?;

    let raw = fs::read_to_string(&path).map_err(|source| ConfigError::IoError {
        path: path.display().to_string(),
        source,
    })?;

    serde_json::from_str::<T>(&raw).map_err(|source| ConfigError::ParseError {
        file: filename.to_string(),
        source,
    })
}

/// Loads, security-checks, parses, and validates all three configuration files.
pub fn load_config_bundle_from_dir(dir: &Path) -> Result<AppConfigBundle, ConfigError> {
    let app_file: AppConfigFile = read_and_parse(dir, "app.json")?;
    let services_file: ServicesConfigFile = read_and_parse(dir, "services.json")?;
    let rules_file: RecoveryRulesConfigFile = read_and_parse(dir, "recovery-rules.json")?;

    validate_bundle(app_file, services_file, rules_file, TargetOs::current())
        .map_err(ConfigError::ValidationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_config_dir(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("desktop-host-cfg-{suffix}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_valid_configs(dir: &Path) {
        fs::write(
            dir.join("app.json"),
            r#"{"application":{"id":"sales","name":"Sales","url":"http://127.0.0.1:8000","startup_timeout_seconds":60,"reload_after_recovery":true}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("services.json"),
            r#"{"services":[{"id":"sales-app","name":"Sales App","required":true,"check":{"type":"tcp","host":"127.0.0.1","port":8000,"timeout_seconds":5},"commands":{}}]}"#,
        )
        .unwrap();
        fs::write(dir.join("recovery-rules.json"), r#"{"rules":[]}"#).unwrap();
    }

    #[test]
    fn loads_valid_bundle_from_directory() {
        let dir = create_temp_config_dir("valid");
        write_valid_configs(&dir);

        let bundle = load_config_bundle_from_dir(&dir).expect("valid config bundle");
        assert_eq!(bundle.app.id, "sales");
        assert_eq!(bundle.services.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unknown_json_fields() {
        let dir = create_temp_config_dir("unknown");
        write_valid_configs(&dir);
        fs::write(
            dir.join("app.json"),
            r#"{"application":{"id":"sales","name":"Sales","url":"http://127.0.0.1:8000","startup_timeout_seconds":60,"reload_after_recovery":true,"unexpected":123}}"#,
        )
        .unwrap();

        let err = load_config_bundle_from_dir(&dir).expect_err("unknown field should fail");
        assert!(matches!(err, ConfigError::ParseError { .. }));

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_world_writable_config_file_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = create_temp_config_dir("perms");
        write_valid_configs(&dir);
        let app_path = dir.join("app.json");
        fs::set_permissions(&app_path, fs::Permissions::from_mode(0o666)).unwrap();

        let err = load_config_bundle_from_dir(&dir).expect_err("world-writable should fail");
        assert!(matches!(err, ConfigError::InsecurePermissions { .. }));

        let _ = fs::remove_dir_all(&dir);
    }
}
