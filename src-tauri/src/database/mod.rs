//! Local SQLite log persistence module.

pub mod migrations;
pub mod models;
pub mod repository;
pub mod retention;

pub use models::{
    CommandExecutionRecord, NewRecoveryEvent, RecoveryEventRecord, RecoveryEventStatus,
    RecoveryEventUpdate, RetentionPolicy,
};
pub use repository::{DatabaseError, LogManager, LOG_DB_FILENAME};
