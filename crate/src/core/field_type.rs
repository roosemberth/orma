/// The type of value a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// The 32 hexadecimal characters systemd identifies a machine by.
    MachineId,
    /// A crypt record, as `/etc/shadow` and the PAM stack understand one.
    HashedPassword,
}

impl FieldKind {
    /// The spelling a schema uses, or `None` for a type orma does not know.
    pub fn parse(type_name: &str) -> Option<FieldKind> {
        match type_name {
            "machine-id" => Some(FieldKind::MachineId),
            "hashed-password" => Some(FieldKind::HashedPassword),
            _ => None,
        }
    }

    /// The spelling a schema uses for this type.
    pub fn name(&self) -> &'static str {
        match self {
            FieldKind::MachineId => "machine-id",
            FieldKind::HashedPassword => "hashed-password",
        }
    }

    pub fn validate(&self, value: &[u8]) -> Result<(), Invalid> {
        let text = std::str::from_utf8(value).map_err(|_| Invalid::NotUtf8)?;
        let text = text.trim_end_matches(['\n', '\r']);
        match self {
            FieldKind::MachineId => Ok(validate_machine_id(text)?),
            FieldKind::HashedPassword => Ok(validate_crypt_record(text)?),
        }
    }

    /// The mode a value of this type is stored under.
    pub fn permissions(&self) -> u32 {
        match self {
            FieldKind::MachineId => 0o644,
            FieldKind::HashedPassword => 0o600,
        }
    }

    /// How a value of this type is produced, or `None` if unknown/unable.
    pub fn recipe(&self) -> Option<Recipe> {
        match self {
            FieldKind::MachineId => Some(Recipe::FromEntropy {
                bytes: 16,
                build: build_machine_id,
            }),
            FieldKind::HashedPassword => Some(Recipe::FromPassphrasePrompt),
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
    FromPassphrasePrompt,
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
    #[error("{0}")]
    MachineId(#[from] MachineIdInvalid),
    #[error("{0}")]
    HashedPassword(#[from] HashedPasswordInvalid),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MachineIdInvalid {
    #[error("expected 32 characters, found {found}")]
    WrongLength { found: usize },
    #[error("expected lowercase hexadecimal characters")]
    NotLowercaseHex,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HashedPasswordInvalid {
    #[error("not a crypt record: missing leading '$'")]
    MissingPrefix,
    #[error("not a crypt record: expected at least 3 sections, found {found}")]
    TooFewSections { found: usize },
    #[error("not a crypt record: section {index} is empty")]
    EmptySection { index: usize },
    #[error("value spans more than one line")]
    SpansMoreThanOneLine,
}

fn validate_machine_id(text: &str) -> Result<(), MachineIdInvalid> {
    let found = text.chars().count();
    if found != 32 {
        return Err(MachineIdInvalid::WrongLength { found });
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(MachineIdInvalid::NotLowercaseHex);
    }
    Ok(())
}

/// A crypt record is `$id$params$salt$hash`, with the section count varying
/// by scheme: yescrypt carries four, classic SHA-512 three, and `rounds=`
/// rides inside a section rather than adding one. So the shape is checked
/// and not the arity.
///
/// This is well-formedness, not authenticity; orma cannot tell a record that
/// opens a session from one that opens nothing. It catches what matters at
/// boot, where a truncated or half-written value locks the operator out of a
/// system that reports itself healthy.
fn validate_crypt_record(text: &str) -> Result<(), HashedPasswordInvalid> {
    // A record carrying a newline of its own would inject a line into
    // whatever line-oriented file it is copied into.
    if text.contains(['\n', '\r']) {
        return Err(HashedPasswordInvalid::SpansMoreThanOneLine);
    }
    if !text.starts_with('$') {
        return Err(HashedPasswordInvalid::MissingPrefix);
    }

    let mut sections = text.split('$');
    sections.next(); // the empty string ahead of the leading '$'

    let mut found = 0usize;
    for (position, section) in sections.enumerate() {
        if section.is_empty() {
            return Err(HashedPasswordInvalid::EmptySection {
                index: position + 1,
            });
        }
        found += 1;
    }
    if found < 3 {
        return Err(HashedPasswordInvalid::TooFewSections { found });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine_id(value: &[u8]) -> Result<(), Invalid> {
        FieldKind::MachineId.validate(value)
    }

    fn hashed_password(value: &[u8]) -> Result<(), Invalid> {
        FieldKind::HashedPassword.validate(value)
    }

    #[test]
    fn a_type_is_known_by_the_name_a_schema_uses() {
        for kind in [FieldKind::MachineId, FieldKind::HashedPassword] {
            assert_eq!(FieldKind::parse(kind.name()), Some(kind));
        }
        assert_eq!(FieldKind::parse("ssh-host-key"), None);
    }

    #[test]
    fn a_machine_id_is_made_out_of_sixteen_bytes_of_randomness() {
        let entropy = [
            0xd2, 0xc8, 0xe7, 0xe9, 0xa4, 0xb3, 0x4d, 0x62, //
            0xb8, 0xf8, 0xa0, 0xc5, 0xe9, 0xd7, 0xf3, 0xb1,
        ];
        let Some(Recipe::FromEntropy { bytes, build }) = FieldKind::MachineId.recipe() else {
            panic!("a machine-id is made out of randomness");
        };
        assert_eq!(bytes, entropy.len());

        let value = build(&entropy);
        assert_eq!(value, b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n");
        assert!(FieldKind::MachineId.validate(&value).is_ok());
    }

    #[test]
    fn a_hashed_password_is_made_from_a_passphrase() {
        assert!(matches!(
            FieldKind::HashedPassword.recipe(),
            Some(Recipe::FromPassphrasePrompt)
        ));
    }

    #[test]
    fn a_machine_id_is_32_lowercase_hex_characters() {
        assert!(machine_id(b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1").is_ok());
    }

    /// systemd writes the file with a trailing newline.
    #[test]
    fn a_trailing_newline_is_allowed() {
        assert!(machine_id(b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n").is_ok());
        assert!(hashed_password(b"$6$salt$hash\n").is_ok());
    }

    #[test]
    fn a_machine_id_of_the_wrong_length_is_refused() {
        assert_eq!(
            machine_id(b"abcd"),
            Err(MachineIdInvalid::WrongLength { found: 4 }.into())
        );
    }

    #[test]
    fn a_machine_id_outside_lowercase_hexadecimal_is_refused() {
        assert_eq!(
            machine_id(b"D2C8E7E9A4B34D62B8F8A0C5E9D7F3B1"),
            Err(MachineIdInvalid::NotLowercaseHex.into())
        );
        assert_eq!(
            machine_id(b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
            Err(MachineIdInvalid::NotLowercaseHex.into())
        );
    }

    #[test]
    fn a_crypt_record_is_accepted_whatever_its_scheme() {
        assert!(hashed_password(b"$y$j9T$saltSaltSalt$hashHashHash").is_ok());
        assert!(hashed_password(b"$6$rounds=5000$saltsalt$hashhash").is_ok());
        assert!(hashed_password(b"$6$salt$hash").is_ok());
    }

    #[test]
    fn something_that_is_not_a_crypt_record_is_refused() {
        assert_eq!(
            hashed_password(b"hunter2"),
            Err(HashedPasswordInvalid::MissingPrefix.into())
        );
        assert_eq!(
            hashed_password(b"$6$salt"),
            Err(HashedPasswordInvalid::TooFewSections { found: 2 }.into())
        );
        assert_eq!(
            hashed_password(b"$y$$j9T$hash"),
            Err(HashedPasswordInvalid::EmptySection { index: 2 }.into())
        );
    }

    #[test]
    fn a_crypt_record_spanning_lines_is_refused() {
        assert_eq!(
            hashed_password(b"$6$salt$hash\nroot::0:0:::"),
            Err(HashedPasswordInvalid::SpansMoreThanOneLine.into())
        );
    }

    #[test]
    fn a_value_that_is_not_text_is_refused() {
        assert_eq!(machine_id(&[0xff, 0xfe]), Err(Invalid::NotUtf8));
        assert_eq!(hashed_password(&[0xff, 0xfe]), Err(Invalid::NotUtf8));
    }
}
