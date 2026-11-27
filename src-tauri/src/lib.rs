//! Desktop Application Host Tauri 2 entry library.

pub mod browser;
pub mod commands;
pub mod config;
pub mod database;
pub mod executor;
pub mod monitoring;
pub mod recovery;
pub mod services;

use std::sync::{Arc, RwLock};

use browser::{BrowserController, NavigationPolicy};
use commands::{
    get_shell_status, reload_shell_webview, retry_shell_connection, ShellStateManager,
};

/// Configures and runs the Tauri 2 desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let navigation_policy = Arc::new(RwLock::new(NavigationPolicy::default()));
    let shell_state = ShellStateManager::new();

    tauri::Builder::default()
        .manage(Arc::clone(&navigation_policy))
        .manage(shell_state)
        .setup(move |app| {
            BrowserController::create_main_window(app.handle(), Arc::clone(&navigation_policy))
                .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_shell_status,
            reload_shell_webview,
            retry_shell_connection
        ])
        .run(tauri::generate_context!())
        .expect("error while running Desktop Application Host");
}
