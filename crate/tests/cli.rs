use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn orma() -> Command {
    Command::cargo_bin("orma").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "fixtures", name]
        .iter()
        .collect()
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

#[test]
fn writing_requires_an_output_path() {
    orma()
        .args(["resolve", "schema.yaml", "volume"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("requires an output path"));
}

#[test]
fn an_unreadable_schema_is_a_usage_error() {
    orma()
        .args([
            "resolve",
            "no-such-schema.yaml",
            "volume",
            "--evaluate-only",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no-such-schema.yaml"));
}

/// The volume is not consulted: a schema declaring nothing asks nothing of it.
#[test]
fn a_schema_declaring_nothing_is_satisfied() {
    orma()
        .arg("resolve")
        .arg(fixture("schema-empty.yaml"))
        .args(["volume", "--evaluate-only"])
        .assert()
        .success();
}

#[test]
fn declared_fields_cannot_be_evaluated_yet() {
    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .args(["volume", "--evaluate-only"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "/machine-id: field type 'machine-id' is not implemented",
        ));
}

#[test]
fn an_unsupported_version_is_refused() {
    orma()
        .arg("resolve")
        .arg(fixture("schema-unknown-version.yaml"))
        .args(["volume", "--evaluate-only"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("unsupported schema version: 2"));
}
