/// The type of value a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// The 32 hexadecimal characters systemd identifies a machine by.
    MachineId,
}

impl FieldKind {
    /// The spelling a schema uses, or `None` for a type orma does not know.
    pub fn parse(type_name: &str) -> Option<FieldKind> {
        match type_name {
            "machine-id" => Some(FieldKind::MachineId),
            _ => None,
        }
    }

    pub fn validate(&self, value: &[u8]) -> Result<(), Invalid> {
        match self {
            FieldKind::MachineId => validate_machine_id(value),
        }
    }

    /// How a value of this type is produced.
    pub fn recipe(&self) -> Recipe {
        match self {
            FieldKind::MachineId => Recipe::FromEntropy {
                bytes: 16,
                build: build_machine_id,
            },
        }
    }
}

/// The recipe for the world to create a value.
#[derive(Debug, Clone, Copy)]
pub enum Recipe {
    FromEntropy {
        bytes: usize,
        build: fn(&[u8]) -> Vec<u8>,
    },
}

fn build_machine_id(entropy: &[u8]) -> Vec<u8> {
    let mut value = String::with_capacity(33);
    for byte in entropy {
        value.push_str(&format!("{byte:02x}"));
    }
    value.push('\n');
    value.into_bytes()
}

/// The value did not satisfy the field type validator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    #[error("value is not valid UTF-8")]
    NotUtf8,
    #[error("expected 32 characters, found {found}")]
    WrongLength { found: usize },
    #[error("expected lowercase hexadecimal characters")]
    NotLowercaseHex,
}

fn validate_machine_id(value: &[u8]) -> Result<(), Invalid> {
    let text = std::str::from_utf8(value).map_err(|_| Invalid::NotUtf8)?;
    let text = text.trim_end_matches(['\n', '\r']);

    let found = text.chars().count();
    if found != 32 {
        return Err(Invalid::WrongLength { found });
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(Invalid::NotLowercaseHex);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(value: &[u8]) -> Result<(), Invalid> {
        FieldKind::MachineId.validate(value)
    }

    #[test]
    fn a_machine_id_is_made_out_of_sixteen_bytes_of_randomness() {
        let entropy = [
            0xd2, 0xc8, 0xe7, 0xe9, 0xa4, 0xb3, 0x4d, 0x62, //
            0xb8, 0xf8, 0xa0, 0xc5, 0xe9, 0xd7, 0xf3, 0xb1,
        ];
        let Recipe::FromEntropy { bytes, build } = FieldKind::MachineId.recipe();
        assert_eq!(bytes, entropy.len());

        let value = build(&entropy);
        assert_eq!(value, b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n");
        assert!(FieldKind::MachineId.validate(&value).is_ok());
    }

    #[test]
    fn the_type_is_known_by_the_name_a_schema_uses() {
        assert_eq!(FieldKind::parse("machine-id"), Some(FieldKind::MachineId));
        assert_eq!(FieldKind::parse("hashed-password"), None);
    }

    #[test]
    fn a_machine_id_is_32_lowercase_hex_characters() {
        assert!(validate(b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1").is_ok());
    }

    /// systemd writes the file with a trailing newline.
    #[test]
    fn a_trailing_newline_is_allowed() {
        assert!(validate(b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n").is_ok());
    }

    #[test]
    fn the_wrong_length_is_refused() {
        assert_eq!(validate(b"abcd"), Err(Invalid::WrongLength { found: 4 }));
        assert_eq!(validate(b""), Err(Invalid::WrongLength { found: 0 }));
    }

    #[test]
    fn uppercase_is_refused() {
        assert_eq!(
            validate(b"D2C8E7E9A4B34D62B8F8A0C5E9D7F3B1"),
            Err(Invalid::NotLowercaseHex)
        );
    }

    #[test]
    fn characters_outside_hexadecimal_are_refused() {
        assert_eq!(
            validate(b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
            Err(Invalid::NotLowercaseHex)
        );
    }

    #[test]
    fn a_value_that_is_not_text_is_refused() {
        assert_eq!(validate(&[0xff, 0xfe]), Err(Invalid::NotUtf8));
    }
}
