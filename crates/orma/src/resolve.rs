use std::path::{Path, PathBuf};

use crate::field_types;
use crate::schema::{self, Field, Schema, Version};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Schema(#[from] schema::Error),

    #[error("not a directory: {}", .0.display())]
    NotADirectory(PathBuf),

    #[error("{}\nvalidation failed", .0.join("\n"))]
    Evaluation(Vec<String>),

    #[error("write failed: {0}")]
    Write(#[from] std::io::Error),
}

enum ReadOutcome {
    Present(Vec<u8>),
    Missing,
    IoError(std::io::Error),
}

/// Validate the contents of <volume> against <schema>.
/// With Some(output), write the validated values;
/// with None, evaluate without writing.
/// Nothing is written if any field fails.
pub fn run(
    schema_path: &Path,
    volume: &Path,
    output: Option<&Path>,
) -> Result<(), Error> {
    let schema = schema::parse(schema_path)?;
    require_directory(volume)?;
    match schema.version {
        Version::V1 => run_v1(&schema, volume, output),
    }
}

fn run_v1(
    schema: &Schema,
    volume: &Path,
    output: Option<&Path>,
) -> Result<(), Error> {
    let reads = read_all(volume, &schema.fields);
    evaluate_all(&schema.fields, &reads).map_err(Error::Evaluation)?;
    if let Some(output) = output {
        act(output, &schema.fields, &reads)?;
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

fn read_all(volume: &Path, fields: &[Field]) -> Vec<ReadOutcome> {
    fields.iter().map(|f| read_one(volume, &f.path)).collect()
}

fn read_one(volume: &Path, schema_path: &str) -> ReadOutcome {
    let abs = volume.join(schema_path.trim_start_matches('/'));
    match std::fs::read(&abs) {
        Ok(b) => ReadOutcome::Present(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ReadOutcome::Missing
        }
        Err(e) => ReadOutcome::IoError(e),
    }
}

fn evaluate_all(
    fields: &[Field],
    reads: &[ReadOutcome],
) -> Result<(), Vec<String>> {
    let errors: Vec<String> = fields
        .iter()
        .zip(reads)
        .filter_map(|(f, r)| evaluate_one(f, r).err())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn evaluate_one(field: &Field, read: &ReadOutcome) -> Result<(), String> {
    match read {
        ReadOutcome::Missing if !field.optional => {
            Err(format!("{}: required but missing", field.path))
        }
        ReadOutcome::Missing => Ok(()),
        ReadOutcome::IoError(e) => {
            Err(format!("{}: read failed: {}", field.path, e))
        }
        ReadOutcome::Present(bytes) => {
            let ft = field_types::parse(field)
                .map_err(|reason| format!("{}: {}", field.path, reason))?;
            ft.validate(bytes)
                .map_err(|reason| format!("{}: {}", field.path, reason))
        }
    }
}

fn act(
    output: &Path,
    fields: &[Field],
    reads: &[ReadOutcome],
) -> std::io::Result<()> {
    std::fs::create_dir_all(output)?;
    for (f, r) in fields.iter().zip(reads) {
        if let ReadOutcome::Present(bytes) = r {
            let dest = output.join(f.path.trim_start_matches('/'));
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, bytes)?;
        }
    }
    Ok(())
}
