use std::path::{Path, PathBuf};
use std::process::ExitCode;

use argh::FromArgs;

mod core;
mod schema_file;
mod volume;

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
}

/// Check the volume at <volume> against <schema>, laying the values it
/// declares at <output>. With --evaluate-only, <output> may be omitted.
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

fn main() -> ExitCode {
    let cli: Cli = argh::from_env();
    match cli.cmd {
        SubCmd::Resolve(cmd) => run_resolve(cmd),
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

    let mode = match output {
        Some(_) => Mode::Write,
        None => Mode::EvaluateOnly,
    };

    match drive_resolve(Resolve::new(&schema, mode), &cmd.volume) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            match err {
                ResolveError::Unsatisfied(_) => ExitCode::from(2),
                ResolveError::UnimplementedWrite => ExitCode::from(3),
            }
        }
    }
}

fn drive_resolve(mut resolve: Resolve, volume: &Path) -> Result<(), ResolveError> {
    loop {
        match resolve.step() {
            Step::ReadValue(request) => match volume::read(volume, request.path()) {
                Ok(value) => request.found(&value),
                Err(volume::ReadError::Absent) => request.absent(),
                Err(volume::ReadError::Unreadable(err)) => request.unreadable(err.to_string()),
            },
            Step::Done(verdict) => return verdict,
        }
    }
}
