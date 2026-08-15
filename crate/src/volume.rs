use std::fs::{DirBuilder, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
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
    read_value(&locate(volume, field)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => ReadError::Absent,
        _ => ReadError::Unreadable(err),
    })
}

/// Read a value stored at the path.
///
/// Symlinks are not followed. This also prevents pipes hanging.
fn read_value(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut stored = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|err| match err.raw_os_error() {
            Some(libc::ELOOP) => misplaced(path, "a symlink", "a file"),
            _ => err,
        })?;
    let found = stored.metadata()?;
    if !found.is_file() {
        return Err(misplaced(path, describe(&found), "a file"));
    }
    let mut value = Vec::new();
    stored.read_to_end(&mut value)?;
    Ok(value)
}

/// Write the value for the specified field at the output path, with the
/// permissions specified by the field.
pub fn write(
    root: &Path,
    field: &FieldPath,
    value: &[u8],
    permissions: u32,
) -> Result<(), WriteError> {
    provision_file(root, field, value, permissions).map_err(WriteError)
}

fn provision_file(
    root: &Path,
    field: &FieldPath,
    value: &[u8],
    permissions: u32,
) -> std::io::Result<()> {
    let dest = locate(root, field);
    let mut at = root.to_path_buf();
    for component in field.components() {
        at.push(component);
        if at == dest {
            break;
        }
        make_way(&at)?;
    }
    write_value(&dest, value, permissions)
}

/// Ensure the directory where a file should be written exists as we expect it.
///
/// Symlinks are rejected.
fn make_way(at: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(at) {
        Ok(found) if found.is_dir() => Ok(()),
        Ok(found) => Err(misplaced(at, describe(&found), "a directory")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            DirBuilder::new().mode(0o700).create(at)
        }
        Err(err) => Err(err),
    }
}

/// Ensure the file is written at the specified destination as we expect it.
///
/// If a file is in the way, it is removed and replaced, else we reject.
/// The file is created with the specified permissions.
fn write_value(dest: &Path, value: &[u8], permissions: u32) -> std::io::Result<()> {
    match std::fs::symlink_metadata(dest) {
        Ok(found) if found.is_file() => std::fs::remove_file(dest)?,
        Ok(found) => return Err(misplaced(dest, describe(&found), "a file")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(permissions)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dest)?
        .write_all(value)
}

/// Say something stands at the path we expected something else.
fn misplaced(path: &Path, found: &str, expected: &str) -> std::io::Error {
    std::io::Error::other(format!(
        "{}: expected {expected}, found {found}",
        path.display()
    ))
}

fn describe(found: &Metadata) -> &'static str {
    let kind = found.file_type();
    match (kind.is_symlink(), kind.is_dir(), kind.is_file()) {
        (true, _, _) => "a symlink",
        (_, true, _) => "a directory",
        (_, _, true) => "a file",
        _ => "something else",
    }
}

fn locate(volume: &Path, field: &FieldPath) -> PathBuf {
    let mut located = volume.to_path_buf();
    // A field path carries no `.` or `..`, so descending it from the volume
    // root cannot leave it for as long as no symlinks are followed.
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
