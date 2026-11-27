# Step 4: Service Manager & Health Checks

## Objective
Implement the Rust `services` module to inspect the health of all configured services (HTTP, TCP, Process, Docker container status), return a unified status model, and execute configured `start`, `stop`, and `restart` lifecycle commands via the Command Executor.

---

## Scope

### Included
- Health check implementations for all configured check types:
  - `http`: HTTP/HTTPS request with configurable `expected_status` codes and `timeout_seconds`.
  - `tcp`: TCP socket connection check (`host:port`) with `timeout_seconds`.
  - `process`: OS process presence check (Windows and Linux compatible).
  - `docker`: Docker container status inspection (handling Docker daemon unavailability or permission errors gracefully).
- Unified service status representation (`Running`, `Stopped`, `Unhealthy`, `ConnectionRefused`, `Timeout`, `Unknown`).
- Distinguishing `required: true` vs. `required: false` (optional) services so an optional service failure does not block the application.
- Executing `start`, `stop`, and `restart` commands defined in `services.json` (resolved for the current OS) via the Command Executor, followed by a post-command health re-check.

### Excluded
- Automatic error-rule matching, retry loops, or cooldowns (covered in Steps 6 and 7).
- Installing or configuring Docker, Apache, PHP, databases, or print servers.
- Assuming all services run inside Docker (e.g., the print server on port `9124` runs independently).
- Exposing a service-management UI page to the client.

---

## Tasks
1. Define the unified service status model in `src-tauri/src/services/models.rs`:
   - `ServiceHealthState` enum (`Healthy`, `Stopped`, `Unhealthy`, `ConnectionRefused`, `Timeout`, `CheckFailed`).
   - `ServiceCheckResult` struct (`service_id`, `required`, `state`, `http_status`, `message`, `latency_ms`, `checked_at`).
2. Implement individual health checkers in `src-tauri/src/services/checks/`:
   - **HTTP Checker** (`reqwest` with per-check timeout): distinguishes connection refused, timeout, and unexpected HTTP status codes (e.g., `500`, `503`) from `expected_status`.
   - **TCP Checker** (`tokio::net::TcpStream::connect` with timeout): distinguishes open port vs. connection refused vs. timeout.
   - **Process Checker** (`sysinfo` or cross-platform process lookup): checks if a configured process name is running on Windows/Linux.
   - **Docker Container Checker**: queries container status safely via predefined `docker` CLI inspection (through Command Executor) without crashing if Docker is stopped or lacks permissions.
3. Implement `ServiceManager` in `src-tauri/src/services/manager.rs`:
   - `check_service(service_id)`: dispatches only to the check type defined in `services.json` (never runs undefined checks).
   - `check_all_services()`: evaluates all configured services and computes overall readiness (all `required: true` services must be `Healthy`; optional service failures are reported/logged without blocking readiness).
   - `start_service(service_id)`, `stop_service(service_id)`, `restart_service(service_id)`: resolves the platform-specific command from `services.json`, runs it via `CommandExecutor`, and re-checks the service status afterward.
   - Handle `null`/missing commands cleanly (e.g., `print-server` with `start: null` returns a clear "no command configured" result).

---

## Expected Files
- `src-tauri/src/services/mod.rs`
- `src-tauri/src/services/models.rs`
- `src-tauri/src/services/manager.rs`
- `src-tauri/src/services/checks/mod.rs`
- `src-tauri/src/services/checks/http.rs`
- `src-tauri/src/services/checks/tcp.rs`
- `src-tauri/src/services/checks/process.rs`
- `src-tauri/src/services/checks/docker.rs`

---

## Technical Details
- **Crates:** `reqwest` (with `rustls-tls` for cross-platform TLS without OpenSSL friction), `tokio::net::TcpStream`, `sysinfo` (for cross-platform process checks).
- **Non-Docker Assumption:** Each service's check and lifecycle commands are driven exclusively by `services.json`. A service on port `9124` is checked as configured (HTTP/TCP/Process) with no Docker assumptions.
- **Timeouts:** Every check strictly enforces its configured `timeout_seconds`.

---

## Acceptance Criteria
- HTTP, TCP, Process, and Docker checks accurately report healthy, stopped/refused, unhealthy (e.g., HTTP 500), and timeout states.
- `required: false` service failures do not prevent overall system readiness.
- `start` and `restart` execute the OS-appropriate command via Command Executor and re-check the service.
- Missing/null lifecycle commands (e.g., `start: null`) are handled gracefully without errors/crashes.
- Docker daemon down or permission denied is caught and reported cleanly as a service/check state.

---

## Testing
- **Unit & Integration Tests (`cargo test`):**
  - HTTP check against a local mock HTTP server returning `200`, `302`, `500`, `503`, and slow/timeout responses.
  - HTTP and TCP checks against an unbound port verifying `ConnectionRefused` detection.
  - TCP check against an active local listener verifying `Healthy`.
  - Process check verifying detection of a running vs. non-existent process name.
  - Readiness aggregation test: 1 required healthy + 1 optional failed -> system ready; 1 required failed -> system not ready.
  - Service `start`/`restart` test verifying Command Executor invocation and post-command re-check.
