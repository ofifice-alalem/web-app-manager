//! Tauri WebView window lifecycle and navigation control.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tokio::sync::Notify;

use super::policy::{BrowserError, NavigationPolicy};

pub const MAIN_WINDOW_LABEL: &str = "main";
const DEFAULT_WINDOW_TITLE: &str = "taqniya";
const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 800.0;
const MIN_WINDOW_WIDTH: f64 = 800.0;
const MIN_WINDOW_HEIGHT: f64 = 600.0;

/// Manages the main desktop window and its embedded WebView.
pub struct BrowserController;

impl BrowserController {
    /// Creates the primary client window with navigation interception attached.
    pub fn create_main_window(
        app: &AppHandle,
        policy: Arc<RwLock<NavigationPolicy>>,
        error_notify: Arc<Notify>,
        showing_shell: Arc<AtomicBool>,
    ) -> Result<WebviewWindow, BrowserError> {
        let nav_policy = Arc::clone(&policy);
        let new_window_policy = Arc::clone(&policy);
        let app_for_new_window = app.clone();

        WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
            .title(DEFAULT_WINDOW_TITLE)
            .inner_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
            .min_inner_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
            .resizable(true)
            .maximized(true)
            .center()
            .on_navigation(move |url| match nav_policy.read() {
                Ok(guard) => guard.is_navigation_allowed(url),
                Err(_) => false,
            })
            .on_page_load(move |window, payload| {
                if payload.url().scheme() == "chrome-error" {
                    let _ = payload.event() == PageLoadEvent::Started
                        || payload.event() == PageLoadEvent::Finished;
                    showing_shell.store(true, Ordering::SeqCst);
                    let shell_url = if cfg!(debug_assertions) {
                        "http://localhost:1420/index.html"
                    } else if cfg!(target_os = "windows") {
                        "http://tauri.localhost/index.html"
                    } else {
                        "tauri://localhost/index.html"
                    };
                    if let Ok(parsed) = tauri::Url::parse(shell_url) {
                        let _ = window.navigate(parsed);
                    }
                    error_notify.notify_one();
                }
            })
            .on_new_window(move |url, _features| {
                // Keep allowed links inside the main WebView instead of spawning an external browser.
                let allowed = new_window_policy
                    .read()
                    .map(|guard| guard.is_navigation_allowed(&url))
                    .unwrap_or(false);

                if allowed {
                    if let Some(window) = app_for_new_window.get_webview_window(MAIN_WINDOW_LABEL) {
                        let _ = window.navigate(url);
                    }
                }

                NewWindowResponse::Deny
            })
            .build()
            .map_err(|err| BrowserError::WebViewOperation(err.to_string()))
    }

    /// Validates `target_url`, updates the navigation policy, and navigates the main WebView.
    pub fn navigate_to_target(
        app: &AppHandle,
        policy: &Arc<RwLock<NavigationPolicy>>,
        target_url: &str,
    ) -> Result<(), BrowserError> {
        let parsed_url = {
            let mut guard = policy
                .write()
                .map_err(|_| BrowserError::WebViewOperation("Navigation policy lock poisoned".to_string()))?;
            guard.set_target_url(target_url)?
        };

        let window = Self::get_main_window(app)?;
        window
            .navigate(parsed_url)
            .map_err(|err| BrowserError::WebViewOperation(err.to_string()))
    }

    /// Reloads the current page inside the main WebView.
    pub fn reload_webview(app: &AppHandle) -> Result<(), BrowserError> {
        let window = Self::get_main_window(app)?;
        window
            .reload()
            .map_err(|err| BrowserError::WebViewOperation(err.to_string()))
    }

    /// Navigates the main WebView back to the local shell UI (`index.html`)
    /// so loading/recovering/error overlays are shown instead of browser error pages.
    pub fn show_shell_view(app: &AppHandle) -> Result<(), BrowserError> {
        let window = Self::get_main_window(app)?;
        let shell_url = if cfg!(debug_assertions) {
            "http://localhost:1420/index.html"
        } else if cfg!(target_os = "windows") {
            "http://tauri.localhost/index.html"
        } else {
            "tauri://localhost/index.html"
        };
        let parsed = tauri::Url::parse(shell_url)
            .map_err(|err| BrowserError::WebViewOperation(err.to_string()))?;
        window
            .navigate(parsed)
            .map_err(|err| BrowserError::WebViewOperation(err.to_string()))
    }

    fn get_main_window(app: &AppHandle) -> Result<WebviewWindow, BrowserError> {
        app.get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| BrowserError::WindowNotFound(MAIN_WINDOW_LABEL.to_string()))
    }
}
