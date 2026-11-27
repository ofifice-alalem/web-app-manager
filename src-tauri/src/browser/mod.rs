//! Browser and WebView management module.

pub mod controller;
pub mod policy;

pub use controller::BrowserController;
pub use policy::{BrowserError, NavigationPolicy};
