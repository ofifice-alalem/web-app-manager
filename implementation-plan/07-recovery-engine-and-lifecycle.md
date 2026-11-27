# Step 7: Recovery Engine & Application Lifecycle Integration

## Objective
Implement the `recovery` module and wire the complete end-to-end application lifecycle: matching detected conditions to predefined rules, executing ordered recovery actions with retries, cooldowns, and per-service locks, verifying recovery, reloading the WebView on success, persisting logs to SQLite, and updating the minimal client UI.

---

## Scope

### Included
- Matching `DetectedCondition` events against enabled rules in `recovery-rules.json`.
- Executing ordered `actions` (`command`, `start_service`, `restart_service`, `wait`, `check_service`, `reload_application`).
- Respecting `continue_on_failure` on command steps.
- Enforcing `retry` (`max_attempts`, `delay_seconds`) and `cooldown_seconds` per rule/service.
- Enforcing a per-service concurrency lock so conflicting recoveries never run simultaneously for the same service.
- Post-recovery service verification before triggering WebView reload.
- Preventing infinite reload loops or infinite recovery loops.
- Logging every recovery attempt and command execution to SQLite via `LogManager`.
- Wiring the full startup-to-runtime lifecycle into `src-tauri/src/lib.rs`, `commands/`, `browser/`, and Vue 3 (`src/`).

### Excluded
- Developer UI for creating/editing recovery rules.
- Sidebars, service-management pages, or log-viewer pages in the client UI.
- Exposing technical command details, stack traces, or raw stdout/stderr to the end user.

---

## Tasks
1. Implement rule matching in `src-tauri/src/recovery/matcher.rs`:
   - Match incoming `DetectedCondition` against `enabled: true` rules in `recovery-rules.json` by `trigger.type`, `service_id`, and trigger value/status.
2. Implement concurrency & cooldown state tracking in `src-tauri/src/recovery/state.rs`:
   - Maintain a per-service async lock (`HashMap<String, Arc<Mutex<()>>>`) so only one recovery runs per service at any time.
   - Track `last_recovery_at` per rule/service and skip execution if within `cooldown_seconds`.
3. Implement the action pipeline & retry executor in `src-tauri/src/recovery/engine.rs`:
   - Create a `recovery_events` record in SQLite (`LogManager`) at the start of recovery.
   - Loop up to `retry.max_attempts` with `retry.delay_seconds` between failed attempts. **On each retry attempt, restart the rule's action sequence from the beginning** (rather than resuming mid-sequence from the failed step) so preconditions, commands, waits, and verification checks run cleanly as a unit; record each attempt's number, outcome, and executed commands in SQLite.
   - Execute actions sequentially:
     - `command`: run via `CommandExecutor` (platform-resolved), record in `command_executions`, abort attempt if `!result.success && !continue_on_failure`.
     - `start_service` / `restart_service`: delegate to `ServiceManager`, record command execution.
     - `wait`: async `tokio::time::sleep(Duration::from_secs(seconds))`.
     - `check_service`: re-check target service via `ServiceManager`; fail attempt if still unhealthy.
     - `reload_application`: mark reload requested (executed only if overall recovery & `app.reload_after_recovery` succeed).
   - Update `recovery_events` with final `status`, `attempt_count`, `webview_reloaded`, and `finished_at`.
4. Implement WebView reload & UI state coordination (`src-tauri/src/browser/` + `src-tauri/src/commands/` + `src/`):
   - Only navigate/reload the WebView when required services are verified healthy.
   - Never reload repeatedly while an error persists.
   - Emit high-level, non-technical UI state events to Vue (`Checking`, `Ready`, `Recovering` [silent/subtle indicator], `Unavailable` [generic user-friendly message when retries are exhausted]).
5. Wire the full lifecycle in `src-tauri/src/lib.rs`:
   - Launch window -> load & validate config -> initialize SQLite & run retention -> run startup readiness & initial recovery if needed -> load application URL in WebView -> start background monitoring loop -> handle runtime recoveries silently -> show generic error if recovery exhausts retries.

---

## Expected Files
- `src-tauri/src/recovery/mod.rs`
- `src-tauri/src/recovery/matcher.rs`
- `src-tauri/src/recovery/state.rs`
- `src-tauri/src/recovery/engine.rs`
- `src-tauri/src/commands/lifecycle.rs` (Tauri commands & event emitters for UI state)
- `src-tauri/src/browser/mod.rs` (updated with reload/load-url control)
- `src-tauri/src/lib.rs` (full module wiring)
- `src/composables/useAppState.ts`
- `src/services/tauriBridge.ts`
- `src/App.vue` / `src/views/` (wired to backend lifecycle states)

---

## Technical Details
- **Loop Prevention:** Three layers of protection: `retry.max_attempts`, `cooldown_seconds`, and per-service `Mutex` lock.
- **UI Isolation:** Vue receives only sanitized state enums (`Checking`, `Ready`, `Recovering`, `Unavailable`) and a generic localized/friendly message on failure—never raw command outputs or internal paths.
- **Verified Reload:** `reload_application` executes only after `check_service` (or final service check) confirms the service is healthy.

---

## Acceptance Criteria
- Matching faults trigger the exact enabled rule and execute actions in order.
- Failing a mandatory step (`continue_on_failure: false`) stops the sequence and triggers retry logic up to `max_attempts`.
- Cooldown prevents repeated recovery triggers within `cooldown_seconds`.
- Concurrent triggers for the same service are serialized/deduplicated via the per-service lock.
- WebView reloads automatically **only** after verified recovery success.
- Exhausting retries displays a generic, non-technical error message in the UI and records the full failure in SQLite.
- Zero terminal windows or technical command details are shown to the user.

---

## Testing
- **Unit & Integration Tests (`cargo test`):**
  - Rule matching for enabled vs. disabled rules.
  - Ordered action execution (`command` -> `wait` -> `check_service` -> `reload_application`).
  - Retry behavior when first attempt fails and second succeeds.
  - Exhausted retries -> `Failed` status logged in SQLite, `webview_reloaded == false`.
  - Cooldown enforcement: second trigger within `cooldown_seconds` is ignored.
  - Concurrency test: simultaneous triggers for the same `service_id` do not run overlapping recoveries.
  - Verification that `reload_application` is NOT called if post-action `check_service` fails.
