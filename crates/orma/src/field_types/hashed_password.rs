use std::io::Write;
use std::process::{Command, Stdio};

use crate::asker::Asker;

#[derive(Debug)]
pub struct HashedPassword;

impl HashedPassword {
    pub(super) fn required_tools(&self) -> &'static [&'static str] {
        &["mkpasswd"]
    }

    pub(super) fn validate(&self, raw: &[u8]) -> Result<(), &'static str> {
        let s =
            std::str::from_utf8(raw).map_err(|_| "value is not valid UTF-8")?;
        let s = s.trim_end_matches(['\n', '\r']);
        if !s.starts_with('$') {
            return Err("not a crypt record: missing leading '$'");
        }
        let parts: Vec<&str> = s.split('$').collect();
        if parts.len() < 4 {
            return Err("not a crypt record: too few sections");
        }
        if parts[1..].iter().any(|p| p.is_empty()) {
            return Err("not a crypt record: contains an empty section");
        }
        Ok(())
    }

    pub(super) fn generate(
        &self,
        field_path: &str,
        asker: &dyn Asker,
    ) -> Result<Vec<u8>, String> {
        let p1 = asker.ask(&format!("Passphrase for {field_path}: "))?;
        let p2 = asker.ask("Confirm: ")?;
        if p1 != p2 {
            return Err("passphrases do not match".into());
        }
        mkpasswd(&p1)
    }
}

fn mkpasswd(passphrase: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("mkpasswd")
        .args(["-m", "yescrypt", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("mkpasswd failed to start: {e}"))?;
    {
        let stdin = child.stdin.as_mut().expect("stdin was piped");
        stdin
            .write_all(passphrase.as_bytes())
            .map_err(|e| format!("mkpasswd stdin: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("mkpasswd wait: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "mkpasswd failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yescrypt_is_valid() {
        let v = b"$y$j9T$saltSaltSalt$hashHashHashHashHash";
        assert!(HashedPassword.validate(v).is_ok());
    }

    #[test]
    fn sha512_with_rounds_is_valid() {
        let v = b"$6$rounds=5000$saltsalt$hashhash";
        assert!(HashedPassword.validate(v).is_ok());
    }

    #[test]
    fn trailing_newline_is_ok() {
        let v = b"$6$salt$hash\n";
        assert!(HashedPassword.validate(v).is_ok());
    }

    #[test]
    fn missing_leading_dollar_fails() {
        assert!(HashedPassword.validate(b"notacrypthash").is_err());
    }

    #[test]
    fn empty_section_fails() {
        assert!(HashedPassword.validate(b"$y$$j9T$hash").is_err());
    }

    #[test]
    fn too_few_sections_fails() {
        assert!(HashedPassword.validate(b"$y").is_err());
    }

    #[test]
    fn non_utf8_fails() {
        assert!(HashedPassword.validate(&[0xff, 0xfe]).is_err());
    }
}
