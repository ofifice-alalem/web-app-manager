//! Sensitive-data redaction for command outputs and diagnostic logs.

use regex::Regex;
use std::sync::OnceLock;

const REDACTED: &str = "[REDACTED]";

fn key_value_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)([A-Za-z0-9_.-]*(?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|auth[_-]?token))(\s*[:=]\s*)(?:"[^"]*"|'[^']*'|\S+)"#,
        )
        .expect("valid key-value redaction regex")
    })
}

fn bearer_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(Bearer\s+)[A-Za-z0-9._~+/=-]+")
            .expect("valid bearer redaction regex")
    })
}

fn url_credentials_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(https?://)([^:\s/@]+):([^\s/]+)@")
            .expect("valid URL credentials redaction regex")
    })
}

/// Redacts passwords, tokens, API keys, bearer headers, and URL credentials from text.
pub fn redact_sensitive(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let step1 = key_value_regex().replace_all(input, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], &caps[2], REDACTED)
    });
    let step2 = bearer_regex().replace_all(&step1, |caps: &regex::Captures| {
        format!("{}{}", &caps[1], REDACTED)
    });
    let step3 = url_credentials_regex().replace_all(&step2, |caps: &regex::Captures| {
        format!("{}{}:{}@", &caps[1], &caps[2], REDACTED)
    });

    step3.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_passwords_tokens_and_bearer_headers() {
        let raw = "DB_PASSWORD=supersecret123 Authorization: Bearer eyJhbGciOiJIUzI1NiJ9 api_key: \"abc-123\"";
        let clean = redact_sensitive(raw);

        assert!(!clean.contains("supersecret123"));
        assert!(!clean.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(!clean.contains("abc-123"));
        assert!(clean.contains("DB_PASSWORD=[REDACTED]"));
        assert!(clean.contains("Bearer [REDACTED]"));
        assert!(clean.contains("api_key: [REDACTED]"));
    }

    #[test]
    fn redacts_url_embedded_credentials() {
        let raw = "Connecting to https://admin:p@ssw0rd@127.0.0.1:8000/api";
        let clean = redact_sensitive(raw);

        assert!(!clean.contains("p@ssw0rd"));
        assert!(clean.contains("https://admin:[REDACTED]@127.0.0.1:8000/api"));
    }

    #[test]
    fn preserves_non_sensitive_output() {
        let raw = "Container sales-app restarted successfully in 1.2s";
        assert_eq!(redact_sensitive(raw), raw);
    }
}
