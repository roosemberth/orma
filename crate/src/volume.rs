use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::core::schema::FieldPath;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("nothing is stored at this path")]
    Absent,
    #[error("{0}")]
    Unreadable(std::io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct WriteError(std::io::Error);

pub fn read(volume: &Path, field: &FieldPath) -> Result<Vec<u8>, ReadError> {
    std::fs::read(locate(volume, field)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => ReadError::Absent,
        _ => ReadError::Unreadable(err),
    })
}

pub fn has_field_value(root: &Path, field: &FieldPath) -> std::io::Result<bool> {
    std::fs::exists(locate(root, field))
}

/// Write the value for the specified field at the output path.
/// The value is only accessible by the owner.
pub fn write(root: &Path, field: &FieldPath, value: &[u8]) -> Result<(), WriteError> {
    provision_file(root, field, value).map_err(WriteError)
}

fn provision_file(root: &Path, field: &FieldPath, value: &[u8]) -> std::io::Result<()> {
    let dest = locate(root, field);
    let mut at = root.to_path_buf();
    for component in field.components() {
        at.push(component);
        if at != dest && !at.exists() {
            std::fs::create_dir(&at)?;
            std::fs::set_permissions(&at, Permissions::from_mode(0o700))?;
        }
    }
    std::fs::write(&dest, value)?;
    std::fs::set_permissions(&dest, Permissions::from_mode(0o600))
}

fn locate(volume: &Path, field: &FieldPath) -> PathBuf {
    let mut located = volume.to_path_buf();
    // A field path carries no `.` or `..`, so descending it from the volume
    // root cannot leave it.
    located.extend(field.components());
    located
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_is_looked_for_under_the_mount_point() {
        let field = FieldPath::parse("/ssh/host_key").unwrap();
        assert_eq!(
            locate(Path::new("/run/identity"), &field),
            PathBuf::from("/run/identity/ssh/host_key")
        );
    }
}
