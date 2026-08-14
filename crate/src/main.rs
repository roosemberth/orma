use std::path::{Path, PathBuf};
use std::process::ExitCode;

use argh::FromArgs;

mod core;
mod passphrase;
mod random;
mod schema_file;
mod volume;

use core::generate::{Generate, GenerateError};
use core::resolve::{Mode, Resolve, ResolveError, Step};

/// Loads a system's passwords and keys at boot from a separate volume.
#[derive(FromArgs)]
struct Cli {
    #[argh(subcommand)]
    cmd: SubCmd,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum SubCmd {
    Resolve(ResolveCmd),
    Generate(GenerateCmd),
}

/// Check the volume at <volume> against <schema>, provisioning the values
/// it declares at <output>. With --evaluate-only, <output> may be omitted.
#[derive(FromArgs)]
#[argh(subcommand, name = "resolve")]
struct ResolveCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,

    /// path where the unlocked volume is mounted
    #[argh(positional)]
    volume: PathBuf,

    /// path to the output directory (required unless --evaluate-only)
    #[argh(positional)]
    output: Option<PathBuf>,

    /// validate without writing
    #[argh(switch)]
    evaluate_only: bool,
}

/// Provision the identity volume at <volume> with the fields <schema>
/// declares.
#[derive(FromArgs)]
#[argh(subcommand, name = "generate")]
struct GenerateCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,

    /// path where the unlocked volume is mounted
    #[argh(positional)]
    volume: PathBuf,
}

fn main() -> ExitCode {
    let cli: Cli = argh::from_env();
    match cli.cmd {
        SubCmd::Resolve(cmd) => run_resolve(cmd),
        SubCmd::Generate(cmd) => run_generate(cmd),
    }
}

fn run_generate(cmd: GenerateCmd) -> ExitCode {
    let schema = match schema_file::read(&cmd.schema) {
        Ok(schema) => schema,
        Err(err) => {
            eprintln!("{err}");
            return match err {
                schema_file::Error::Schema(_) => ExitCode::from(3),
                _ => ExitCode::from(1),
            };
        }
    };

    if !cmd.volume.is_dir() {
        eprintln!("{}: not a directory", cmd.volume.display());
        return ExitCode::from(1);
    }

    match drive_generate(Generate::new(&schema), &cmd.volume) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            match err {
                GenerateError::AlreadyHeld(_) => ExitCode::from(2),
                GenerateError::Unable { .. } => ExitCode::from(3),
                _ => ExitCode::from(1),
            }
        }
    }
}

fn drive_generate(mut generate: Generate, volume: &Path) -> Result<(), GenerateError> {
    use core::generate::Step;
    loop {
        match generate.step() {
            Step::CheckValue(request) => match volume::read(volume, request.path()) {
                Ok(value) => request.present(&value),
                Err(volume::ReadError::Absent) => request.absent(),
                Err(volume::ReadError::Unreadable(err)) => request.failed(err.to_string()),
            },
            Step::DrawEntropy(request) => match random::draw(request.wanted()) {
                Ok(entropy) => request.filled(&entropy),
                Err(err) => request.failed(err.to_string()),
            },
            Step::HashPassphrase(request) => {
                let hashed = passphrase::prompt_passphrase_and_hash(request.path().as_str());
                match hashed {
                    Ok(record) => request.hashed(&record),
                    Err(err) => request.failed(err.to_string()),
                }
            }
            Step::WriteValue(request) => {
                let written = volume::write(volume, request.path(), request.value());
                match written {
                    Ok(()) => request.written(),
                    Err(err) => request.failed(err.to_string()),
                }
            }
            Step::Done(outcome) => return outcome,
        }
    }
}

fn run_resolve(cmd: ResolveCmd) -> ExitCode {
    let output = match (cmd.evaluate_only, cmd.output) {
        (true, _) => None,
        (false, Some(path)) => Some(path),
        (false, None) => {
            eprintln!("resolve without --evaluate-only requires an output path");
            return ExitCode::from(1);
        }
    };

    let schema = match schema_file::read(&cmd.schema) {
        Ok(schema) => schema,
        Err(err) => {
            eprintln!("{err}");
            return match err {
                schema_file::Error::Schema(_) => ExitCode::from(3),
                _ => ExitCode::from(1),
            };
        }
    };

    if !cmd.volume.is_dir() {
        eprintln!("{}: not a directory", cmd.volume.display());
        return ExitCode::from(1);
    }

    let mode = match &output {
        Some(output) if !output.is_dir() => {
            eprintln!("{}: not a directory", output.display());
            return ExitCode::from(1);
        }
        Some(_) => Mode::Write,
        None => Mode::EvaluateOnly,
    };

    let resolve = Resolve::new(&schema, mode);
    match drive_resolve(resolve, &cmd.volume, output.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            match err {
                ResolveError::Unsatisfied(_) => ExitCode::from(2),
                ResolveError::WriteFailed { .. } => ExitCode::from(1),
            }
        }
    }
}

fn drive_resolve(
    mut resolve: Resolve,
    volume: &Path,
    output: Option<&Path>,
) -> Result<(), ResolveError> {
    loop {
        match resolve.step() {
            Step::ReadValue(request) => match volume::read(volume, request.path()) {
                Ok(value) => request.found(&value),
                Err(volume::ReadError::Absent) => request.absent(),
                Err(volume::ReadError::Unreadable(err)) => request.unreadable(err.to_string()),
            },
            Step::WriteValue(request) => match output {
                Some(output) => {
                    let provisioned = volume::write(output, request.path(), request.value());
                    match provisioned {
                        Ok(()) => request.written(),
                        Err(err) => request.failed(err.to_string()),
                    }
                }
                None => request.failed("no output path was given".to_owned()),
            },
            Step::Done(verdict) => return verdict,
        }
    }
}
