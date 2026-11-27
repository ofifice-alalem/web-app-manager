# Desktop Application Host

A cross-platform (**Windows 10/11 x64** and **Ubuntu 24.04 LTS x86_64**) desktop client built with **Tauri 2**, **Vue 3**, **TypeScript**, **Tailwind CSS**, **Rust**, and **SQLite**.

It hosts a single pre-configured web application inside an embedded WebView, verifies required local dependencies in the background, executes predefined recovery rules when known faults occur, and logs recovery activity locally to SQLite.

---

## Scope & Security Boundaries (v1 Client)

- **Single application host:** Configured for one target web system per deployment.
- **No environment provisioning:** Does **not** install Docker, Apache/Nginx, PHP, databases, or print servers. All system dependencies must be pre-installed.
- **Manual launch only:** Started manually by the user from the desktop icon; **no** OS auto-start registration.
- **Minimal client UI:** Displays only the hosted application, a loading/reconnecting indicator, and a generic error fallback if recovery fails. No sidebars, service-management panels, or log viewers.
- **Strict command safety:** Only predefined commands in JSON configuration are executed directly (`executable` + `args`, zero shell interpolation).

---

## Prerequisites

### Windows 10 / 11 (x64)
- **Node.js** 20+ and **npm**
- **Rust** (`rustup` stable MSVC toolchain)
- **Visual Studio 2022 Build Tools** (Desktop development with C++ / `VCTools`)
- **Microsoft Edge WebView2 Runtime** (pre-installed on Windows 11; bundled bootstrapper configured for Windows 10)

### Linux — Linux Mint (Ubuntu 24.04 LTS base, x86_64)
```bash
sudo apt-get update && sudo apt-get install -y \
  build-essential curl wget file libssl-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libgtk-3-dev
```
- **Node.js** 20+ and **npm**
- **Rust** (`rustup` stable toolchain)

---

## Development

```bash
# Install frontend dependencies
npm install

# Run the desktop application in development mode
npm run tauri dev
```

---

## Testing

```bash
# Frontend typecheck, build, and Vitest unit tests
npm run typecheck
npm run build
npm test

# Rust unit, integration, and fault-injection tests
cd src-tauri
cargo test
```

---

## Configuration

Configuration files reside in `config/` during development and in protected OS directories in production:
- **Windows:** `%ProgramData%\desktop-application-host\config\` (administrator-writable ACLs)
- **Linux:** `/etc/desktop-application-host/` (`root`-owned, non-world-writable)
- **Override (optional):** `DESKTOP_HOST_CONFIG_DIR`

Files:
- `config/app.json` — Target application ID, name, URL, `startup_timeout_seconds`, and `reload_after_recovery`.
- `config/services.json` — Monitored services (`http`, `tcp`, `process`, `docker`), `required` flag, and OS-specific `platform_commands` (`windows` / `linux`).
- `config/recovery-rules.json` — Enabled recovery rules, triggers, ordered actions, `retry` policy, and `cooldown_seconds`.
- `config/schemas/` — JSON Schema (Draft 2020-12) definitions for validation and future tooling.

---

## Production Build & Deploy (`deploay/`)

```powershell
# Windows: builds both Seller & Settings apps into deploay/windows/
npm run deploy:win
```

```bash
# Linux Mint: builds both Seller & Settings apps (.deb & .AppImage) into deploay/linux/
bash scripts/build-linux.sh
```

- **Windows (`deploay/windows/`):** NSIS installers (`-setup.exe`) and standalone `.exe` binaries for both apps.
- **Linux Mint (`deploay/linux/`):** `.deb` and `.AppImage` packages for both apps.
