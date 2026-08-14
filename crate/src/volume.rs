use std::path::{Path, PathBuf};

use crate::core::schema::FieldPath;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("nothing is stored at this path")]
    Absent,
    #[error("{0}")]
    Unreadable(std::io::Error),
}

pub fn read(volume: &Path, field: &FieldPath) -> Result<Vec<u8>, ReadError> {
    std::fs::read(locate(volume, field)).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => ReadError::Absent,
        _ => ReadError::Unreadable(err),
    })
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
