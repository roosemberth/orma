//! Ask the operator for a passphrase

use std::io::{BufRead, IsTerminal, Write, stderr, stdin};
use std::process::Stdio;

use crate::tool::Tool;

/// How to ask questions to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AskVia {
    /// Straight to this process's terminal.
    #[default]
    Tty,
    /// Through systemd, which routes the question to whichever agent can put
    /// it in front of the operator.
    SystemdAskPassword,
}

impl std::str::FromStr for AskVia {
    type Err = String;

    fn from_str(name: &str) -> Result<AskVia, String> {
        match name {
            "tty" => Ok(AskVia::Tty),
            "systemd-ask-password" => Ok(AskVia::SystemdAskPassword),
            other => Err(format!(
                "unknown way to ask '{other}', expected 'tty' or 'systemd-ask-password'"
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not be read: {0}")]
    Unheard(std::io::Error),
    #[error("passphrases do not match")]
    Mismatch,
    #[error(transparent)]
    Tool(#[from] crate::tool::Error),
}

/// Exercise what prompt_passphrase_and_hash would do without actually prompting
/// the operator.
pub fn rehearse_passphrase_and_hash(via: AskVia) -> Result<Vec<u8>, Error> {
    /// What the rehearsal hashes in the operator's stead.
    const REHEARSAL: &str = "orma rehearsal";
    reachable(via)?;
    crypt_record(REHEARSAL)
}

/// Whether the specified via is likely to be usable.
fn reachable(via: AskVia) -> Result<(), Error> {
    match via {
        // The prompt goes to this process's own stdio.
        AskVia::Tty => Ok(()),
        AskVia::SystemdAskPassword => {
            let agent = Tool::SYSTEMD_ASK_PASSWORD;
            let answered = agent
                .command()
                .arg("--help")
                .output()
                .map_err(|err| agent.failed(format!("could not be started: {err}")))?;
            match answered.status.success() {
                true => Ok(()),
                false => Err(agent.failed("is present but does not run").into()),
            }
        }
    }
}

/// Ask the operator for the passphrase guarding `field`, and hash it.
/// If the description is present, is describes the field before prompting.
pub fn prompt_passphrase_and_hash(
    field: &str,
    description: Option<&str>,
    via: AskVia,
) -> Result<Vec<u8>, Error> {
    if let Some(description) = description {
        let mut err = stderr();
        writeln!(err, "{}", description.trim_end()).ok();
        err.flush().ok();
    }
    let passphrase = ask(&format!("Passphrase for {field}: "), via)?;
    if passphrase != ask("Confirm: ", via)? {
        return Err(Error::Mismatch);
    }
    crypt_record(&passphrase)
}

fn ask(prompt: &str, via: AskVia) -> Result<String, Error> {
    match via {
        AskVia::Tty => ask_tty(prompt),
        AskVia::SystemdAskPassword => ask_through_systemd(prompt),
    }
}

/// Print the prompt to stderr to keep it out of anything redirecting stdout.
fn ask_tty(prompt: &str) -> Result<String, Error> {
    let mut err = stderr();
    write!(err, "{prompt}").ok();
    err.flush().ok();
    if stdin().is_terminal() {
        return rpassword::read_password().map_err(Error::Unheard);
    }
    let mut line = String::new();
    stdin()
        .lock()
        .read_line(&mut line)
        .map_err(Error::Unheard)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

/// Hand the question to systemd and await the response.
fn ask_through_systemd(prompt: &str) -> Result<String, Error> {
    let agent = Tool::SYSTEMD_ASK_PASSWORD;
    let asked = agent
        .command()
        .arg(prompt)
        .output()
        .map_err(|err| agent.failed(format!("could not be started: {err}")))?;
    if !asked.status.success() {
        let complaint = String::from_utf8_lossy(&asked.stderr);
        return Err(agent.failed(complaint.trim()).into());
    }
    let answer = String::from_utf8(asked.stdout)
        .map_err(|_| agent.failed("answered with something that is not text"))?;
    Ok(answer.trim_end_matches(['\n', '\r']).to_owned())
}

pub fn crypt_record(passphrase: &str) -> Result<Vec<u8>, Error> {
    let mkpasswd = Tool::MKPASSWD;
    let mut child = mkpasswd
        .command()
        .args(["-m", "yescrypt", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| mkpasswd.failed(format!("could not be started: {err}")))?;

    if let Some(mut sink) = child.stdin.take() {
        sink.write_all(passphrase.as_bytes())
            .map_err(|err| mkpasswd.failed(err))?;
    }

    let hashed = child
        .wait_with_output()
        .map_err(|err| mkpasswd.failed(err))?;
    if !hashed.status.success() {
        let complaint = String::from_utf8_lossy(&hashed.stderr);
        return Err(mkpasswd.failed(complaint.trim()).into());
    }
    Ok(hashed.stdout)
}
