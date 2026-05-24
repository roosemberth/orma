mod hashed_password;

use crate::schema;
use hashed_password::HashedPassword;

/// A field's type, paired with everything needed to validate or generate.
#[derive(Debug)]
pub enum FieldType {
    HashedPassword(HashedPassword),
}

impl FieldType {
    pub fn validate(&self, raw: &[u8]) -> Result<(), &'static str> {
        match self {
            FieldType::HashedPassword(h) => h.validate(raw),
        }
    }

    pub fn required_tools(&self) -> &'static [&'static str] {
        match self {
            FieldType::HashedPassword(h) => h.required_tools(),
        }
    }

    pub fn generate(&self, field_path: &str) -> Result<Vec<u8>, String> {
        match self {
            FieldType::HashedPassword(h) => h.generate(field_path),
        }
    }
}

pub fn parse(field: &schema::Field) -> Result<FieldType, String> {
    match field.r#type.as_str() {
        "hashed-password" => Ok(FieldType::HashedPassword(HashedPassword)),
        other => Err(format!("unknown field type '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: &str) -> schema::Field {
        schema::Field {
            path: "/x".into(),
            r#type: kind.into(),
            optional: false,
        }
    }

    #[test]
    fn parse_known_type() {
        let f = field("hashed-password");
        assert!(matches!(parse(&f), Ok(FieldType::HashedPassword(_))));
    }

    #[test]
    fn parse_unknown_type() {
        let f = field("ssh-host-key");
        let err = parse(&f).unwrap_err();
        assert!(err.contains("ssh-host-key"));
    }
}
