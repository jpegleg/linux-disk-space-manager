// runner.rs - execute threshold reaction commands
//
// Each command is run as a child process via `sh -c` so the full POSIX shell
// feature set (pipes, redirects, compound commands) is available in policy YAML.
// stdout/stderr are captured; debug mode prints them; all modes log failures.

use std::process::Command;
use crate::logger::Logger;

/// Run a single shell command, log its outcome, and return whether it succeeded.
/// The command string is passed verbatim to `sh -c`.
pub fn run_command(cmd: &str, log: &Logger) -> bool {
    log.info(&format!("running reaction: {}", cmd));

    let result = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output();

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            if !stdout.trim().is_empty() {
                log.debug(&format!("  stdout: {}", stdout.trim()));
            }
            if !stderr.trim().is_empty() {
                log.warn(&format!("  stderr: {}", stderr.trim()));
            }
            if out.status.success() {
                log.debug(&format!("  ok (exit 0): {}", cmd));
                true
            } else {
                log.error(&format!(
                    "reaction command exited with {}: {}",
                    out.status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".to_string()),
                    cmd
                ));
                false
            }
        }
        Err(e) => {
            log.error(&format!("failed to spawn '{}': {}", cmd, e));
            false
        }
    }
}

pub fn run_commands(commands: &[String], log: &Logger) {
    for cmd in commands {
        run_command(cmd, log);
    }
}
