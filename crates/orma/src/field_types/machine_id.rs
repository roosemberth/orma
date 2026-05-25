use std::fs::File;
use std::io::Read;

use crate::asker::Asker;

#[derive(Debug)]
pub struct MachineId;

impl MachineId {
    pub(super) fn required_tools(&self) -> &'static [&'static str] {
        &[]
    }

    pub(super) fn validate(&self, raw: &[u8]) -> Result<(), &'static str> {
        let s =
            std::str::from_utf8(raw).map_err(|_| "value is not valid UTF-8")?;
        let s = s.trim_end_matches(['\n', '\r']);
        if s.len() != 32 {
            return Err("machine-id must be exactly 32 hex characters");
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(
                "machine-id must contain only lowercase hex characters",
            );
        }
        Ok(())
    }

    pub(super) fn generate(
        &self,
        _label: &str,
        _asker: &dyn Asker,
    ) -> Result<Vec<u8>, String> {
        let mut bytes = [0u8; 16];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|e| format!("read /dev/urandom: {e}"))?;
        let mut out = String::with_capacity(33);
        for b in bytes {
            out.push_str(&format!("{b:02x}"));
        }
        out.push('\n');
        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_is_valid() {
        let v = b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1";
        assert!(MachineId.validate(v).is_ok());
    }

    #[test]
    fn trailing_newline_is_ok() {
        let v = b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n";
        assert!(MachineId.validate(v).is_ok());
    }

    #[test]
    fn uppercase_fails() {
        assert!(
            MachineId
                .validate(b"D2C8E7E9A4B34D62B8F8A0C5E9D7F3B1")
                .is_err()
        );
    }

    #[test]
    fn wrong_length_fails() {
        assert!(MachineId.validate(b"abcd").is_err());
        assert!(
            MachineId
                .validate(b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b100")
                .is_err()
        );
    }

    #[test]
    fn non_hex_fails() {
        assert!(
            MachineId
                .validate(b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")
                .is_err()
        );
    }

    #[test]
    fn generated_is_valid() {
        use crate::asker::TtyAsker;
        let v = MachineId.generate("/machine-id", &TtyAsker).unwrap();
        assert!(MachineId.validate(&v).is_ok());
    }
}
