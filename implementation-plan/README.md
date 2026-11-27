# Desktop Application Host — Implementation Plan

This directory contains the step-by-step implementation plan for the **Desktop Application Host** client application (v1), derived from [desktop-host-project-spec-final.md](../desktop-host-project-spec-final.md).

## Overview

The Desktop Application Host is a cross-platform (**Windows** and **Linux**) desktop client built with **Tauri 2**, **Vue 3**, **TypeScript**, **Tailwind CSS**, **Rust**, and **SQLite**. It hosts a single pre-configured web application inside an embedded WebView, monitors its required local dependencies (e.g., a containerized web app and an independent local print server), and executes predefined background recovery rules when known faults occur.

### Key Scope Boundaries (v1 Client)
- **Single-system host:** Configured for one target application per build/deployment.
- **No environment setup:** Does **not** install or provision Docker, web servers, databases, or print servers.
- **Manual launch only:** Started manually by the user from the desktop icon; **no** auto-start on OS boot.
- **Minimal client UI:** Displays only the hosted application, a simple loading/connection state, and a generic user-friendly error message if recovery fails. **No** sidebars, service-management pages, log-viewer pages, or developer settings.
- **Strict security:** Only predefined commands in JSON configuration are executed via `executable` + `args` (no shell interpolation, no dynamic commands from HTTP responses or error text, no UI-triggered arbitrary commands).
- **No developer rule builder:** The developer tool for authoring JSON recovery rules is a separate future project; however, all JSON schemas must be clean, documented, and machine-readable.

---

## Implementation Roadmap & Order

| Step | File | Module / Focus | Depends On |
|---|---|---|---|
| **Step 1** | [01-project-scaffolding-and-desktop-shell.md](./01-project-scaffolding-and-desktop-shell.md) | Tauri 2 + Vue 3 + TS + Tailwind setup, main window, WebView shell, minimal client UI | None |
| **Step 2** | [02-configuration-manager-and-schemas.md](./02-configuration-manager-and-schemas.md) | `app.json`, `services.json`, `recovery-rules.json` schemas, Rust loader & validator, TS types | Step 1 |
| **Step 3** | [03-command-executor.md](./03-command-executor.md) | Secure Rust background command execution, timeouts, no-terminal-window, secret redaction | Step 2 |
| **Step 4** | [04-service-manager.md](./04-service-manager.md) | Health checks (HTTP, TCP, Process, Docker), service status model, start/stop/restart actions | Step 2, Step 3 |
| **Step 5** | [05-log-manager-sqlite.md](./05-log-manager-sqlite.md) | SQLite database, `recovery_events` & `command_executions` tables, redaction, retention policy | Step 2, Step 3 |
| **Step 6** | [06-error-detector-and-monitoring.md](./06-error-detector-and-monitoring.md) | Startup readiness loop, background health monitoring, WebView error signals, trigger detection | Step 2, Step 4 |
| **Step 7** | [07-recovery-engine-and-lifecycle.md](./07-recovery-engine-and-lifecycle.md) | Rule matching, action pipeline, retry/cooldown, per-service lock, WebView reload, UI wiring | Step 1–6 |
| **Step 8** | [08-cross-platform-testing-and-packaging.md](./08-cross-platform-testing-and-packaging.md) | Fault-injection & E2E tests, Windows (WebView2) & Linux (WebKitGTK) builds, project `README.md` | Step 1–7 |

---

## Dependency Graph

```text
Step 1: Project Scaffolding & Desktop Shell
   │
   ▼
Step 2: Configuration Manager & JSON Schemas
   │
   ├──────────────────────────┐
   ▼                          ▼
Step 3: Command Executor      (used by Step 4 & Step 6)
   │           │
   ├───────────┤
   ▼           ▼
Step 4:      Step 5:
Service      Log Manager
Manager      (SQLite)
   │           │
   ▼           │
Step 6:        │
Error Detector │
& Monitoring   │
   │           │
   └─────┬─────┘
         ▼
Step 7: Recovery Engine & Application Lifecycle
         │
         ▼
Step 8: Cross-Platform Testing & Packaging
```

---

## Progression Gate Rules

1. **One step at a time:** Complete all tasks, acceptance criteria, and tests for the current step before starting the next.
2. **No placeholders as done:** Do not mark a step complete using mock data or stubbed commands where real integration is required by that step.
3. **Cross-platform parity:** Verify Rust code compiles and behaves consistently for both Windows and Linux targets at every step (no Windows-only assumptions).
4. **Security & scope discipline:** Reject any addition that introduces shell execution of untrusted strings, auto-start on boot, multi-app management, or admin/management UI panels.
5. **Document decisions:** Record any necessary technical adjustments in the project documentation with clear rationale.
