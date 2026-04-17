#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_info_prints_metadata() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "info",
            "greentic.bundle-fixture",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("greentic.bundle-fixture 0.0.1"))
        .stdout(predicate::str::contains("recipe: standard"))
        .stdout(predicate::str::contains("greentic:flows/*"));
}

#[test]
fn ext_info_missing_returns_error() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "info",
            "greentic.bundle-missing",
        ])
        .assert()
        .failure();
}
