//! Ask the operator for a passphrase

use std::io::{BufRead, IsTerminal, Write, stderr, stdin};
use std::process::Stdio;

use crate::tool::Tool;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not be read: {0}")]
    Unheard(std::io::Error),
    #[error("passphrases do not match")]
    Mismatch,
    #[error(transparent)]
    Tool(#[from] crate::tool::Error),
}

/// Ask the operator for the passphrase guarding `field`, and hash it.
/// If the description is present, is describes the field before prompting.
pub fn prompt_passphrase_and_hash(
    field: &str,
    description: Option<&str>,
) -> Result<Vec<u8>, Error> {
    if let Some(description) = description {
        let mut err = stderr();
        writeln!(err, "{}", description.trim_end()).ok();
        err.flush().ok();
    }
    let passphrase = ask(&format!("Passphrase for {field}: "))?;
    if passphrase != ask("Confirm: ")? {
        return Err(Error::Mismatch);
    }
    crypt_record(&passphrase)
}

/// Print the prompt to stderr to keep it out of anything redirecting stdout.
fn ask(prompt: &str) -> Result<String, Error> {
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
