//! Tauri IPC command handlers exposed to the Vue client shell.

pub mod shell;

pub use shell::{
    get_shell_status, publish_shell_status, reload_shell_webview, retry_shell_connection,
    ShellPhase, ShellStateManager, ShellStatusDto,
};
