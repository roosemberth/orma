//! Ask the operator for a passphrase, and hash it.

use std::io::Write;
use std::process::Stdio;

use crate::asker::Asker;
use crate::tool::Tool;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Ask(#[from] crate::asker::Error),
    #[error(transparent)]
    Tool(#[from] crate::tool::Error),
}

/// Exercise what we would do without prompting the question to the operator.
pub fn rehearse_and_hash(asker: &mut Asker) -> Result<Vec<u8>, Error> {
    Ok(crypt_record(&asker.rehearse()?)?)
}

/// Ask for the passphrase guarding `field`, and hash it.
/// If the description is present, it describes the field before prompting.
pub fn prompt_and_hash(
    asker: &mut Asker,
    field: &str,
    description: Option<&str>,
) -> Result<Vec<u8>, Error> {
    if let Some(description) = description {
        asker.say(description);
    }
    let passphrase = asker.ask_guard_against_typos(&format!("Passphrase for {field}: "))?;
    Ok(crypt_record(&passphrase)?)
}

fn crypt_record(passphrase: &str) -> Result<Vec<u8>, crate::tool::Error> {
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
        return Err(mkpasswd.failed(complaint.trim()));
    }
    Ok(hashed.stdout)
}
