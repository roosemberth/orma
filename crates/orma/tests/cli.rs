use assert_cmd::Command;
use assert_fs::TempDir;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

const VALID_CRYPT_HASH: &str = "$y$j9T$saltSaltSalt$hashHashHashHashHash";

fn orma() -> Command {
    Command::cargo_bin("orma").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "fixtures", name]
        .iter()
        .collect()
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
        .success();

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
    volume.create_dir_all().unwrap();
    volume
        .child("passwd.hash")
        .write_str(VALID_CRYPT_HASH)
        .unwrap();
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

    output.child("passwd.hash").assert(VALID_CRYPT_HASH);
    output.child("sudo.hash").assert(VALID_CRYPT_HASH);
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
    volume.create_dir_all().unwrap();
    volume
        .child("passwd.hash")
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

    output.child("passwd.hash").assert(VALID_CRYPT_HASH);
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

    let v_pw = fs::read_to_string(volume.child("passwd.hash").path()).unwrap();
    let o_pw = fs::read_to_string(output.child("passwd.hash").path()).unwrap();
    assert_eq!(v_pw, o_pw);

    let v_su = fs::read_to_string(volume.child("sudo.hash").path()).unwrap();
    let o_su = fs::read_to_string(output.child("sudo.hash").path()).unwrap();
    assert_eq!(v_su, o_su);
}
