use assert_cmd::Command;
use predicates::prelude::*;

fn orma() -> Command {
    Command::cargo_bin("orma").unwrap()
}

#[test]
fn help_names_the_resolve_subcommand() {
    orma()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("resolve"));
}

#[test]
fn a_missing_positional_is_a_usage_error() {
    orma().arg("resolve").assert().code(1);
}

/// The paths below do not exist. Nothing reads them yet, and once something
/// does, this case is settled before the volume is ever consulted.
#[test]
fn writing_requires_an_output_path() {
    orma()
        .args(["resolve", "schema.yaml", "volume"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("requires an output path"));
}

#[test]
fn resolve_refuses_what_it_cannot_evaluate() {
    orma()
        .args(["resolve", "schema.yaml", "volume", "--evaluate-only"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("not implemented in this build"));
}
