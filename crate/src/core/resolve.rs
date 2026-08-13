//! Checking a volume against the schema a system expects of it.

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("resolve is not implemented in this build")]
    Unimplemented,
}

/// Decide whether a volume holds what a schema declares.
pub fn resolve() -> Result<(), ResolveError> {
    Err(ResolveError::Unimplemented)
}
