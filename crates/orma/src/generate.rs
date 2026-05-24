use std::path::{Path, PathBuf};

use crate::asker::Asker;
use crate::field_types::{self, FieldType};
use crate::schema::{self, Field};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Schema(#[from] schema::Error),

    #[error("not a directory: {}", .0.display())]
    NotADirectory(PathBuf),

    #[error("{0}")]
    UnknownType(String),

    #[error("missing tools on PATH: {}", .0.join(", "))]
    MissingTools(Vec<String>),

    #[error("would overwrite existing values:\n{}", .0.join("\n"))]
    WouldOverwrite(Vec<String>),

    #[error("{path}: {reason}")]
    Generate { path: String, reason: String },

    #[error("write failed: {0}")]
    Write(#[from] std::io::Error),
}

/// Provision <volume> by generating a value for every field declared
/// in <schema>. Without force, refuses to overwrite any value already
/// present in the volume.
pub fn run(
    schema_path: &Path,
    volume: &Path,
    force: bool,
    asker: &dyn Asker,
) -> Result<(), Error> {
    let schema = schema::parse(schema_path)?;
    require_directory(volume)?;

    let parsed: Vec<FieldType> = schema
        .fields
        .iter()
        .map(|f| field_types::parse(f).map_err(Error::UnknownType))
        .collect::<Result<_, _>>()?;

    preflight(&parsed, asker)?;
    if !force {
        refuse_overwrites(volume, &schema.fields)?;
    }

    let values: Vec<Vec<u8>> = schema
        .fields
        .iter()
        .zip(&parsed)
        .map(|(f, ft)| {
            ft.generate(&f.path, asker)
                .map_err(|reason| Error::Generate {
                    path: f.path.clone(),
                    reason,
                })
        })
        .collect::<Result<_, _>>()?;

    for (f, v) in schema.fields.iter().zip(&values) {
        write_value(volume, &f.path, v)?;
    }
    Ok(())
}

fn require_directory(p: &Path) -> Result<(), Error> {
    if p.is_dir() {
        Ok(())
    } else {
        Err(Error::NotADirectory(p.to_path_buf()))
    }
}

fn preflight(types: &[FieldType], asker: &dyn Asker) -> Result<(), Error> {
    let mut missing: Vec<String> = Vec::new();
    let mut note_missing = |tool: &str| {
        if !in_path(tool) && !missing.iter().any(|t| t == tool) {
            missing.push(tool.to_string());
        }
    };
    for ft in types {
        for tool in ft.required_tools() {
            note_missing(tool);
        }
    }
    for tool in asker.required_tools() {
        note_missing(tool);
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::MissingTools(missing))
    }
}

fn in_path(tool: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in path.split(':') {
        if Path::new(dir).join(tool).is_file() {
            return true;
        }
    }
    false
}

fn refuse_overwrites(volume: &Path, fields: &[Field]) -> Result<(), Error> {
    let existing: Vec<String> = fields
        .iter()
        .filter(|f| volume.join(f.path.trim_start_matches('/')).exists())
        .map(|f| f.path.clone())
        .collect();
    if existing.is_empty() {
        Ok(())
    } else {
        Err(Error::WouldOverwrite(existing))
    }
}

fn write_value(
    volume: &Path,
    schema_path: &str,
    value: &[u8],
) -> std::io::Result<()> {
    let dest = volume.join(schema_path.trim_start_matches('/'));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, value)
}
