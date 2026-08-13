use std::path::PathBuf;
use std::process::ExitCode;

use argh::FromArgs;

mod core;

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

    // FIXME: consumed when resolve learns to read its schema and its volume.
    let _ = (&cmd.schema, &cmd.volume, &output);

    match core::resolve::resolve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(3)
        }
    }
}
