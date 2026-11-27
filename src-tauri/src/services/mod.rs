//! Service health check and lifecycle management module.

pub mod checks;
pub mod manager;
pub mod models;

pub use manager::ServiceManager;
pub use models::{
    ServiceCheckResult, ServiceHealthState, ServiceLifecycleAction, ServiceLifecycleOutcome,
    SystemReadinessReport,
};
