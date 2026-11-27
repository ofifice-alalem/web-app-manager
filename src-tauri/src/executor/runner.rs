//! Cross-platform background command runner with process-tree timeout termination.
//!
//! Security guarantees:
//! - Never executes commands through a shell (`cmd /c`, `sh -c`).
//! - Passes `executable` and `args` separately to the OS process API.
//! - Suppresses console windows on Windows (`CREATE_NO_WINDOW`).
//! - Terminates the entire process tree on timeout.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

use super::models::{CommandResult, CommandSpec, CommandStatus, CommandStep};
use super::redactor::redact_sensitive;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Executes predefined background commands securely.
#[derive(Debug, Default, Clone)]
pub struct CommandExecutor;

impl CommandExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Executes a single command and enforces its timeout, capturing redacted output.
    pub async fn execute(&self, spec: &CommandSpec) -> CommandResult {
        let started = Instant::now();

        if spec.timeout_seconds == 0 {
            return Self::spawn_error_result(
                spec,
                started,
                "Command timeout_seconds must be greater than 0".to_string(),
            );
        }

        let resolved_exe = match Self::resolve_executable(&spec.executable) {
            Ok(path) => path,
            Err(reason) => return Self::spawn_error_result(spec, started, reason),
        };

        let resolved_cwd = match Self::validate_working_dir(spec.working_dir.as_deref()) {
            Ok(dir) => dir,
            Err(reason) => return Self::spawn_error_result(spec, started, reason),
        };

        let mut cmd = Command::new(&resolved_exe);
        cmd.args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(cwd) = &resolved_cwd {
            cmd.current_dir(cwd);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        #[cfg(unix)]
        {
            // Place the child in its own process group so timeout kills the full tree.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return Self::spawn_error_result(
                    spec,
                    started,
                    format!("Failed to spawn '{}': {err}", resolved_exe.display()),
                );
            }
        };

        #[cfg(windows)]
        let job_guard = WindowsJobObject::attach_to_child(&child);

        #[cfg(unix)]
        let child_pgid = child.id().map(|pid| pid as libc::pid_t);

        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();

        let io_and_wait = async {
            let stdout_fut = async {
                let mut buf = Vec::new();
                if let Some(ref mut out) = stdout_pipe {
                    let _ = out.read_to_end(&mut buf).await;
                }
                String::from_utf8_lossy(&buf).into_owned()
            };
            let stderr_fut = async {
                let mut buf = Vec::new();
                if let Some(ref mut err) = stderr_pipe {
                    let _ = err.read_to_end(&mut buf).await;
                }
                String::from_utf8_lossy(&buf).into_owned()
            };

            let (stdout_raw, stderr_raw, status_res) =
                tokio::join!(stdout_fut, stderr_fut, child.wait());
            (stdout_raw, stderr_raw, status_res)
        };

        let timeout_duration = Duration::from_secs(spec.timeout_seconds);
        match tokio::time::timeout(timeout_duration, io_and_wait).await {
            Ok((stdout_raw, stderr_raw, Ok(exit_status))) => {
                let exit_code = exit_status.code();
                let success = exit_status.success();
                CommandResult {
                    executable: spec.executable.clone(),
                    args: spec.args.clone(),
                    stdout: redact_sensitive(&stdout_raw),
                    stderr: redact_sensitive(&stderr_raw),
                    exit_code,
                    status: if success {
                        CommandStatus::Succeeded
                    } else {
                        CommandStatus::Failed
                    },
                    success,
                    timed_out: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    error_message: if success {
                        None
                    } else {
                        Some(format!("Process exited with code {:?}", exit_code))
                    },
                }
            }
            Ok((stdout_raw, stderr_raw, Err(wait_err))) => CommandResult {
                executable: spec.executable.clone(),
                args: spec.args.clone(),
                stdout: redact_sensitive(&stdout_raw),
                stderr: redact_sensitive(&stderr_raw),
                exit_code: None,
                status: CommandStatus::Failed,
                success: false,
                timed_out: false,
                duration_ms: started.elapsed().as_millis() as u64,
                error_message: Some(format!("Failed while waiting for process: {wait_err}")),
            },
            Err(_) => {
                // Terminate the entire process tree on timeout.
                #[cfg(windows)]
                {
                    if let Some(ref job) = job_guard {
                        job.terminate_tree();
                    }
                }
                #[cfg(unix)]
                {
                    if let Some(pgid) = child_pgid {
                        unsafe {
                            libc::kill(-pgid, libc::SIGKILL);
                        }
                    }
                }

                let _ = child.kill().await;
                let _ = child.wait().await;

                CommandResult {
                    executable: spec.executable.clone(),
                    args: spec.args.clone(),
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: None,
                    status: CommandStatus::TimedOut,
                    success: false,
                    timed_out: true,
                    duration_ms: started.elapsed().as_millis() as u64,
                    error_message: Some(format!(
                        "Command timed out after {} second(s)",
                        spec.timeout_seconds
                    )),
                }
            }
        }
    }

    /// Executes a sequence of commands in order, stopping on failure
    /// unless `continue_on_failure` is `true` for that step.
    pub async fn execute_sequence(&self, steps: &[CommandStep]) -> Vec<CommandResult> {
        let mut results = Vec::with_capacity(steps.len());
        for step in steps {
            let result = self.execute(&step.spec).await;
            let should_stop = !result.success && !step.continue_on_failure;
            results.push(result);
            if should_stop {
                break;
            }
        }
        results
    }

    /// Resolves and verifies executable existence and permissions without using a shell.
    pub fn resolve_executable(executable: &str) -> Result<PathBuf, String> {
        let trimmed = executable.trim();
        if trimmed.is_empty() {
            return Err("Executable name must not be empty".to_string());
        }

        let candidate = Path::new(trimmed);
        if trimmed.contains('/') || trimmed.contains('\\') {
            return Self::verify_executable_file(candidate)
                .then(|| candidate.to_path_buf())
                .ok_or_else(|| {
                    format!(
                        "Executable '{}' does not exist or is not accessible",
                        trimmed
                    )
                });
        }

        let path_var = std::env::var_os("PATH")
            .ok_or_else(|| "PATH environment variable is not set".to_string())?;

        for dir in std::env::split_paths(&path_var) {
            let direct = dir.join(trimmed);
            if Self::verify_executable_file(&direct) {
                return Ok(direct);
            }

            #[cfg(windows)]
            if candidate.extension().is_none() {
                let pathext = std::env::var("PATHEXT")
                    .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
                for ext in pathext.split(';').filter(|e| !e.is_empty()) {
                    let with_ext = dir.join(format!("{trimmed}{ext}"));
                    if Self::verify_executable_file(&with_ext) {
                        return Ok(with_ext);
                    }
                }
            }
        }

        Err(format!("Executable '{}' was not found in PATH", trimmed))
    }

    fn verify_executable_file(path: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    fn validate_working_dir(working_dir: Option<&str>) -> Result<Option<PathBuf>, String> {
        let Some(dir) = working_dir else {
            return Ok(None);
        };
        let path = PathBuf::from(dir);
        if path.is_dir() {
            Ok(Some(path))
        } else {
            Err(format!("Working directory '{}' does not exist", dir))
        }
    }

    fn spawn_error_result(spec: &CommandSpec, started: Instant, message: String) -> CommandResult {
        CommandResult {
            executable: spec.executable.clone(),
            args: spec.args.clone(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            status: CommandStatus::SpawnError,
            success: false,
            timed_out: false,
            duration_ms: started.elapsed().as_millis() as u64,
            error_message: Some(message),
        }
    }
}

#[cfg(windows)]
struct WindowsJobObject {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsJobObject {
    fn attach_to_child(child: &Child) -> Option<Self> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let raw_process = child.raw_handle()?;
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ok_set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok_set == 0 {
                CloseHandle(job);
                return None;
            }

            let ok_assign = AssignProcessToJobObject(job, raw_process as _);
            if ok_assign == 0 {
                CloseHandle(job);
                return None;
            }

            Some(Self { handle: job })
        }
    }

    fn terminate_tree(&self) {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        unsafe {
            TerminateJobObject(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJobObject {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_spec(text: &str) -> CommandSpec {
        #[cfg(windows)]
        {
            CommandSpec {
                executable: "powershell.exe".to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    format!("Write-Output '{text}'"),
                ],
                working_dir: None,
                timeout_seconds: 10,
            }
        }
        #[cfg(not(windows))]
        {
            CommandSpec {
                executable: "echo".to_string(),
                args: vec![text.to_string()],
                working_dir: None,
                timeout_seconds: 10,
            }
        }
    }

    fn failing_spec() -> CommandSpec {
        #[cfg(windows)]
        {
            CommandSpec {
                executable: "powershell.exe".to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    "[Console]::Error.WriteLine('err password=mysecret'); exit 2".to_string(),
                ],
                working_dir: None,
                timeout_seconds: 10,
            }
        }
        #[cfg(not(windows))]
        {
            CommandSpec {
                executable: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "echo 'err password=mysecret' >&2; exit 2".to_string(),
                ],
                working_dir: None,
                timeout_seconds: 10,
            }
        }
    }

    fn slow_tree_spec() -> CommandSpec {
        #[cfg(windows)]
        {
            CommandSpec {
                executable: "powershell.exe".to_string(),
                args: vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    "Start-Process -NoNewWindow -FilePath powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30'; Start-Sleep -Seconds 30".to_string(),
                ],
                working_dir: None,
                timeout_seconds: 1,
            }
        }
        #[cfg(not(windows))]
        {
            CommandSpec {
                executable: "sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "sleep 30 & wait".to_string(),
                ],
                working_dir: None,
                timeout_seconds: 1,
            }
        }
    }

    #[tokio::test]
    async fn executes_command_and_captures_stdout() {
        let executor = CommandExecutor::new();
        let res = executor.execute(&echo_spec("hello_executor")).await;

        assert!(res.success);
        assert_eq!(res.status, CommandStatus::Succeeded);
        assert_eq!(res.exit_code, Some(0));
        assert!(res.stdout.contains("hello_executor"));
    }

    #[tokio::test]
    async fn captures_non_zero_exit_and_redacts_stderr() {
        let executor = CommandExecutor::new();
        let res = executor.execute(&failing_spec()).await;

        assert!(!res.success);
        assert_eq!(res.status, CommandStatus::Failed);
        assert_eq!(res.exit_code, Some(2));
        assert!(!res.stderr.contains("mysecret"));
        assert!(res.stderr.contains("password=[REDACTED]"));
    }

    #[tokio::test]
    async fn enforces_timeout_and_terminates_process_tree() {
        let executor = CommandExecutor::new();
        let res = executor.execute(&slow_tree_spec()).await;

        assert!(!res.success);
        assert!(res.timed_out);
        assert_eq!(res.status, CommandStatus::TimedOut);
    }

    #[tokio::test]
    async fn returns_spawn_error_for_missing_executable() {
        let executor = CommandExecutor::new();
        let res = executor
            .execute(&CommandSpec {
                executable: "definitely_non_existent_binary_xyz_123".to_string(),
                args: vec![],
                working_dir: None,
                timeout_seconds: 5,
            })
            .await;

        assert!(!res.success);
        assert_eq!(res.status, CommandStatus::SpawnError);
    }

    #[tokio::test]
    async fn sequence_stops_when_continue_on_failure_is_false() {
        let executor = CommandExecutor::new();
        let results = executor
            .execute_sequence(&[
                CommandStep {
                    spec: failing_spec(),
                    continue_on_failure: false,
                },
                CommandStep {
                    spec: echo_spec("should_not_run"),
                    continue_on_failure: false,
                },
            ])
            .await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CommandStatus::Failed);
    }

    #[tokio::test]
    async fn sequence_continues_when_continue_on_failure_is_true() {
        let executor = CommandExecutor::new();
        let results = executor
            .execute_sequence(&[
                CommandStep {
                    spec: failing_spec(),
                    continue_on_failure: true,
                },
                CommandStep {
                    spec: echo_spec("ran_second_step"),
                    continue_on_failure: false,
                },
            ])
            .await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, CommandStatus::Failed);
        assert_eq!(results[1].status, CommandStatus::Succeeded);
        assert!(results[1].stdout.contains("ran_second_step"));
    }

    #[tokio::test]
    async fn passes_shell_metacharacters_literally_without_shell_interpretation() {
        let executor = CommandExecutor::new();
        let res = executor.execute(&echo_spec("safe; echo injected")).await;

        assert!(res.success);
        assert!(res.stdout.contains("safe; echo injected"));
    }
}
