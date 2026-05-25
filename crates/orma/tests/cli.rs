use assert_cmd::Command;
use assert_fs::TempDir;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

const VALID_CRYPT_HASH: &str = "$y$j9T$saltSaltSalt$hashHashHashHashHash";
const VALID_MACHINE_ID: &str = "deadbeefdeadbeefdeadbeefdeadbeef";

fn orma() -> Command {
    Command::cargo_bin("orma").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "fixtures", name]
        .iter()
        .collect()
}

/// Seed `volume` with values for every required field in `schema-example.yaml`.
/// Optional fields are left absent; tests can add them explicitly.
fn seed_example_volume(volume: &ChildPath) {
    volume.create_dir_all().unwrap();
    volume
        .child("machine-id")
        .write_str(VALID_MACHINE_ID)
        .unwrap();
    volume
        .child("passwd.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();
}

/// Assert that every file in `src` exists in `dst` with the same bytes.
fn assert_dir_contents_match(src: &Path, dst: &Path) {
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let s = fs::read(entry.path()).unwrap();
        let d = fs::read(dst.join(&name)).unwrap_or_else(|_| {
            panic!("missing in dst: {}", name.to_string_lossy())
        });
        assert_eq!(s, d, "differs: {}", name.to_string_lossy());
    }
}

#[test]
fn generate_happy_path() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();

    orma()
        .arg("generate")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .write_stdin("secret\nsecret\nsudo\nsudo\n")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Passphrase for User password (/passwd.hash): ",
        ));

    volume
        .child("machine-id")
        .assert(predicate::str::is_match(r"^[0-9a-f]{32}\n?$").unwrap());
    volume
        .child("passwd.hash")
        .assert(predicate::str::starts_with("$y$"));
    volume
        .child("sudo.hash")
        .assert(predicate::str::starts_with("$y$"));
}

#[test]
fn generate_refuses_overwrite() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    volume
        .child("passwd.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();

    orma()
        .arg("generate")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .write_stdin("secret\nsecret\nsudo\nsudo\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("would overwrite"));

    let contents =
        fs::read_to_string(volume.child("passwd.hash").path()).unwrap();
    assert_eq!(contents, VALID_CRYPT_HASH);
}

#[test]
fn generate_force_overwrites() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    volume
        .child("passwd.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();

    orma()
        .arg("generate")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg("--force")
        .write_stdin("secret\nsecret\nsudo\nsudo\n")
        .assert()
        .success();

    let contents =
        fs::read_to_string(volume.child("passwd.hash").path()).unwrap();
    assert_ne!(contents, VALID_CRYPT_HASH);
    assert!(contents.starts_with("$y$"));
}

#[test]
fn generate_mismatched_passphrases() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();

    orma()
        .arg("generate")
        .arg(fixture("schema-required-only.yaml"))
        .arg(volume.path())
        .write_stdin("a\nb\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("passphrases do not match"));

    volume
        .child("passwd.hash")
        .assert(predicate::path::missing());
}

#[test]
fn resolve_happy_path() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    seed_example_volume(&volume);
    volume
        .child("sudo.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();
    let output = tmp.child("output");
    output.create_dir_all().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .success();

    assert_dir_contents_match(volume.path(), output.path());
}

#[test]
fn resolve_required_missing() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    volume
        .child("sudo.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();
    let output = tmp.child("output");
    output.create_dir_all().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("required but missing"));

    output
        .child("passwd.hash")
        .assert(predicate::path::missing());
    output.child("sudo.hash").assert(predicate::path::missing());
}

#[test]
fn resolve_optional_missing() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    seed_example_volume(&volume);
    let output = tmp.child("output");
    output.create_dir_all().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .success();

    output.child("sudo.hash").assert(predicate::path::missing());
}

#[test]
fn resolve_evaluate_only() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    volume
        .child("passwd.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();

    orma()
        .arg("resolve")
        .arg("--evaluate-only")
        .arg(fixture("schema-required-only.yaml"))
        .arg(volume.path())
        .assert()
        .success();
}

#[test]
fn resolve_unsupported_version() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    let output = tmp.child("output");
    output.create_dir_all().unwrap();

    orma()
        .arg("resolve")
        .arg(fixture("schema-v2.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported schema version"));
}

#[test]
fn roundtrip_generate_then_resolve() {
    let tmp = TempDir::new().unwrap();
    let volume = tmp.child("volume");
    volume.create_dir_all().unwrap();
    let output = tmp.child("output");
    output.create_dir_all().unwrap();

    orma()
        .arg("generate")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .write_stdin("hello\nhello\nworld\nworld\n")
        .assert()
        .success();

    orma()
        .arg("resolve")
        .arg(fixture("schema-example.yaml"))
        .arg(volume.path())
        .arg(output.path())
        .assert()
        .success();

    assert_dir_contents_match(volume.path(), output.path());
}
