//! Backend for the standalone Desktop Host Settings companion application.

use std::fs;
use std::path::{Path, PathBuf};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use url::Url;

const CONFIG_DIR_ENV: &str = "DESKTOP_HOST_CONFIG_DIR";
const HOST_APP_DIR_NAME: &str = "desktop-application-host";
const HOST_IDENTIFIER: &str = "com.desktophost.client";
const LOG_DB_FILENAME: &str = "desktop-host-logs.db";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBundleDto {
    pub config_dir: String,
    pub app_json: String,
    pub services_json: String,
    pub rules_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEventDto {
    pub id: i64,
    pub event_type: String,
    pub rule_id: Option<String>,
    pub service_id: Option<String>,
    pub trigger_type: String,
    pub trigger_details: String,
    pub status: String,
    pub attempt_count: u32,
    pub webview_reloaded: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecutionDto {
    pub id: i64,
    pub executable: String,
    pub arguments: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub status: String,
    pub duration_ms: i64,
    pub executed_at: String,
}

fn resolve_shared_config_dir() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var(CONFIG_DIR_ENV) {
        return Ok(PathBuf::from(explicit));
    }

    #[cfg(target_os = "windows")]
    if let Ok(program_data) = std::env::var("ProgramData") {
        let prod_dir = PathBuf::from(program_data)
            .join(HOST_APP_DIR_NAME)
            .join("config");
        if prod_dir.is_dir() {
            return Ok(prod_dir);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let prod_dir = PathBuf::from("/etc").join(HOST_APP_DIR_NAME);
        if prod_dir.is_dir() {
            return Ok(prod_dir);
        }
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = PathBuf::from(home).join(".config").join(HOST_APP_DIR_NAME);
            if user_dir.is_dir() {
                return Ok(user_dir);
            }
        }
    }

    for dev_candidate in [PathBuf::from("../config"), PathBuf::from("config")] {
        if dev_candidate.is_dir() {
            // If in AppImage, current dir might be inside the read-only squashfs mount.
            // Check if directory is writable:
            if let Ok(metadata) = dev_candidate.metadata() {
                if !metadata.permissions().readonly() {
                    return Ok(dev_candidate);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    if let Ok(program_data) = std::env::var("ProgramData") {
        return Ok(PathBuf::from(program_data)
            .join(HOST_APP_DIR_NAME)
            .join("config"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Ok(PathBuf::from(home).join(".config").join(HOST_APP_DIR_NAME));
        }
        Ok(PathBuf::from("/etc").join(HOST_APP_DIR_NAME))
    }
}

fn read_or_default(dir: &Path, filename: &str, default_json: &str) -> Result<String, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(default_json.to_string());
    }
    fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read '{}': {err}", path.display()))
}

fn resolve_host_db_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let data_dir = app.path().app_data_dir().ok()?;
    let parent = data_dir.parent()?;
    let db_path = parent.join(HOST_IDENTIFIER).join(LOG_DB_FILENAME);
    if db_path.is_file() {
        Some(db_path)
    } else {
        None
    }
}

const DEFAULT_APP_JSON: &str = r#"{
  "application": {
    "id": "sales-system",
    "name": "Sales System",
    "url": "http://127.0.0.1:8000",
    "startup_timeout_seconds": 60,
    "reload_after_recovery": true
  }
}"#;

const DEFAULT_SERVICES_JSON: &str = r#"{
  "services": [
    {
      "id": "sales-app",
      "name": "Sales Application",
      "required": true,
      "check": {
        "type": "http",
        "url": "http://127.0.0.1:8000",
        "expected_status": [200, 301, 302],
        "timeout_seconds": 5
      },
      "commands": {
        "start": {
          "platform_commands": {
            "windows": { "executable": "docker.exe", "args": ["compose", "up", "-d"], "timeout_seconds": 60 },
            "linux": { "executable": "docker", "args": ["compose", "up", "-d"], "timeout_seconds": 60 }
          }
        },
        "stop": null,
        "restart": {
          "platform_commands": {
            "windows": { "executable": "docker.exe", "args": ["compose", "restart", "app"], "timeout_seconds": 45 },
            "linux": { "executable": "docker", "args": ["compose", "restart", "app"], "timeout_seconds": 45 }
          }
        }
      }
    },
    {
      "id": "print-server",
      "name": "Print Server",
      "required": true,
      "check": {
        "type": "http",
        "url": "http://127.0.0.1:9124/health",
        "expected_status": [200],
        "timeout_seconds": 5
      },
      "commands": { "start": null, "stop": null, "restart": null }
    }
  ]
}"#;

const DEFAULT_RULES_JSON: &str = r#"{
  "rules": [
    {
      "id": "sales-container-stopped",
      "name": "Sales Container Stopped",
      "enabled": true,
      "trigger": {
        "type": "service_status",
        "service_id": "sales-app",
        "value": "stopped"
      },
      "actions": [
        { "type": "restart_service", "service_id": "sales-app", "timeout_seconds": 45, "continue_on_failure": false },
        { "type": "wait", "seconds": 3 },
        { "type": "check_service", "service_id": "sales-app", "timeout_seconds": 30 },
        { "type": "reload_application", "enabled": true }
      ],
      "retry": { "max_attempts": 3, "delay_seconds": 5 },
      "cooldown_seconds": 60
    }
  ]
}"#;

