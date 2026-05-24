mod hashed_password;
mod machine_id;

use crate::asker::Asker;
use crate::schema;
use hashed_password::HashedPassword;
use machine_id::MachineId;

/// A field's type, paired with everything needed to validate or generate.
#[derive(Debug)]
pub enum FieldType {
    HashedPassword(HashedPassword),
    MachineId(MachineId),
}

impl FieldType {
    pub fn validate(&self, raw: &[u8]) -> Result<(), &'static str> {
        match self {
            FieldType::HashedPassword(h) => h.validate(raw),
            FieldType::MachineId(m) => m.validate(raw),
        }
    }

    pub fn required_tools(&self) -> &'static [&'static str] {
        match self {
            FieldType::HashedPassword(h) => h.required_tools(),
            FieldType::MachineId(m) => m.required_tools(),
        }
    }

    pub fn generate(
        &self,
        field_path: &str,
        asker: &dyn Asker,
    ) -> Result<Vec<u8>, String> {
        match self {
            FieldType::HashedPassword(h) => h.generate(field_path, asker),
            FieldType::MachineId(m) => m.generate(field_path, asker),
        }
    }
}

pub fn parse(field: &schema::Field) -> Result<FieldType, String> {
    match field.r#type.as_str() {
        "hashed-password" => Ok(FieldType::HashedPassword(HashedPassword)),
        "machine-id" => Ok(FieldType::MachineId(MachineId)),
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
