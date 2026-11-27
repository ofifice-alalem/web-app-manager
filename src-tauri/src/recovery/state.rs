//! Concurrency locks and cooldown tracking for the Recovery Engine.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

use crate::monitoring::ActiveRecoveryTracker;

/// RAII guard that releases a service from `ActiveRecoveryTracker` on drop.
pub struct ActiveRecoveryGuard {
    service_id: String,
    tracker: ActiveRecoveryTracker,
}

impl Drop for ActiveRecoveryGuard {
    fn drop(&mut self) {
        self.tracker.release(&self.service_id);
    }
}

/// Manages per-service concurrency locks and per-rule cooldown timestamps.
#[derive(Debug, Clone)]
pub struct RecoveryState {
    active_tracker: ActiveRecoveryTracker,
    service_locks: Arc<StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
    cooldowns: Arc<StdMutex<HashMap<String, Instant>>>,
}

impl RecoveryState {
    pub fn new(active_tracker: ActiveRecoveryTracker) -> Self {
        Self {
            active_tracker,
            service_locks: Arc::new(StdMutex::new(HashMap::new())),
            cooldowns: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    pub fn active_tracker(&self) -> &ActiveRecoveryTracker {
        &self.active_tracker
    }

    /// Marks `service_id` as actively recovering, returning an RAII guard,
    /// or `None` if a recovery is already active for `service_id`.
    pub fn try_begin_service_recovery(&self, service_id: &str) -> Option<ActiveRecoveryGuard> {
        if !self.active_tracker.try_acquire(service_id) {
            return None;
        }
        Some(ActiveRecoveryGuard {
            service_id: service_id.to_string(),
            tracker: self.active_tracker.clone(),
        })
    }

    /// Returns the per-service async mutex to serialize operations on the same service.
    pub fn service_mutex(&self, service_id: &str) -> Arc<AsyncMutex<()>> {
        let mut guard = self
            .service_locks
            .lock()
            .expect("service_locks mutex poisoned");
        guard
            .entry(service_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Checks whether `rule_id` is currently within its `cooldown_seconds` window.
    pub fn is_in_cooldown(&self, rule_id: &str, cooldown_seconds: u64) -> bool {
        if cooldown_seconds == 0 {
            return false;
        }
        let Ok(guard) = self.cooldowns.lock() else {
            return false;
        };
        let Some(&last_finished) = guard.get(rule_id) else {
            return false;
        };
        last_finished.elapsed() < Duration::from_secs(cooldown_seconds)
    }

    /// Records the completion time of a recovery attempt for cooldown enforcement.
    pub fn mark_cooldown(&self, rule_id: &str) {
        if let Ok(mut guard) = self.cooldowns.lock() {
            guard.insert(rule_id.to_string(), Instant::now());
        }
    }
}
