#![cfg(feature = "extensions")]

use assert_cmd::Command;
use serde_json::Value;

#[test]
fn ext_render_json_on_success_emits_single_line_summary() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("demo.gtpack");

    let assert = Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
            "--session",
            "tests/data/designer-session.json",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();

    let raw = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let line = raw.trim();
    let v: Value = serde_json::from_str(line).expect("stdout must be a single JSON line");
    assert_eq!(v["status"], "ok");
    assert!(v["filename"].is_string());
    assert_eq!(v["sha256"].as_str().unwrap().len(), 64);
    assert!(v["bytesLen"].as_u64().unwrap() > 0);
}
