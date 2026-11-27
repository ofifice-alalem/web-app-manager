# Step 8: Cross-Platform Testing, Packaging & Release Readiness

## Objective
Validate the complete Desktop Application Host across both **Windows** and **Linux** against all fault-injection scenarios, configure production installers/packages for both platforms, and finalize project documentation.

---

## Scope

### Included
- End-to-end and fault-injection testing for every scenario required by the specification:
  - Windows execution & packaging.
  - Linux execution & packaging (explicitly targeting a supported distribution/package format first, with clear extensibility).
  - Stopped required services on startup and runtime.
  - HTTP 500 error detection and rule-based recovery.
  - Connection Refused error detection and recovery.
  - Recovery command failure & retry exhaustion.
  - Command and health-check timeout expiration.
  - Repeated errors & cooldown / anti-loop verification.
  - Optional (`required: false`) service failure without blocking the main application.
- Configuring Tauri 2 build targets and bundling `config/` and `resources/icons/` for Windows (`.msi` / `.exe` with WebView2) and Linux (`.deb` / `AppImage` with WebKitGTK).
- Verifying installers do **not** install Docker or external services and do **not** register OS auto-start.
- Updating the root `README.md` with setup, development, configuration, and build instructions for Windows and Linux.

### Excluded
- Bundling or installing Docker, Apache, Nginx, PHP, databases, or print servers in the installer.
- Adding auto-start registry keys, `.desktop` autostart entries, or systemd user services for the client app.
- Adding the future developer rule-builder tool.

---

## Tasks
1. Create an end-to-end / fault-injection test harness (using local mock HTTP/TCP servers and test scripts/commands) covering all required failure modes:
   - Stopped required service at startup -> auto-started in background -> WebView loads.
   - HTTP 500 on monitored endpoint -> matching rule executes -> service recovers -> WebView reloads.
   - Connection Refused (e.g., stopped service or print server on port `9124`) -> detected -> recovery attempted -> generic error shown if unrecoverable.
   - Recovery command returns non-zero exit code -> stops chain -> retries up to `max_attempts` -> logs failure to SQLite.
   - Slow/hanging command or health check -> hits timeout cleanly -> kills process -> logs timeout.
   - Repeated fault within `cooldown_seconds` -> suppressed (no infinite recovery or reload loop).
   - Optional service (`required: false`) down -> logged, main application still opens and functions.
2. Configure `src-tauri/tauri.conf.json` bundling settings for production:
   - Bundle `resources/icons/` and default `config/` templates cleanly.
   - Configure Windows bundle (NSIS/MSI targeting Windows 10/11 x64, with WebView2 bootstrapper/runtime expectations).
   - Configure Linux bundle explicitly targeting **Ubuntu 24.04 LTS (x86_64)** as the primary supported distribution for actual testing and verification (producing `.deb` and `AppImage` packages, with `webkit2gtk-4.1` dependencies documented).
3. Run production builds on Windows and Linux (`cargo tauri build`) and verify:
   - App launches manually from icon/binary.
   - No terminal window appears on Windows or Linux.
   - No auto-start entry is created by the installer.
   - SQLite database and external config files resolve properly in installed mode.
4. Update the root `README.md` with:
   - System prerequisites for development and building on Windows (WebView2, MSVC, Rust, Node.js) and Linux (WebKitGTK, `libwebkit2gtk-4.1-dev`, `libssl-dev`, etc.).
   - Development run instructions (`npm install`, `cargo tauri dev`).
   - Production build instructions (`cargo tauri build`).
   - Configuration guide (`config/app.json`, `services.json`, `recovery-rules.json`) and security/scope notes.

---

## Expected Files
- `tests/` (or `src-tauri/tests/` for Rust integration & fault-injection tests)
- `src-tauri/tauri.conf.json` (production bundle configuration for Windows & Linux)
- `README.md` (root project documentation for Windows and Linux)

---

## Technical Details
- **Windows WebView:** Microsoft Edge WebView2 Runtime.
- **Linux WebView:** WebKitGTK (`webkit2gtk-4.1` for Tauri 2).
- **Linux Packaging Strategy:** Explicitly target **Ubuntu 24.04 LTS (x86_64)** for initial packaging (`.deb` and `AppImage`) and end-to-end verification, rather than assuming all Linux distributions behave identically.
- **Installer Boundaries:** Installers deploy only the Desktop Application Host binary, icons, and configuration files—never external system services.

---

## Acceptance Criteria
- All 7 fault-injection / reliability scenarios pass on both Windows and Linux.
- `cargo tauri build` produces installable artifacts for Windows and Linux.
- Installed app launches manually from icon, displays only the hosted system, shows no terminal window, does not auto-start on boot, and persists SQLite logs properly.
- Root `README.md` accurately documents prerequisites, dev workflow, config structure, and build steps for both OSes.

---

## Testing
- **Automated Test Suite:** Run full `cargo test` (unit + integration + fault-injection tests) and `npm run build` / `vue-tsc --noEmit`.
- **Cross-Platform Manual Verification Checklist (Windows & Linux):**
  - [ ] Fresh install from generated package/installer.
  - [ ] Verify no OS auto-start entry exists after install or first run.
  - [ ] Verify stopped service recovery, HTTP 500 recovery, connection refused handling, timeout handling, and retry exhaustion UI.
  - [ ] Inspect SQLite database in app data directory to confirm `recovery_events` and `command_executions` are recorded with secrets redacted.
