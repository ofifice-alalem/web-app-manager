# Step 2: Configuration Manager & JSON Schemas

## Objective
Design, document, and implement the JSON configuration system (`app.json`, `services.json`, `recovery-rules.json`) with strict schema validation, cross-platform command definitions (Windows and Linux), and diagnostic error reporting.

---

## Scope

### Included
- Defining and documenting JSON schemas for:
  - `config/app.json` (single-application metadata, URL, startup timeout, reload behavior).
  - `config/services.json` (service definitions, `required` flag, health check types: `http`, `tcp`, `process`, `docker`, and `platform_commands` for `windows` and `linux`).
  - `config/recovery-rules.json` (rules, `enabled` flag, supported `trigger` types, ordered `actions`, `retry` policy, `cooldown_seconds`).
- Implementing the Rust `config` module to load, deserialize, and validate all three JSON files.
- Supporting external, editable configuration files without requiring an application rebuild where appropriate.
- Producing clear diagnostic error messages when JSON syntax, types, references, or unsupported trigger/action types are encountered.
- Defining matching TypeScript types in `src/types/` for any configuration-derived state exposed to the frontend (excluding sensitive data).

### Excluded
- Executing any commands or health checks defined in the config (covered in Steps 3 and 4).
- Building a UI or developer tool to edit or generate JSON files (future separate project).
- Storing passwords, tokens, or secrets in general configuration files.

---

## Tasks
1. Define the formal JSON structure and documentation (e.g., JSON Schema files or documented schema reference) for:
   - `app.json`: `application.id`, `name`, `url`, `startup_timeout_seconds`, `reload_after_recovery`.
   - `services.json`: `services[]` with `id`, `name`, `required`, `check` (`http`, `tcp`, `process`, `docker`), and `commands` (`start`, `stop`, `restart`) supporting `platform_commands` (`windows` / `linux`).
   - `recovery-rules.json`: `rules[]` with `id`, `name`, `enabled`, `trigger` (`http_status`, `connection_refused`, `service_status`, `tcp_unavailable`, `process_not_running`), `actions` (`command`, `start_service`, `restart_service`, `wait`, `check_service`, `reload_application`), `retry` (`max_attempts`, `delay_seconds`), and `cooldown_seconds`.
2. Create sample/default configuration files under `config/` matching the Sales System + Print Server (`127.0.0.1:9124`) reference architecture.
3. Implement Rust structs with `serde` (`Deserialize`, `Serialize`) in `src-tauri/src/config/`.
4. Implement secure configuration path resolution and permission checks:
   - Define a dedicated, protected storage location for external configuration files (e.g., administrator-owned installation/system config directory on Windows such as `%ProgramData%\<app-id>\config\` with restricted write ACLs, and `/etc/<app-id>/` owned by `root` with `0644` permissions on Linux) so non-privileged users or untrusted processes cannot tamper with command definitions.
   - Verify configuration file permissions/integrity before loading (reject or flag world-writable config files on Linux or insecure ACLs) while still allowing authorized administrators to update configs without rebuilding the app.
5. Implement semantic validation beyond basic JSON parsing:
   - Verify `application.url` is a valid HTTP/HTTPS URL and `startup_timeout_seconds > 0`.
   - Verify unique `service.id` and `rule.id` values.
   - Verify every `service_id` referenced in `recovery-rules.json` exists in `services.json`.
   - Reject unsupported `check.type`, `trigger.type`, or `action.type` values with explicit diagnostic messages.
   - Validate `retry.max_attempts >= 1`, `delay_seconds >= 0`, `cooldown_seconds >= 0`, and positive timeouts.
   - Validate `platform_commands` resolution for the current OS (`windows` vs `linux`).
6. Implement structured diagnostic error reporting when configuration loading or validation fails, and expose safe startup config error state to the UI (generic error for user, detailed diagnostic for logs).
7. Create corresponding TypeScript interfaces in `src/types/` for frontend-facing config/status DTOs.

---

## Expected Files
- `config/app.json`
- `config/services.json`
- `config/recovery-rules.json`
- `config/schemas/` (optional/recommended JSON Schema definitions for future developer tooling)
- `src-tauri/src/config/mod.rs`
- `src-tauri/src/config/models.rs`
- `src-tauri/src/config/validator.rs`
- `src-tauri/src/config/loader.rs`
- `src/types/config.ts`

---

## Technical Details
- **Crates:** `serde`, `serde_json`, `url` (for URL validation).
- **Cross-Platform Commands:** Commands must support `platform_commands` keyed by `"windows"` and `"linux"`, allowing different executables, paths, or arguments per OS.
- **Security:** Configuration must never contain secrets/credentials; validation must reject malformed or unknown rule/check types before any execution layer consumes them. External config files must reside in a write-protected directory (admin/root-writable only, read-only for standard client execution) to prevent unauthorized command injection via JSON tampering.
- **Machine-Readability:** Schemas and field names must be consistent and well-documented so the future developer rule-builder tool can generate valid files.

---

## Acceptance Criteria
- All three JSON files (`app.json`, `services.json`, `recovery-rules.json`) load and validate on startup.
- Invalid JSON, missing required fields, unknown `service_id` references, negative timeouts, or unsupported trigger/action types fail validation with clear diagnostic messages.
- Platform-specific commands resolve correctly based on target OS (`windows` vs `linux`).
- Editing external config files takes effect on next application launch without recompiling Rust/Vue.

---

## Testing
- **Unit Tests (`cargo test`):**
  - Valid configuration parsing for `app.json`, `services.json`, and `recovery-rules.json`.
  - Rejection of malformed JSON and missing required fields.
  - Rejection of duplicate IDs and dangling `service_id` references.
  - Rejection of unsupported `check.type`, `trigger.type`, and `action.type`.
  - Verification of Windows vs. Linux `platform_commands` selection logic.
  - Validation of timeout, retry, and cooldown bounds.
