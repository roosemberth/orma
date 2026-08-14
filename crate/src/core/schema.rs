/// The file format. This is different from the schema in that it is used to
/// parse the file contents, before any policy or validation has been applied.
pub mod file {
    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Schema {
        pub version: u64,
        pub fields: Vec<Field>,
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Field {
        pub path: String,
        #[serde(rename = "type")]
        pub type_name: String,
    }

    #[cfg(test)]
    pub(crate) mod fixtures {
        use super::{Field, Schema};

        pub fn field(path: &str, type_name: &str) -> Field {
            Field {
                path: path.to_owned(),
                type_name: type_name.to_owned(),
            }
        }

        pub fn schema(fields: Vec<Field>) -> Schema {
            Schema { version: 1, fields }
        }
    }
}

/// A schema whose every field is one orma can act on.
#[derive(Debug)]
pub struct Schema {
    fields: Vec<Field>,
}

impl Schema {
    pub fn new(file: file::Schema) -> Result<Schema, SchemaError> {
        if file.version != 1 {
            return Err(SchemaError::UnsupportedVersion(file.version));
        }

        let mut fields: Vec<Field> = Vec::with_capacity(file.fields.len());
        for field in file.fields {
            let path = FieldPath::parse(&field.path).map_err(|source| SchemaError::Path {
                path: field.path.clone(),
                source,
            })?;
            if fields.iter().any(|f| f.path == path) {
                return Err(SchemaError::DuplicatePath(path.as_str().to_owned()));
            }
            fields.push(Field {
                path,
                type_name: field.type_name,
            });
        }

        Ok(Schema { fields })
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
}

#[derive(Debug)]
pub struct Field {
    path: FieldPath,
    type_name: String,
}

impl Field {
    pub fn path(&self) -> &FieldPath {
        &self.path
    }

    pub fn type_name(&self) -> &str {
        &self.type_name
    }
}

/// Path to a file containng the field value inside the volume.
///
/// Paths outside the volume (e.g. `.`, `..`, ...) are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPath(String);

impl FieldPath {
    pub fn parse(value: &str) -> Result<FieldPath, PathError> {
        if value.is_empty() {
            return Err(PathError::Empty);
        }
        if !value.starts_with('/') {
            return Err(PathError::NotAbsolute);
        }
        for component in value.split('/').skip(1) {
            if component.is_empty() {
                return Err(PathError::EmptyComponent);
            }
            if component == "." || component == ".." {
                return Err(PathError::RelativeComponent);
            }
            if component.contains('\0') {
                return Err(PathError::Nul);
            }
        }
        Ok(FieldPath(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FieldPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("unsupported schema version: {0}")]
    UnsupportedVersion(u64),

    #[error("field '{path}': {source}")]
    Path {
        path: String,
        #[source]
        source: PathError,
    },

    #[error("duplicate field path: {0}")]
    DuplicatePath(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,

    #[error("path must start with '/'")]
    NotAbsolute,

    #[error("path contains an empty component")]
    EmptyComponent,

    #[error("path contains a '.' or '..' component")]
    RelativeComponent,

    #[error("path contains a NUL byte")]
    Nul,
}

#[cfg(test)]
mod tests {
    use super::file::fixtures;
    use super::*;

    fn schema(fields: Vec<file::Field>) -> Result<Schema, SchemaError> {
        Schema::new(fixtures::schema(fields))
    }

    #[test]
    fn a_schema_may_declare_nothing() {
        assert!(schema(vec![]).unwrap().fields().is_empty());
    }

    #[test]
    fn unsupported_version_is_refused() {
        assert!(matches!(
            Schema::new(file::Schema {
                version: 2,
                fields: vec![]
            }),
            Err(SchemaError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn duplicate_paths_are_refused() {
        let declared = vec![
            fixtures::field("/a", "machine-id"),
            fixtures::field("/a", "machine-id"),
        ];
        assert!(matches!(
            schema(declared),
            Err(SchemaError::DuplicatePath(p)) if p == "/a"
        ));
    }

    #[test]
    fn a_traversing_path_refuses_the_schema() {
        let declared = vec![fixtures::field("/../../etc/shadow", "machine-id")];
        assert!(matches!(
            schema(declared),
            Err(SchemaError::Path {
                source: PathError::RelativeComponent,
                ..
            })
        ));
    }

    #[test]
    fn paths_that_would_leave_the_volume_are_refused() {
        let cases = [
            ("", PathError::Empty),
            ("user.passwd", PathError::NotAbsolute),
            ("./user.passwd", PathError::NotAbsolute),
            ("../user.passwd", PathError::NotAbsolute),
            ("/", PathError::EmptyComponent),
            ("//user.passwd", PathError::EmptyComponent),
            ("/a//b", PathError::EmptyComponent),
            ("/a/", PathError::EmptyComponent),
            ("/..", PathError::RelativeComponent),
            ("/a/../../etc", PathError::RelativeComponent),
            ("/a/./b", PathError::RelativeComponent),
            ("/a\0b", PathError::Nul),
        ];
        for (declared, expected) in cases {
            assert_eq!(
                FieldPath::parse(declared),
                Err(expected),
                "path {declared:?}"
            );
        }
    }

    #[test]
    fn a_dot_inside_a_component_is_not_a_traversal() {
        assert!(FieldPath::parse("/user.passwd").is_ok());
        assert!(FieldPath::parse("/..hidden").is_ok());
        assert!(FieldPath::parse("/ssh/host_ed25519_key").is_ok());
    }
}
