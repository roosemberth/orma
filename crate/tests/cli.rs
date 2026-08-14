use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_fs::TempDir;
use assert_fs::prelude::*;
use predicates::prelude::*;

const MACHINE_ID: &str = "d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1";

fn orma() -> Command {
    Command::cargo_bin("orma").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "fixtures", name]
        .iter()
        .collect()
}

/// Returns a volume holding `values` at the specified paths.
fn volume(path_and_values: &[(&str, &str)]) -> TempDir {
    let volume = TempDir::new().unwrap();
    for (path, value) in path_and_values {
        volume
            .child(path.trim_start_matches('/'))
            .write_str(value)
            .unwrap();
    }
    volume
}

/// Run orma resolve and evaluate `schema` against `values`.
fn evaluate(schema: &str, values: &[(&str, &str)]) -> assert_cmd::assert::Assert {
    let volume = volume(values);
    orma()
        .arg("resolve")
        .arg(fixture(schema))
        .arg(volume.path())
        .arg("--evaluate-only")
        .assert()
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

#[test]
fn a_volume_that_is_not_a_directory_is_a_usage_error() {
    orma()
        .arg("resolve")
        .arg(fixture("schema-empty.yaml"))
        .args(["no-such-volume", "--evaluate-only"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn an_unsupported_version_is_refused() {
    evaluate("schema-unknown-version.yaml", &[])
        .code(3)
        .stderr(predicate::str::contains("unsupported schema version: 2"));
}

#[test]
fn a_type_orma_does_not_implement_is_refused() {
    evaluate("schema-unknown-type.yaml", &[])
        .code(3)
        .stderr(predicate::str::contains(
            "/ssh/host_ed25519_key: field type 'ssh-host-key' is not implemented",
        ));
}

#[test]
fn a_schema_declaring_nothing_is_satisfied() {
    evaluate("schema-empty.yaml", &[]).success();
}

#[test]
fn a_volume_holding_the_declared_values_is_satisfied() {
    evaluate("schema-example.yaml", &[("/machine-id", MACHINE_ID)]).success();
}

#[test]
fn a_missing_value_fails_the_volume() {
    evaluate("schema-example.yaml", &[])
        .code(2)
        .stderr(predicate::str::contains(
            "/machine-id: required but missing",
        ));
}

#[test]
fn a_value_of_the_wrong_shape_fails_the_volume() {
    evaluate(
        "schema-example.yaml",
        &[("/machine-id", "not-a-machine-id")],
    )
    .code(2)
    .stderr(predicate::str::contains("/machine-id: expected 32"));
}

#[test]
fn an_output_that_is_not_a_directory_is_a_usage_error() {
    let volume = volume(&[]);
    orma()
        .arg("resolve")
        .arg(fixture("schema-empty.yaml"))
        .arg(volume.path())
        .arg("no-such-output")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn accepted_values_are_provisioned_at_the_output() {
    let volume = volume(&[("/machine-id", MACHINE_ID)]);
    let output = TempDir::new().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .success();

    output.child("machine-id").assert(MACHINE_ID);
}

#[test]
fn provisioned_values_are_only_readable_by_its_owner() {
    let volume = volume(&[("/machine-id", MACHINE_ID)]);
    let output = TempDir::new().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .success();

    let provisioned = std::fs::metadata(output.child("machine-id").path()).unwrap();
    assert_eq!(provisioned.permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_volume_that_fails_its_schema_provisions_nothing() {
    let volume = volume(&[]);
    let output = TempDir::new().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .code(2);

    output
        .child("machine-id")
        .assert(predicate::path::missing());
}

#[test]
fn help_names_the_generate_subcommand() {
    orma()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("generate"));
}

fn generate(volume: &TempDir) -> assert_cmd::assert::Assert {
    orma()
        .arg("generate")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .assert()
}

#[test]
fn generate_populates_an_empty_volume() {
    let volume = volume(&[]);
    generate(&volume).success();

    volume
        .child("machine-id")
        .assert(predicate::str::is_match(r"^[0-9a-f]{32}\n?$").unwrap());
    let produced = std::fs::metadata(volume.child("machine-id").path()).unwrap();
    assert_eq!(produced.permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_generated_volume_satisfies_its_schema() {
    let volume = volume(&[]);
    generate(&volume).success();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg("--evaluate-only")
        .assert()
        .success();
}

#[test]
fn generate_refuses_to_overwrite() {
    let volume = volume(&[("/machine-id", MACHINE_ID)]);
    generate(&volume)
        .code(2)
        .stderr(predicate::str::contains("would overwrite:\n/machine-id"));

    volume.child("machine-id").assert(MACHINE_ID);
}

const CRYPT_RECORD: &str = "$y$j9T$saltSaltSalt$hashHashHashHash";

#[test]
fn a_volume_holding_a_crypt_record_is_satisfied() {
    evaluate(
        "schema-hashed-password.yaml",
        &[("/user.passwd", CRYPT_RECORD)],
    )
    .success();
}

#[test]
fn a_value_that_is_not_a_crypt_record_fails_the_volume() {
    evaluate(
        "schema-hashed-password.yaml",
        &[("/user.passwd", "hunter2")],
    )
    .code(2)
    .stderr(predicate::str::contains("/user.passwd: not a crypt record"));
}

#[test]
fn generate_refuses_a_type_it_cannot_produce() {
    let volume = volume(&[]);
    orma()
        .arg("generate")
        .arg(fixture("schema-hashed-password.yaml"))
        .arg(volume.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains(
            "/user.passwd: producing a 'hashed-password' is not implemented",
        ));

    volume
        .child("user.passwd")
        .assert(predicate::path::missing());
}
