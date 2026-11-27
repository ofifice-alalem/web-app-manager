//! Tauri IPC commands and shared shell state for the minimal client UI.

use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Notify;

use crate::browser::BrowserController;

pub const SHELL_STATUS_EVENT: &str = "shell-status-changed";

/// High-level UI phase exposed to the Vue client shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellPhase {
    Checking,
    Ready,
    Recovering,
    Unavailable,
}

/// Sanitized status payload sent to the Vue frontend.
/// Contains only non-technical, user-facing state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellStatusDto {
    pub phase: ShellPhase,
    pub message: String,
    pub target_url: Option<String>,
}

impl Default for ShellStatusDto {
    fn default() -> Self {
        Self {
            phase: ShellPhase::Checking,
            message: "جارٍ التحقق من جاهزية المنظومة...".to_string(),
            target_url: None,
        }
    }
}

/// Thread-safe state holder for the client shell status.
#[derive(Debug, Default)]
pub struct ShellStateManager {
    status: RwLock<ShellStatusDto>,
}

impl ShellStateManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_status(&self) -> Result<ShellStatusDto, String> {
        self.status
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| "Failed to read shell status".to_string())
    }

    pub fn set_status(&self, next: ShellStatusDto) -> Result<ShellStatusDto, String> {
        let mut guard = self
            .status
            .write()
            .map_err(|_| "Failed to update shell status".to_string())?;
        *guard = next.clone();
        Ok(next)
    }
}

/// Updates `ShellStateManager` and emits the status event to the frontend.
pub fn publish_shell_status(
    app: &AppHandle,
    state: &ShellStateManager,
    next: ShellStatusDto,
) -> Result<ShellStatusDto, String> {
    let updated = state.set_status(next)?;
    let _ = app.emit(SHELL_STATUS_EVENT, &updated);
    Ok(updated)
}

/// Returns the current shell readiness status to the Vue frontend.
#[tauri::command]
pub fn get_shell_status(state: State<'_, ShellStateManager>) -> Result<ShellStatusDto, String> {
    state.get_status()
}

/// Reloads the embedded WebView.
#[tauri::command]
pub fn reload_shell_webview(app: AppHandle) -> Result<(), String> {
    BrowserController::reload_webview(&app).map_err(|err| err.to_string())
}

/// Transitions the shell back to checking state and wakes the lifecycle coordinator.
#[tauri::command]
pub fn retry_shell_connection(
    app: AppHandle,
    state: State<'_, ShellStateManager>,
    retry_notify: State<'_, Arc<Notify>>,
) -> Result<ShellStatusDto, String> {
    let current = state.get_status()?;
    let updated = publish_shell_status(
        &app,
        &state,
        ShellStatusDto {
            phase: ShellPhase::Checking,
            message: "جارٍ التحقق من جاهزية المنظومة...".to_string(),
            target_url: current.target_url,
        },
    )?;

    retry_notify.notify_one();
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shell_status_starts_in_checking_phase() {
        let manager = ShellStateManager::new();
        let status = manager.get_status().expect("status readable");
        assert_eq!(status.phase, ShellPhase::Checking);
        assert!(status.target_url.is_none());
    }

    #[test]
    fn updates_and_serializes_shell_status() {
        let manager = ShellStateManager::new();
        let updated = manager
            .set_status(ShellStatusDto {
                phase: ShellPhase::Unavailable,
                message: "Unable to connect to the application.".to_string(),
                target_url: Some("http://127.0.0.1:8000".to_string()),
            })
            .expect("status updated");

        let json = serde_json::to_string(&updated).expect("serializable");
        assert!(json.contains("\"phase\":\"unavailable\""));
        assert!(json.contains("\"targetUrl\":\"http://127.0.0.1:8000\""));
    }
}
