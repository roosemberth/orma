//! How orma asks the operator for input. Provided implementations of [`Asker`]:
//!
//! - [`TtyAsker`] reads from stdin directly.
//! - [`SystemdAsker`] shells out to `systemd-ask-password`.

use std::io::{BufRead, IsTerminal, Write, stderr, stdin};
use std::process::Command;

pub trait Asker {
    fn ask(&self, prompt: &str) -> Result<String, String>;
    fn required_tools(&self) -> &'static [&'static str];
}

/// Reads user responses from the process's stdin.
///
/// When stdin is a terminal, uses `rpassword` for echo-off termios so the
/// operator's input doesn't leak into the scrollback.
/// When stdin is piped (tests, scripted invocations), falls back to a plain
/// line-buffered read.
pub struct TtyAsker;

impl Asker for TtyAsker {
    fn required_tools(&self) -> &'static [&'static str] {
        &[]
    }

    fn ask(&self, prompt: &str) -> Result<String, String> {
        let mut err = stderr();
        write!(err, "{prompt}").ok();
        err.flush().ok();
        if stdin().is_terminal() {
            rpassword::read_password().map_err(|e| format!("read: {e}"))
        } else {
            let mut buf = String::new();
            stdin()
                .lock()
                .read_line(&mut buf)
                .map_err(|e| format!("read: {e}"))?;
            Ok(buf.trim_end_matches(['\n', '\r']).to_string())
        }
    }
}

/// Delegates to `systemd-ask-password`.
///
/// The prompt flows through whatever password agent is registered with systemd.
/// This means we don't have to own `/dev/console` ourselves; systemd routes
/// the prompt to wherever the operator can see it.
pub struct SystemdAsker;

impl Asker for SystemdAsker {
    fn required_tools(&self) -> &'static [&'static str] {
        &["systemd-ask-password"]
    }

    fn ask(&self, prompt: &str) -> Result<String, String> {
        let out = Command::new("systemd-ask-password")
            .arg(prompt)
            .output()
            .map_err(|e| format!("spawn systemd-ask-password: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "systemd-ask-password exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let s = String::from_utf8(out.stdout).map_err(|_| {
            "non-utf8 output from systemd-ask-password".to_string()
        })?;
        Ok(s.trim_end_matches(['\n', '\r']).to_string())
    }
}
