use std::path::{Path, PathBuf};
use std::process::ExitCode;

use argh::FromArgs;

mod asker;
mod core;
mod passphrase;
mod random;
mod schema_file;
mod tool;
mod volume;

use asker::{AskVia, Asker};
use core::generate::Mode as GenerateMode;
use core::generate::{CheckValue, DrawEntropy, Generate, GenerateError};
use core::resolve::{Mode, Resolve, ResolveError, Step};

/// Loads a machine's identity at boot from a volume kept apart from the system
/// image: its machine-id, host keys, and whatever else distinguishes a machine
/// from another built the same way. Run `orma <command> --help` for details.
#[derive(FromArgs)]
#[argh(
    error_code(
        1,
        "Invalid argument or system error: a malformed command line,\n    \
         an unreadable schema, a path that is not a directory, a tool\n    \
         that is missing, a write that failed"
    ),
    error_code(
        2,
        "the volume is not as the operation requires: it fails its \
         schema,\n    or it already holds values generate would have produced"
    ),
    error_code(
        3,
        "orma cannot act on the schema: a version or a field type this\n    \
         build does not implement"
    )
)]
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

/// Verify the identity volume at <volume> against <schema> and write the
/// values under <output>.
/// Values are validated and nothing is written unless they are all valid.
#[derive(FromArgs)]
#[argh(
    subcommand,
    name = "resolve",
    example = "Provision at boot, from the initrd:\n  {command_name} \
               /etc/orma/schema.yaml /var/lib/identity /sysroot/var/lib/orma",
    example = "Ask whether the next image's schema would boot, before \
               updating:\n  {command_name} /new/schema.yaml /var/lib/identity \
               --evaluate-only"
)]
struct ResolveCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,

    /// directory where the unlocked identity volume is mounted
    #[argh(positional)]
    volume: PathBuf,

    /// directory to provision into (required unless --evaluate-only)
    #[argh(positional)]
    output: Option<PathBuf>,

    /// verify and report through the exit status, writing nothing
    #[argh(switch)]
    evaluate_only: bool,
}

/// Provision the identity volume at <volume> with the fields <schema> declares.
/// With --upgrade, generate any missing values in the identity volume.
#[derive(FromArgs)]
#[argh(
    subcommand,
    name = "generate",
    example = "Populate a new volume:\n  {command_name} /etc/orma/schema.yaml \
               /var/lib/identity",
    example = "Generate any missing values, from an emergency shell:\n  \
               {command_name} /etc/orma/schema.yaml /var/lib/identity \
               --upgrade",
    example = "Check this system could provision, without any writing:\n  \
               {command_name} /etc/orma/schema.yaml /var/lib/identity \
               --dry-run",
    example = "Provision unattended, from a pipeline:\n  yes hunter2 | \
               {command_name} /etc/orma/schema.yaml /var/lib/identity \
               --ask-via stdin"
)]
struct GenerateCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,

    /// directory where the unlocked identity volume is mounted
    #[argh(positional)]
    volume: PathBuf,

    /// rehearse and stop
    #[argh(switch)]
    dry_run: bool,

    /// how to reach the operator: tty (default), systemd-ask-password, or
    /// stdin to take one piped line per question
    #[argh(option, default = "AskVia::Tty")]
    ask_via: AskVia,

    /// produce only the values the volume lacks
    #[argh(switch)]
    upgrade: bool,
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

    let mode = match cmd.upgrade {
        true => GenerateMode::Upgrade,
        false => GenerateMode::Populate,
    };

    let mut asker = Asker::new(cmd.ask_via);
    // Always dry-run before performing.
    if let Err(err) = rehearse_generate(Generate::new(&schema, mode), &cmd.volume, &mut asker) {
        eprintln!("{err}");
        return generate_exit(&err);
    }
    if cmd.dry_run {
        return ExitCode::SUCCESS;
    }

    match drive_generate(Generate::new(&schema, mode), &cmd.volume, &mut asker) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            generate_exit(&err)
        }
    }
}

fn generate_exit(err: &GenerateError) -> ExitCode {
    match err {
        GenerateError::AlreadyHeld(_) | GenerateError::InvalidValues(_) => ExitCode::from(2),
        GenerateError::Unable { .. } => ExitCode::from(3),
        _ => ExitCode::from(1),
    }
}

/// Drive generate without actually writing anything. Most steps are innocuously
/// exercised so we can check if they are likely to run in the real drive.
fn rehearse_generate(
    mut generate: Generate,
    volume: &Path,
    asker: &mut Asker,
) -> Result<(), GenerateError> {
    use core::generate::Step;
    loop {
        match generate.step() {
            Step::CheckValue(request) => answer_check(volume, request),
            Step::DrawEntropy(request) => answer_draw(request),
            Step::HashPassphrase(request) => match passphrase::rehearse_and_hash(asker) {
                Ok(record) => request.hashed(&record),
                Err(err) => request.failed(err.to_string()),
            },
            Step::WriteValue(request) => request.written(),
            Step::Done(outcome) => return outcome,
        }
    }
}

fn answer_check(volume: &Path, request: CheckValue) {
    match volume::read(volume, request.path()) {
        Ok(value) => request.present(&value),
        Err(volume::ReadError::Absent) => request.absent(),
        Err(volume::ReadError::Unreadable(err)) => request.failed(err.to_string()),
    }
}

fn answer_draw(request: DrawEntropy) {
    match random::draw(request.wanted()) {
        Ok(entropy) => request.filled(&entropy),
        Err(err) => request.failed(err.to_string()),
    }
}

fn drive_generate(
    mut generate: Generate,
    volume: &Path,
    asker: &mut Asker,
) -> Result<(), GenerateError> {
    use core::generate::Step;
    loop {
        match generate.step() {
            Step::CheckValue(request) => answer_check(volume, request),
            Step::DrawEntropy(request) => answer_draw(request),
            Step::HashPassphrase(request) => {
                let hashed = passphrase::prompt_and_hash(
                    asker,
                    request.path().as_str(),
                    request.description(),
                );
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
