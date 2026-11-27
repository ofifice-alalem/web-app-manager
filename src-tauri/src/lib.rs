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
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, watch, Notify};

use browser::{BrowserController, NavigationPolicy};
use commands::{
    get_shell_status, publish_shell_status, reload_shell_webview, retry_shell_connection,
    ShellPhase, ShellStateManager, ShellStatusDto,
};
use config::{load_config_bundle_from_dir, resolve_config_dir};
use database::{LogManager, RetentionPolicy};
use executor::CommandExecutor;
use monitoring::{
    wait_for_startup_readiness, ActiveRecoveryTracker, BackgroundMonitor, DetectedCondition,
    StartupReadinessOutcome,
};
use recovery::{RecoveryEngine, RecoveryOutcome, RecoveryState};
use services::ServiceManager;

const STARTUP_POLL_INTERVAL: Duration = Duration::from_secs(2);
const RUNTIME_MONITOR_INTERVAL: Duration = Duration::from_secs(10);
const GENERIC_UNAVAILABLE_MSG: &str =
    "The application could not be started or reached. Please verify the system is available and try again.";

/// Configures and runs the Tauri 2 desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let navigation_policy = Arc::new(RwLock::new(NavigationPolicy::default()));
    let shell_state = ShellStateManager::new();
    let retry_notify = Arc::new(Notify::new());

    tauri::Builder::default()
        .manage(Arc::clone(&navigation_policy))
        .manage(shell_state)
        .manage(Arc::clone(&retry_notify))
        .setup(move |app| {
            let handle = app.handle().clone();
            BrowserController::create_main_window(&handle, Arc::clone(&navigation_policy))
                .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)?;

            let nav_for_task = Arc::clone(&navigation_policy);
            let retry_for_task = Arc::clone(&retry_notify);
            tauri::async_runtime::spawn(async move {
                run_application_lifecycle(handle, nav_for_task, retry_for_task).await;
            });

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

async fn run_application_lifecycle(
    app: AppHandle,
    navigation_policy: Arc<RwLock<NavigationPolicy>>,
    retry_notify: Arc<Notify>,
) {
    let shell_state = app.state::<ShellStateManager>();

    let bundle = match resolve_config_dir().and_then(|dir| load_config_bundle_from_dir(&dir)) {
        Ok(bundle) => bundle,
        Err(err) => {
            eprintln!("Configuration error: {err}");
            let _ = publish_shell_status(
                &app,
                shell_state.inner(),
                ShellStatusDto {
                    phase: ShellPhase::Unavailable,
                    message: GENERIC_UNAVAILABLE_MSG.to_string(),
                    target_url: None,
                },
            );
            return;
        }
    };

    let log_manager = match app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| LogManager::open_in_dir(&dir).map_err(|e| e.to_string()))
    {
        Ok(lm) => {
            let _ = lm.apply_retention(&RetentionPolicy::default());
            lm
        }
        Err(err) => {
            eprintln!("Failed to initialize local log database: {err}");
            match LogManager::open_in_memory() {
                Ok(lm) => lm,
                Err(_) => return,
            }
        }
    };

    let executor = CommandExecutor::new();
    let Ok(service_manager) = ServiceManager::new(bundle.services.clone(), executor.clone()) else {
        let _ = publish_shell_status(
            &app,
            shell_state.inner(),
            ShellStatusDto {
                phase: ShellPhase::Unavailable,
                message: GENERIC_UNAVAILABLE_MSG.to_string(),
                target_url: None,
            },
        );
        return;
    };
    let service_manager = Arc::new(service_manager);
    let active_tracker = ActiveRecoveryTracker::new();
    let recovery_state = RecoveryState::new(active_tracker.clone());
    let recovery_engine = Arc::new(RecoveryEngine::new(
        bundle.rules.clone(),
        bundle.app.reload_after_recovery,
        Arc::clone(&service_manager),
        executor,
        log_manager,
        recovery_state,
    ));

    // Startup readiness loop (supports manual Retry if initial startup fails).
    loop {
        let _ = publish_shell_status(
            &app,
            shell_state.inner(),
            ShellStatusDto {
                phase: ShellPhase::Checking,
                message: "Checking application readiness...".to_string(),
                target_url: Some(bundle.app.url.clone()),
            },
        );

        let readiness = wait_for_startup_readiness(
            &service_manager,
            bundle.app.startup_timeout_seconds,
            STARTUP_POLL_INTERVAL,
        )
        .await;

        let is_ready = match readiness {
            StartupReadinessOutcome::Ready(_) => true,
            StartupReadinessOutcome::TimedOut { conditions, .. } => {
                if !conditions.is_empty() {
                    let _ = publish_shell_status(
                        &app,
                        shell_state.inner(),
                        ShellStatusDto {
                            phase: ShellPhase::Recovering,
                            message: "Starting required services...".to_string(),
                            target_url: Some(bundle.app.url.clone()),
                        },
                    );
                    for cond in &conditions {
                        let _ = recovery_engine.handle_condition(cond).await;
                    }
                }
                service_manager.check_all_services().await.is_ready
            }
        };

        if is_ready {
            if let Err(err) =
                BrowserController::navigate_to_target(&app, &navigation_policy, &bundle.app.url)
            {
                eprintln!("Failed to load target URL: {err}");
                let _ = publish_shell_status(
                    &app,
                    shell_state.inner(),
                    ShellStatusDto {
                        phase: ShellPhase::Unavailable,
                        message: GENERIC_UNAVAILABLE_MSG.to_string(),
                        target_url: Some(bundle.app.url.clone()),
                    },
                );
            } else {
                let _ = publish_shell_status(
                    &app,
                    shell_state.inner(),
                    ShellStatusDto {
                        phase: ShellPhase::Ready,
                        message: "Application ready.".to_string(),
                        target_url: Some(bundle.app.url.clone()),
                    },
                );
                break;
            }
        } else {
            let _ = publish_shell_status(
                &app,
                shell_state.inner(),
                ShellStatusDto {
                    phase: ShellPhase::Unavailable,
                    message: GENERIC_UNAVAILABLE_MSG.to_string(),
                    target_url: Some(bundle.app.url.clone()),
                },
            );
        }

        retry_notify.notified().await;
    }

    // Start continuous background monitoring and silent recovery.
    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    let (condition_tx, mut condition_rx) = mpsc::channel::<DetectedCondition>(32);

    let monitor = BackgroundMonitor::new(
        Arc::clone(&service_manager),
        active_tracker,
        RUNTIME_MONITOR_INTERVAL,
    );
    tauri::async_runtime::spawn(async move {
        monitor.run(shutdown_rx, condition_tx).await;
    });

    while let Some(cond) = condition_rx.recv().await {
        let outcome = recovery_engine.handle_condition(&cond).await;
        match outcome {
            RecoveryOutcome::Succeeded {
                should_reload_webview,
                ..
            } => {
                if should_reload_webview {
                    let _ = BrowserController::reload_webview(&app);
                }
                let _ = publish_shell_status(
                    &app,
                    shell_state.inner(),
                    ShellStatusDto {
                        phase: ShellPhase::Ready,
                        message: "Application ready.".to_string(),
                        target_url: Some(bundle.app.url.clone()),
                    },
                );
            }
            RecoveryOutcome::Failed { .. } => {
                let _ = publish_shell_status(
                    &app,
                    shell_state.inner(),
                    ShellStatusDto {
                        phase: ShellPhase::Unavailable,
                        message: GENERIC_UNAVAILABLE_MSG.to_string(),
                        target_url: Some(bundle.app.url.clone()),
                    },
                );
            }
            RecoveryOutcome::SkippedCooldown { .. }
            | RecoveryOutcome::SkippedAlreadyActive { .. }
            | RecoveryOutcome::NoMatchingRule => {}
        }
    }
}