#[tauri::command]
fn load_settings_bundle() -> Result<SettingsBundleDto, String> {
    let dir = resolve_shared_config_dir()?;
    let app_json = read_or_default(&dir, "app.json", DEFAULT_APP_JSON)?;
    let services_json = read_or_default(&dir, "services.json", DEFAULT_SERVICES_JSON)?;
    let rules_json = read_or_default(&dir, "recovery-rules.json", DEFAULT_RULES_JSON)?;

    Ok(SettingsBundleDto {
        config_dir: dir.display().to_string(),
        app_json,
        services_json,
        rules_json,
    })
}

#[tauri::command]
fn save_config_file(filename: String, content: String) -> Result<String, String> {
    if !matches!(
        filename.as_str(),
        "app.json" | "services.json" | "recovery-rules.json"
    ) {
        return Err("Unsupported configuration filename".to_string());
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON in {filename}: {e}"))?;

    if filename == "app.json" {
        let app_obj = parsed
            .get("application")
            .ok_or("Missing 'application' root object")?;
        let url_str = app_obj
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'application.url'")?;
        let parsed_url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err("application.url must use http or https".to_string());
        }
        let timeout = app_obj
            .get("startup_timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if timeout == 0 {
            return Err("startup_timeout_seconds must be greater than 0".to_string());
        }
    } else if filename == "services.json" {
        if !parsed.get("services").map(|v| v.is_array()).unwrap_or(false) {
            return Err("services.json must contain a 'services' array".to_string());
        }
    } else if filename == "recovery-rules.json"
        && !parsed.get("rules").map(|v| v.is_array()).unwrap_or(false)
    {
        return Err("recovery-rules.json must contain a 'rules' array".to_string());
    }

    let dir = resolve_shared_config_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config directory '{}': {e}", dir.display()))?;

    let pretty = serde_json::to_string_pretty(&parsed)
        .map_err(|e| format!("Failed to format JSON: {e}"))?;
    let target_path = dir.join(&filename);
    fs::write(&target_path, format!("{pretty}\n"))
        .map_err(|e| format!("Failed to save '{}': {e}", target_path.display()))?;

    Ok(format!("Saved {filename} to {}", target_path.display()))
}

#[tauri::command]
fn load_recent_logs(app: tauri::AppHandle, limit: u32) -> Result<Vec<LogEventDto>, String> {
    let Some(db_path) = resolve_host_db_path(&app) else {
        return Ok(Vec::new());
    };

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open logs database: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, event_type, rule_id, service_id, trigger_type, trigger_details,
                    status, attempt_count, webview_reloaded, started_at, finished_at
             FROM recovery_events
             ORDER BY id DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Failed to query recovery_events: {e}"))?;

    let rows = stmt
        .query_map([limit as i64], |row| {
            let reloaded: i32 = row.get(8)?;
            Ok(LogEventDto {
                id: row.get(0)?,
                event_type: row.get(1)?,
                rule_id: row.get(2)?,
                service_id: row.get(3)?,
                trigger_type: row.get(4)?,
                trigger_details: row.get(5)?,
                status: row.get(6)?,
                attempt_count: row.get(7)?,
                webview_reloaded: reloaded != 0,
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
            })
        })
        .map_err(|e| format!("Failed to read rows: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to map log rows: {e}"))
}

#[tauri::command]
fn load_event_executions(
    app: tauri::AppHandle,
    event_id: i64,
) -> Result<Vec<CommandExecutionDto>, String> {
    let Some(db_path) = resolve_host_db_path(&app) else {
        return Ok(Vec::new());
    };

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open logs database: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, executable, arguments, stdout, stderr, exit_code, status, duration_ms, executed_at
             FROM command_executions
             WHERE recovery_event_id = ?1
             ORDER BY id ASC",
        )
        .map_err(|e| format!("Failed to query command_executions: {e}"))?;

    let rows = stmt
        .query_map([event_id], |row| {
            Ok(CommandExecutionDto {
                id: row.get(0)?,
                executable: row.get(1)?,
                arguments: row.get(2)?,
                stdout: row.get(3)?,
                stderr: row.get(4)?,
                exit_code: row.get(5)?,
                status: row.get(6)?,
                duration_ms: row.get(7)?,
                executed_at: row.get(8)?,
            })
        })
        .map_err(|e| format!("Failed to read execution rows: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to map execution rows: {e}"))
}

#[tauri::command]
fn run_perfumes_containers(action: String) -> Result<String, String> {
    if action != "start" && action != "stop" {
        return Err("Unsupported action; expected 'start' or 'stop'".to_string());
    }

    let docker_exe = if cfg!(target_os = "windows") {
        "docker.exe"
    } else {
        "docker"
    };

    let containers = ["perfumes_app", "perfumes_db"];
    let mut outputs = Vec::new();

    for container in containers {
        let mut cmd = std::process::Command::new(docker_exe);
        cmd.args([action.as_str(), container]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("فشل تشغيل {docker_exe} {action} {container}: {e}"))?;

        if !output.status.success() {
            let err_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "فشل {action} للحاوية {container}: {}",
                if err_text.is_empty() {
                    format!("exit code {:?}", output.status.code())
                } else {
                    err_text
                }
            ));
        }

        let ok_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        outputs.push(if ok_text.is_empty() {
            container.to_string()
        } else {
            ok_text
        });
    }

    let label = if action == "start" {
        "تم تشغيل"
    } else {
        "تم إيقاف"
    };
    Ok(format!("{label}: {}", outputs.join("، ")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_settings_bundle,
            save_config_file,
            load_recent_logs,
            load_event_executions,
            run_perfumes_containers
        ])
        .run(tauri::generate_context!())
        .expect("error while running Desktop Host Settings");
}
