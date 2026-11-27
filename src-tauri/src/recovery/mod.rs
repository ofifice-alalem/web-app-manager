//! Automated recovery engine module.

pub mod engine;
pub mod matcher;
pub mod state;

pub use engine::{RecoveryEngine, RecoveryOutcome};
pub use matcher::find_matching_rule;
pub use state::{ActiveRecoveryGuard, RecoveryState};
