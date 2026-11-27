//! Matches detected fault conditions against enabled rules in `recovery-rules.json`.

use crate::config::{RecoveryRule, RecoveryTrigger};
use crate::monitoring::{DetectedCondition, DetectedTrigger};

/// Returns the first enabled `RecoveryRule` whose trigger matches `condition`.
pub fn find_matching_rule<'a>(
    rules: &'a [RecoveryRule],
    condition: &DetectedCondition,
) -> Option<&'a RecoveryRule> {
    rules
        .iter()
        .filter(|rule| rule.enabled)
        .find(|rule| trigger_matches(&rule.trigger, condition))
}

fn trigger_matches(trigger: &RecoveryTrigger, condition: &DetectedCondition) -> bool {
    if trigger.service_id() != condition.service_id {
        return false;
    }

    match (trigger, &condition.trigger) {
        (
            RecoveryTrigger::HttpStatus { status_codes, .. },
            DetectedTrigger::HttpStatus { status_code },
        ) => status_codes.contains(status_code),
        (RecoveryTrigger::ConnectionRefused { .. }, DetectedTrigger::ConnectionRefused) => true,
        (
            RecoveryTrigger::ServiceStatus { value: expected, .. },
            DetectedTrigger::ServiceStatus { value: actual },
        ) => expected == actual,
        (RecoveryTrigger::TcpUnavailable { .. }, DetectedTrigger::TcpUnavailable) => true,
        (RecoveryTrigger::ProcessNotRunning { .. }, DetectedTrigger::ProcessNotRunning) => true,
        _ => false,
    }
}
