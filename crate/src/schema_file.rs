//! Read and parse the YAML file containing the schema off the filesystem.

use std::path::Path;

use crate::core::schema::{Schema, SchemaError, file};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Malformed {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("{0}")]
    Schema(#[from] SchemaError),
}

pub fn read(path: &Path) -> Result<Schema, Error> {
    let bytes = std::fs::read(path).map_err(|source| Error::Unreadable {
        path: path.display().to_string(),
        source,
    })?;
    decode(path, &bytes)
}

fn decode(path: &Path, bytes: &[u8]) -> Result<Schema, Error> {
    let declared: file::Schema =
        serde_yaml::from_slice(bytes).map_err(|source| Error::Malformed {
            path: path.display().to_string(),
            source,
        })?;
    Ok(Schema::new(declared)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a fixture keeping its name for the diagnostics.
    macro_rules! fixture {
        ($name:literal) => {
            decode(
                Path::new($name),
                include_str!(concat!("../fixtures/", $name)).as_bytes(),
            )
        };
    }

    fn decode_str(yaml: &str) -> Result<Schema, Error> {
        decode(Path::new("schema.yaml"), yaml.as_bytes())
    }

    #[test]
    fn reads_the_declared_fields() {
        let schema = fixture!("schema-example.yaml").unwrap();
        let seen: Vec<&str> = schema.fields().iter().map(|f| f.path().as_str()).collect();
        assert_eq!(seen, vec!["/machine-id", "/user.passwd", "/sudo.passwd"]);
    }

    #[test]
    fn version_is_required() {
        assert!(matches!(
            decode_str("fields: []"),
            Err(Error::Malformed { .. })
        ));
    }

    #[test]
    fn unknown_keys_are_refused() {
        let yaml = "
version: 1
fields:
  - path: /machine-id
    type: machine-id
    mode: \"0600\"
";
        assert!(matches!(decode_str(yaml), Err(Error::Malformed { .. })));
    }

    #[test]
    fn the_error_names_the_file() {
        let err = decode_str("version: [}").unwrap_err();
        assert!(format!("{err}").starts_with("schema.yaml:"));
    }

    #[test]
    fn a_schema_the_core_refuses_is_refused() {
        assert!(matches!(
            fixture!("schema-unknown-version.yaml"),
            Err(Error::Schema(_))
        ));
    }
}
