# Step 1: Project Scaffolding & Desktop Shell

## Objective
Initialize the Tauri 2 desktop application project with Vue 3, TypeScript, and Tailwind CSS, establish the modular directory structure, and create a functional standalone desktop window hosting an embedded WebView with a minimal client-only UI.

---

## Scope

### Included
- Initializing the Tauri 2 + Vite + Vue 3 + TypeScript + Tailwind CSS workspace.
- Setting up the modular folder structure for both frontend (`src/`) and backend (`src-tauri/src/`).
- Configuring Tauri window properties and capabilities (`tauri.conf.json`, `capabilities/`).
- Creating the minimal client UI shell (loading/connecting state and generic error fallback view).
- Loading a test/placeholder URL inside the embedded WebView without opening an external browser.
- Preventing internal application links from opening in an external browser unnecessarily.
- Ensuring the application is **not** configured to auto-start on Windows or Linux boot.

### Excluded
- Reading or validating `config/*.json` files (covered in Step 2).
- Executing OS commands or managing services (covered in Steps 3 and 4).
- SQLite logging (covered in Step 5).
- Sidebars, service-management pages, log-viewer pages, or developer settings pages (explicitly excluded from v1).
- Auto-start/autostart plugins or OS startup registration.

---

## Tasks
1. Initialize the Tauri 2 project with Vue 3, TypeScript, and Tailwind CSS using official Tauri 2-compatible package versions.
2. Create the modular directory layout:
   - Frontend: `src/components/`, `src/views/`, `src/composables/`, `src/services/`, `src/types/`, `src/assets/`
   - Backend: `src-tauri/src/commands/`, `config/`, `services/`, `monitoring/`, `recovery/`, `executor/`, `database/`, `browser/` (as module stubs/declarations ready for subsequent steps)
   - Root assets/config placeholders: `config/`, `resources/icons/`
3. Configure `src-tauri/tauri.conf.json`:
   - Set window title, default dimensions, resizable behavior, and application identifier.
   - Configure Content Security Policy (CSP) and Tauri 2 capabilities (`src-tauri/capabilities/`) following least-privilege principles.
   - Ensure no `autostart` plugin or startup registration is included.
4. Implement the `browser` module in Rust to manage the main application window and WebView navigation/reload primitives, including intercepting navigation so internal links stay within the WebView rather than launching an external browser.
5. Build the minimal Vue 3 client UI:
   - A clean loading/status overlay shown while startup checks are pending.
   - A generic, non-technical error view for when the hosted system is unreachable.
   - No sidebars, navigation menus, service lists, log viewers, or settings panels.
6. Define the initial typed Tauri IPC boundary (`src/services/` and `src-tauri/src/commands/`) so Vue never accesses system APIs directly.
7. Verify launching the app opens a standalone window displaying a test local URL inside the WebView.

---

## Expected Files
- `package.json`, `tsconfig.json`, `vite.config.ts`, `tailwind.config.*`
- `index.html`
- `src/App.vue`, `src/main.ts`
- `src/views/` (minimal host/error/loading views)
- `src/components/` (minimal status/error indicator components)
- `src/composables/`, `src/services/`, `src/types/`, `src/assets/`
- `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`
- `src-tauri/capabilities/default.json`
- `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/commands/mod.rs`
- `resources/icons/`

---

## Technical Details
- **Stack:** Tauri 2, Rust, Vue 3 (Composition API), TypeScript, Tailwind CSS.
- **WebView Engines:** WebView2 on Windows, WebKitGTK on Linux (via Tauri 2's `wry` runtime).
- **Architecture:** Strict separation between frontend (UI state only) and backend (all system, network, and process operations). Vue communicates with Rust exclusively through explicit Tauri commands/events.
- **Navigation Control:** Internal URLs must remain inside the embedded WebView; external browser spawning is suppressed for application navigation.

---

## Acceptance Criteria
- Application launches manually as a standalone desktop window on the development machine.
- A test URL renders inside the embedded WebView without opening an external browser.
- No sidebar, services page, logs page, or settings page exists in the UI.
- No auto-start configuration exists for Windows or Linux.
- Rust and TypeScript compile cleanly with zero errors.

---

## Testing
- **Build & Type Check:** Run `npm run build` (or `vue-tsc --noEmit`) and `cargo check` inside `src-tauri/`.
- **Manual Smoke Test:** Launch `cargo tauri dev` and verify:
  - Standalone window opens with expected title and dimensions.
  - Test URL loads inside the WebView.
  - Loading state and generic error fallback render cleanly when toggled.
  - Clicking internal links stays inside the WebView.
