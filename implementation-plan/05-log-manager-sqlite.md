# Step 5: Local Log Manager (SQLite)

## Objective
Implement the local SQLite persistence layer (`database` module) to record all recovery events, command executions, and diagnostic outcomes with sensitive-data redaction and an automatic retention/rotation policy, without exposing a log-viewer UI in the client.

---

## Scope

### Included
- Initializing a local SQLite database in the application's OS-appropriate data directory (Windows & Linux).
- Creating schema migrations for `recovery_events` and `command_executions` (plus diagnostic/startup event support).
- Recording all required fields: timestamp, event type, associated service, detected error, recovery rule used, executed commands, redacted `stdout`/`stderr`, `exit_code`, attempt count, duration (`duration_ms`), recovery outcome, and whether WebView was reloaded.
- Enforcing secret/password/token redaction before persisting any output.
- Implementing a retention/rotation policy (e.g., max age and/or max record cap with periodic cleanup) so the database does not grow indefinitely.
- Providing Rust query/retrieval functions for automated tests and future developer tooling.

### Excluded
- Any log-viewer page, table, or sidebar UI in the client application.
- Storing unredacted passwords, tokens, or secrets in SQLite.
- Remote/cloud log shipping.

---

## Tasks
1. Select and configure the SQLite crate (e.g., `rusqlite` with `bundled` feature or `sqlx` with SQLite) for reliable cross-platform builds on Windows and Linux without requiring a system-installed SQLite library.
2. Implement database initialization and schema migration in `src-tauri/src/database/migrations.rs`:
   - `recovery_events` table:
     - `id` (PRIMARY KEY)
     - `event_type`
     - `rule_id`
     - `service_id`
     - `trigger_type`
     - `trigger_details`
     - `status`
     - `attempt_count`
     - `webview_reloaded` (BOOLEAN)
     - `started_at`
     - `finished_at`
   - `command_executions` table:
     - `id` (PRIMARY KEY)
     - `recovery_event_id` (FOREIGN KEY -> `recovery_events.id` ON DELETE CASCADE)
     - `executable`
     - `arguments` (JSON array string)
     - `stdout` (redacted)
     - `stderr` (redacted)
     - `exit_code`
     - `status`
     - `duration_ms`
     - `executed_at`
   - Indexes on `started_at`, `service_id`, and `recovery_event_id`.
3. Implement `LogManager` repository methods in `src-tauri/src/database/repository.rs`:
   - `create_recovery_event(...)` -> returns `recovery_event_id`.
   - `update_recovery_event(...)` (status, attempt_count, webview_reloaded, finished_at).
   - `record_command_execution(recovery_event_id, &CommandResult)` (always passing output through `redactor` before insert).
   - `query_recent_events(limit)` and `query_executions_for_event(event_id)` for verification and diagnostics.
4. Implement log retention & rotation in `src-tauri/src/database/retention.rs`:
   - Prune records older than a configured retention period (e.g., 30 days) or exceeding a maximum row threshold, cascading to `command_executions`, and run `PRAGMA incremental_vacuum` / periodic cleanup on startup.
5. Ensure database errors (e.g., transient lock) are handled gracefully and never crash the main application.

---

## Expected Files
- `src-tauri/src/database/mod.rs`
- `src-tauri/src/database/migrations.rs`
- `src-tauri/src/database/models.rs`
- `src-tauri/src/database/repository.rs`
- `src-tauri/src/database/retention.rs`

---

## Technical Details
- **Database Engine:** SQLite (`rusqlite` with `bundled` feature recommended for zero external runtime dependencies on Windows and Linux).
- **Storage Location:** OS application data directory resolved via Tauri's `app.path().app_data_dir()` (e.g., `%APPDATA%/...` on Windows, `~/.local/share/...` on Linux).
- **Concurrency:** Use a thread-safe connection wrapper (e.g., `Mutex<Connection>` or `r2d2` pool) with `WAL` journal mode enabled (`PRAGMA journal_mode=WAL;`).
- **Privacy/Security:** All `stdout`/`stderr` and `trigger_details` strings must be sanitized via Step 3's redactor before SQLite insertion.

---

## Acceptance Criteria
- SQLite database file is created automatically in the app data directory on startup.
- `recovery_events` and `command_executions` tables exist with proper foreign-key linkage.
- Recovery events and their associated command executions are persisted with all required fields.
- Sensitive strings (passwords, tokens) are redacted prior to storage.
- Retention policy prunes expired records without breaking foreign-key integrity.
- No log-management UI is added to the Vue frontend.

---

## Testing
- **Unit & Integration Tests (`cargo test` using temporary/in-memory or temp-dir SQLite):**
  - Schema creation and idempotent migration on repeated startup.
  - Inserting a `recovery_event`, updating its final status/`webview_reloaded`, and linking multiple `command_executions`.
  - Retrieving events and verifying all fields (`exit_code`, `duration_ms`, `attempt_count`, etc.) match.
  - Verifying sensitive tokens/passwords in `stdout`/`stderr` are stored redacted.
  - Verifying retention pruning deletes old `recovery_events` and cascades to `command_executions`.
