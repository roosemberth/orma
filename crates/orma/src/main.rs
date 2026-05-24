use argh::FromArgs;
use std::path::PathBuf;
use std::process::ExitCode;

mod field_types;
mod generate;
mod resolve;
mod schema;

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

/// Resolve <schema> against the unlocked volume at <volume>, writing
/// values to <output>. With --evaluate-only, <output> may be omitted.
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

/// Provision the volume at <volume> by generating a all fields in <schema>.
#[derive(FromArgs)]
#[argh(subcommand, name = "generate")]
struct GenerateCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,

    /// path where the unlocked volume is mounted
    #[argh(positional)]
    volume: PathBuf,

    /// overwrite values already present in the volume
    #[argh(switch)]
    force: bool,
}

fn main() -> ExitCode {
    let cli: Cli = argh::from_env();
    let result = match cli.cmd {
        SubCmd::Resolve(c) => run_resolve(c),
        SubCmd::Generate(c) => run_generate(c),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            for line in format!("{e}").lines() {
                eprintln!("{line}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_resolve(c: ResolveCmd) -> Result<(), Box<dyn std::error::Error>> {
    let output = match (c.evaluate_only, c.output) {
        (true, _) => None,
        (false, Some(p)) => Some(p),
        (false, None) => {
            return Err(
                "resolve without --evaluate-only requires an output path"
                    .into(),
            );
        }
    };
    resolve::run(&c.schema, &c.volume, output.as_deref())?;
    Ok(())
}

fn run_generate(c: GenerateCmd) -> Result<(), Box<dyn std::error::Error>> {
    generate::run(&c.schema, &c.volume, c.force)?;
    Ok(())
}
