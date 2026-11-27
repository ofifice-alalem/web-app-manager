# Step 6: Error Detector & Monitoring Loop

## Objective
Implement the `monitoring` module to perform startup readiness checks (without fixed sleeps) and continuous background monitoring of configured services, reliably detecting only predefined fault conditions and emitting structured trigger events.

---

## Scope

### Included
- Startup readiness polling loop using repeated health checks up to `application.startup_timeout_seconds` (replacing any fixed `sleep` before loading the WebView).
- Background runtime monitoring loop that periodically checks configured services from Rust.
- Detecting predefined trigger conditions matching `recovery-rules.json`:
  - `http_status` (e.g., HTTP 500, 503)
  - `connection_refused`
  - `service_status` (e.g., `stopped`)
  - `tcp_unavailable`
  - `process_not_running`
- Safe, secondary WebView error signal integration for reliably observable client-side load/connection failures, while keeping Rust HTTP/service checks as the primary diagnostic source of truth.
- Ensuring unknown/unconfigured errors are never converted into recovery actions, and HTTP 500 is never blindly assumed to mean Docker is down.

### Excluded
- Executing recovery actions, retries, or cooldowns (covered in Step 7).
- Parsing arbitrary HTTP response bodies or error text into OS commands.
- Relying on inconsistent cross-platform WebView JS/HTTP inspection APIs as the primary detector.

---

## Tasks
1. Define `DetectedCondition` and `TriggerType` models in `src-tauri/src/monitoring/models.rs` aligning 1-to-1 with `recovery-rules.json` trigger types (`http_status`, `connection_refused`, `service_status`, `tcp_unavailable`, `process_not_running`).
2. Implement the **Startup Readiness Coordinator** in `src-tauri/src/monitoring/startup.rs`:
   - Poll `ServiceManager::check_all_services()` at a short interval until either all `required: true` services are healthy or `startup_timeout_seconds` elapses.
   - If a required service is initially stopped/unhealthy and has a matching enabled startup/recovery trigger, emit the condition for the Recovery Engine rather than blindly sleeping.
   - Never block startup if only `required: false` services fail.
3. Implement the **Background Runtime Monitor** in `src-tauri/src/monitoring/monitor.rs`:
   - Run an async background task (`tokio::spawn`) on a configurable polling interval.
   - Inspect configured services via `ServiceManager`.
   - Translate observed `ServiceCheckResult` failures into specific `DetectedCondition` payloads (e.g., `http_status` with status code `500`, `connection_refused`, `tcp_unavailable`, `process_not_running`, `service_status: "stopped"`).
   - Support graceful pause/stop when the application exits, and **skip/suppress** triggering a new recovery for any `service_id` that currently has an active recovery operation in progress (checking the shared recovery-in-progress state before emitting/dispatching a trigger) to prevent overlapping or conflicting recoveries.
4. Implement a minimal, reliable WebView signal bridge (`src-tauri/src/monitoring/webview_signals.rs` + Tauri command/event):
   - Allow the WebView shell to report verified load-error/offline events to Rust, which triggers an immediate Rust-side `ServiceManager` verification check rather than trusting raw browser error text directly.

---

## Expected Files
- `src-tauri/src/monitoring/mod.rs`
- `src-tauri/src/monitoring/models.rs`
- `src-tauri/src/monitoring/startup.rs`
- `src-tauri/src/monitoring/monitor.rs`
- `src-tauri/src/monitoring/webview_signals.rs`

---

## Technical Details
- **Primary Source of Truth:** Rust `ServiceManager` checks (`reqwest`, `tokio::net::TcpStream`, `sysinfo`, Docker check) because WebView2 (Windows) and WebKitGTK (Linux) differ in how they expose HTTP status codes and JS errors.
- **No Blind Sleep:** Startup uses repeated async health checks with `tokio::time::Instant` deadline tracking against `startup_timeout_seconds`.
- **Precision:** An `http_status: 500` condition carries the exact `service_id` and `status_code: 500`, so only rules explicitly matching that service and status code will fire.

---

## Acceptance Criteria
- Application startup waits dynamically via health checks up to `startup_timeout_seconds` and proceeds immediately once required services are healthy.
- Background monitor detects `http_status` (e.g., 500), `connection_refused`, `tcp_unavailable`, `process_not_running`, and `service_status` accurately.
- Unrecognized errors or optional service failures do not trigger false-positive blocking or undefined recovery behavior.
- Active recovery for a service suppresses duplicate/overlapping monitoring triggers for that same service until the recovery completes.
- WebView load-failure signals trigger Rust-side verification rather than direct command execution.

---

## Testing
- **Unit & Integration Tests (`cargo test`):**
  - Startup readiness test: service becomes healthy after 2 failed polls -> readiness succeeds before timeout without fixed sleep.
  - Startup timeout test: required service stays down -> readiness returns timeout condition after `startup_timeout_seconds`.
  - Trigger mapping tests: HTTP 500 -> `http_status(500)`; closed port -> `connection_refused` / `tcp_unavailable`; missing process -> `process_not_running`.
  - Optional service failure test: `required: false` service fails -> startup readiness still passes.
