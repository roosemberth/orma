use serde::Deserialize;
use serde::de::{self, Deserializer};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Schema {
    pub version: Version,
    pub fields: Vec<Field>,
}

#[derive(Debug, Deserialize)]
pub struct Field {
    pub path: String,
    pub r#type: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Version {
    V1,
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = u32::deserialize(d)?;
        match n {
            1 => Ok(Version::V1),
            other => Err(de::Error::custom(format!(
                "unsupported schema version: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V1 => write!(f, "1"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("read failed: {0}")]
    Read(#[from] std::io::Error),

    #[error("parse failed: {0}")]
    Parse(#[from] serde_yaml::Error),
}

pub fn parse(path: &Path) -> Result<Schema, Error> {
    let raw = std::fs::read_to_string(path)?;
    let schema = serde_yaml::from_str(&raw)?;
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_field() {
        let yaml = "
version: 1
fields:
  - path: /passwd.hash
    type: hashed-password
";
        let s: Schema = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(s.version, Version::V1);
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].path, "/passwd.hash");
        assert_eq!(s.fields[0].r#type, "hashed-password");
        assert!(!s.fields[0].optional);
    }

    #[test]
    fn optional_defaults_to_false() {
        let yaml = "
version: 1
fields:
  - path: /a
    type: x
  - path: /b
    type: x
    optional: true
";
        let s: Schema = serde_yaml::from_str(yaml).unwrap();
        assert!(!s.fields[0].optional);
        assert!(s.fields[1].optional);
    }

    #[test]
    fn version_is_required() {
        let yaml = "
fields:
  - path: /x
    type: y
";
        let r: Result<Schema, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err());
    }

    #[test]
    fn unsupported_version_fails() {
        let yaml = "
version: 2
fields: []
";
        let r: Result<Schema, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err());
    }
}
