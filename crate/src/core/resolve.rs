//! Checking a volume against the schema a system expects of it.

use crate::core::schema::Schema;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("{path}: field type '{type_name}' is not implemented")]
    UnimplementedFieldType { path: String, type_name: String },
}

/// Decide whether a volume holds what a schema declares.
pub fn resolve(schema: &Schema) -> Result<(), ResolveError> {
    match schema.fields().first() {
        None => Ok(()),
        Some(field) => Err(ResolveError::UnimplementedFieldType {
            path: field.path().as_str().to_owned(),
            type_name: field.type_name().to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::file;
    use crate::core::schema::file::fixtures;

    fn schema(fields: Vec<file::Field>) -> Schema {
        Schema::new(fixtures::schema(fields)).unwrap()
    }

    #[test]
    fn a_schema_declaring_nothing_is_satisfied() {
        assert!(resolve(&schema(vec![])).is_ok());
    }

    #[test]
    fn a_declared_field_cannot_be_evaluated_yet() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);

        assert!(matches!(
            resolve(&schema).unwrap_err(),
            ResolveError::UnimplementedFieldType { ref path, ref type_name }
                if path == "/machine-id" && type_name == "machine-id"
        ));
    }
}
