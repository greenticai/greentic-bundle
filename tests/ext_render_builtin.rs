#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn ext_render_produces_valid_gtpack() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("smoke-demo-0.1.0.gtpack");

    Command::cargo_bin("greentic-bundle")
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
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sha256="));

    let bytes = std::fs::read(&out).unwrap();
    assert!(!bytes.is_empty());
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(names.iter().any(|n| n.ends_with("bundle.yaml")));
    assert!(names.iter().any(|n| n.ends_with("flows/main.ygtc")));
    assert!(
        names
            .iter()
            .any(|n| n.ends_with("assets/cards/welcome.json"))
    );
}

#[test]
fn ext_render_reproducible() {
    let tmp = tempfile::TempDir::new().unwrap();
    let a = tmp.path().join("a.gtpack");
    let b = tmp.path().join("b.gtpack");

    for out in [&a, &b] {
        Command::cargo_bin("greentic-bundle")
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
            ])
            .assert()
            .success();
    }

    use sha2::{Digest, Sha256};
    let hash = |p: &std::path::Path| -> String {
        let mut h = Sha256::new();
        h.update(std::fs::read(p).unwrap());
        let out = h.finalize();
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(out.len() * 2);
        for b in out.iter() {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    };
    assert_eq!(hash(&a), hash(&b));
}
