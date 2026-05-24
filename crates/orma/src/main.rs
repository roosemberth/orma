use argh::FromArgs;
use std::path::PathBuf;
use std::process::ExitCode;

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
}

/// Resolve a schema.
#[derive(FromArgs)]
#[argh(subcommand, name = "resolve")]
struct ResolveCmd {
    /// path to the schema YAML
    #[argh(positional)]
    schema: PathBuf,
}

fn main() -> ExitCode {
    let cli: Cli = argh::from_env();
    let result = match cli.cmd {
        SubCmd::Resolve(c) => resolve(c),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("orma: {e}");
            ExitCode::FAILURE
        }
    }
}

fn resolve(args: ResolveCmd) -> Result<(), Box<dyn std::error::Error>> {
    let s = schema::parse(&args.schema)?;
    println!("parsed schema v{}, {} field(s)", s.version, s.fields.len());
    for f in &s.fields {
        let req = if f.optional { "optional" } else { "required" };
        println!("  {} : {} ({})", f.path, f.r#type, req);
    }
    Ok(())
}
