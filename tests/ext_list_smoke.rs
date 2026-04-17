#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_list_finds_fixture() {
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("greentic.bundle-fixture"))
        .stdout(predicate::str::contains("recipe=standard"))
        .stdout(predicate::str::contains("Builtin"));
}

#[test]
fn ext_list_empty_dir_prints_nothing() {
    let tmp = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            tmp.path().to_str().unwrap(),
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
