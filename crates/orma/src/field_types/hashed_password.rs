#[derive(Debug)]
pub struct HashedPassword;

impl HashedPassword {
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
