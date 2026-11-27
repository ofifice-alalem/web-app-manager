//! URL validation and navigation policy for the embedded WebView.
//!
//! Ensures that internal application navigation stays inside the WebView
//! and prevents arbitrary external origins or unsafe schemes from loading.

use thiserror::Error;
use url::Url;

/// Errors returned by browser navigation and window control operations.
#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("Invalid target URL: {0}")]
    InvalidTargetUrl(String),

    #[error("Unsupported URL scheme '{0}'; only http and https are allowed")]
    UnsupportedScheme(String),

    #[error("Main application window '{0}' was not found")]
    WindowNotFound(String),

    #[error("WebView operation failed: {0}")]
    WebViewOperation(String),
}

/// Normalized origin tuple `(scheme, host, port)` used for origin comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginKey {
    scheme: String,
    host: String,
    port: u16,
}

impl OriginKey {
    fn from_url(url: &Url) -> Option<Self> {
        let host = url.host_str()?.to_ascii_lowercase();
        let port = url.port_or_known_default()?;
        Some(Self {
            scheme: url.scheme().to_ascii_lowercase(),
            host,
            port,
        })
    }
}

/// Controls which URLs are permitted to load inside the embedded WebView.
#[derive(Debug, Clone)]
pub struct NavigationPolicy {
    allowed_target: Option<OriginKey>,
    allow_dev_origin: bool,
}

impl Default for NavigationPolicy {
    fn default() -> Self {
        Self {
            allowed_target: None,
            allow_dev_origin: cfg!(debug_assertions),
        }
    }
}

impl NavigationPolicy {
    /// Creates a policy permitting navigation to the origin of `target_url`.
    pub fn with_target_url(target_url: &str) -> Result<Self, BrowserError> {
        let parsed = Self::validate_http_url(target_url)?;
        let origin = OriginKey::from_url(&parsed)
            .ok_or_else(|| BrowserError::InvalidTargetUrl(target_url.to_string()))?;

        Ok(Self {
            allowed_target: Some(origin),
            allow_dev_origin: cfg!(debug_assertions),
        })
    }

    /// Validates that `raw_url` is a well-formed HTTP or HTTPS URL with a host.
    pub fn validate_http_url(raw_url: &str) -> Result<Url, BrowserError> {
        let parsed = Url::parse(raw_url.trim())
            .map_err(|err| BrowserError::InvalidTargetUrl(err.to_string()))?;

        let scheme = parsed.scheme().to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(BrowserError::UnsupportedScheme(scheme));
        }

        if parsed.host_str().is_none() {
            return Err(BrowserError::InvalidTargetUrl(
                "URL must include a host".to_string(),
            ));
        }

        Ok(parsed)
    }

    /// Updates the permitted target origin after validating `target_url`.
    pub fn set_target_url(&mut self, target_url: &str) -> Result<Url, BrowserError> {
        let parsed = Self::validate_http_url(target_url)?;
        let origin = OriginKey::from_url(&parsed)
            .ok_or_else(|| BrowserError::InvalidTargetUrl(target_url.to_string()))?;
        self.allowed_target = Some(origin);
        Ok(parsed)
    }

    /// Returns `true` if `url` is allowed to load inside the embedded WebView.
    pub fn is_navigation_allowed(&self, url: &Url) -> bool {
        if Self::is_internal_shell_url(url) {
            return true;
        }

        if self.allow_dev_origin && Self::is_local_dev_url(url) {
            return true;
        }

        let Some(allowed) = &self.allowed_target else {
            return false;
        };

        OriginKey::from_url(url).as_ref() == Some(allowed)
    }

    fn is_internal_shell_url(url: &Url) -> bool {
        let scheme = url.scheme().to_ascii_lowercase();
        if scheme == "tauri" {
            return true;
        }
        if (scheme == "http" || scheme == "https")
            && url.host_str().map(|h| h.eq_ignore_ascii_case("tauri.localhost")).unwrap_or(false)
        {
            return true;
        }
        false
    }

    fn is_local_dev_url(url: &Url) -> bool {
        let scheme = url.scheme().to_ascii_lowercase();
        if scheme != "http" {
            return false;
        }
        let Some(host) = url.host_str() else {
            return false;
        };
        let is_loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
        is_loopback && url.port() == Some(1420)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_http_and_https_urls() {
        assert!(NavigationPolicy::validate_http_url("http://127.0.0.1:8000").is_ok());
        assert!(NavigationPolicy::validate_http_url("https://sales.local/login").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes_and_malformed_urls() {
        assert!(matches!(
            NavigationPolicy::validate_http_url("file:///etc/passwd"),
            Err(BrowserError::UnsupportedScheme(_))
        ));
        assert!(matches!(
            NavigationPolicy::validate_http_url("javascript:alert(1)"),
            Err(BrowserError::UnsupportedScheme(_))
        ));
        assert!(matches!(
            NavigationPolicy::validate_http_url("not a url"),
            Err(BrowserError::InvalidTargetUrl(_))
        ));
    }

    #[test]
    fn allows_same_origin_and_blocks_external_origins() {
        let policy = NavigationPolicy::with_target_url("http://127.0.0.1:8000/dashboard")
            .expect("valid target URL");

        let internal = Url::parse("http://127.0.0.1:8000/invoices/42").unwrap();
        let different_port = Url::parse("http://127.0.0.1:9124/health").unwrap();
        let external = Url::parse("https://example.com").unwrap();
        let shell_url = Url::parse("tauri://localhost/index.html").unwrap();

        assert!(policy.is_navigation_allowed(&internal));
        assert!(policy.is_navigation_allowed(&shell_url));
        assert!(!policy.is_navigation_allowed(&different_port));
        assert!(!policy.is_navigation_allowed(&external));
    }
}
