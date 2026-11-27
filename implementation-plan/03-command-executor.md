# Step 3: Secure Command Executor

## Objective
Implement the Rust `executor` module to run predefined OS commands in the background with strict security controls, timeout enforcement, output capture, sequential execution, and zero visible terminal windows.

---

## Scope

### Included
- Executing a single predefined command using separate `executable` and `args` (never via shell interpolation).
- Supporting optional `working_directory` and mandatory `timeout_seconds`.
- Capturing `stdout`, `stderr`, `exit_code`, and execution duration (`duration_ms`).
- Sequential execution of command lists, stopping immediately when a step with `continue_on_failure: false` fails.
- Hiding console/terminal windows on Windows (`CREATE_NO_WINDOW`).
- Verifying executable availability and access permissions before/during execution.
- Implementing a secret/sensitive-data redaction utility for captured `stdout`/`stderr`.
- Handling permission errors (e.g., Docker socket permission denied) gracefully without crashing the application.

### Excluded
- Executing arbitrary commands from the Vue frontend.
- Converting HTTP responses, error messages, or log text into executable commands.
- Invoking `cmd.exe /c` or `sh -c` to run composite shell strings.
- Running the application permanently as Administrator or Root.
- Installing Docker or system services.

---

## Tasks
1. Define the `CommandSpec` and `CommandResult` types in `src-tauri/src/executor/models.rs`:
   - `CommandSpec`: `executable`, `args`, `working_dir` (optional), `timeout_seconds`.
   - `CommandResult`: `executable`, `args`, `stdout`, `stderr`, `exit_code`, `success`, `timed_out`, `duration_ms`, `error_message`.
2. Implement single-command execution in `src-tauri/src/executor/runner.rs` using `tokio::process::Command` (or `std::process::Command` with async wrapper):
   - Pass `executable` and `args` directly to the OS process API (no shell).
   - Set `current_dir` when `working_dir` is provided and validated.
   - Apply `CREATE_NO_WINDOW` (`0x08000000`) on Windows (`#[cfg(target_os = "windows")]`) so no Command Prompt window ever flashes.
   - Enforce `timeout_seconds` using `tokio::time::timeout`; when a timeout expires, terminate the **entire process tree** (the main process and all spawned child/descendant processes) so no orphaned background processes remain running.
3. Implement pre-flight executable resolution/validation:
   - Check whether the executable exists (absolute path or resolvable on `PATH`) and is executable, returning a structured diagnostic error if missing or inaccessible.
4. Implement sequential command chain execution:
   - Execute steps in order.
   - Halt the sequence immediately if a step fails (non-zero exit code, spawn error, or timeout) and `continue_on_failure` is `false`.
   - Return per-step results for logging and recovery evaluation.
5. Implement a sensitive-data redactor (`src-tauri/src/executor/redactor.rs`) to scrub passwords, tokens, bearer headers, and key=value secrets from `stdout` and `stderr` before returning/persisting logs.
6. Ensure no Tauri command exposes raw command execution to the frontend; only internal Rust modules (`services`, `recovery`) may invoke the executor with validated config-defined commands.

---

## Expected Files
- `src-tauri/src/executor/mod.rs`
- `src-tauri/src/executor/models.rs`
- `src-tauri/src/executor/runner.rs`
- `src-tauri/src/executor/redactor.rs`

---

## Technical Details
- **Async Runtime:** `tokio::process::Command` with `tokio::time::timeout` for non-blocking background execution.
- **Process Tree Termination on Timeout:**
  - **Windows:** Assign the spawned process to a Win32 Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) and call `TerminateJobObject` on timeout so all child processes are killed together.
  - **Linux:** Spawn the child in a dedicated process group (`setpgid(0, 0)`) and send `SIGKILL` to the entire process group (`-pgid`) on timeout.
- **Windows Console Suppression:** `use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000);` guarded with `#[cfg(windows)]`.
- **Cross-Platform:** Standard POSIX process spawning on Linux; Windows-specific flags isolated via `#[cfg(target_os = "windows")]`.
- **Security Guarantees:**
  - Zero shell invocation (`sh -c`, `cmd /c` prohibited).
  - Commands originate strictly from validated `services.json` or `recovery-rules.json`.
  - Elevated privileges, if ever required by a specific command, must be explicitly handled per OS rather than running the host app as root/admin.

---

## Acceptance Criteria
- Commands execute with separate `executable` and `args` without spawning a shell.
- No terminal or Command Prompt window appears on Windows or Linux.
- `stdout`, `stderr`, `exit_code`, and `duration_ms` are accurately captured.
- Exceeding `timeout_seconds` terminates the process and reports a timeout result.
- Sequential chains stop on failure when `continue_on_failure` is `false`.
- Missing executables or permission-denied errors return structured failures without panicking.
- Passwords/tokens in command output are redacted.

---

## Testing
- **Unit & Integration Tests (`cargo test`):**
  - Successful command execution capturing `stdout` and `exit_code == 0`.
  - Non-zero exit code capturing `stderr` and `exit_code != 0`.
  - Timeout enforcement on a long-running test process, verifying that both the main process and any spawned child processes are terminated.
  - Sequential execution halting on failure (`continue_on_failure: false`) vs. continuing (`continue_on_failure: true`).
  - Missing executable handling (returns structured error, no panic).
  - Secret redaction tests (passwords, tokens, API keys masked in output).
  - Verification that arguments with spaces or shell metacharacters (e.g., `;`, `&&`, `|`) are passed literally as arguments and never interpreted by a shell.
