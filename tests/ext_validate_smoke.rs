#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_validate_ok() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "validate",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Config is valid"));
}

#[test]
fn ext_validate_rejects_invalid_config() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        r#"{ "metadata": { "name": "x", "version": "0.1.0" }, "channels": [] }"#,
    )
    .unwrap();
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "validate",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}
