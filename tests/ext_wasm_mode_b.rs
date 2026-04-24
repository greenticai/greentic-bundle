//! Integration test for Mode B WASM execution dispatcher.
//!
//! Requires `GREENTIC_EXT_ALLOW_UNSIGNED=1` because the dummy fixture is unsigned.
//! Set automatically by the test setup below.

use greentic_bundle::ext::wasm::BundleWasmInvoker;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dummy-bundle-ext")
}

#[test]
fn dummy_ext_render_round_trip() {
    let dir = fixture_dir();
    assert!(
        dir.join("extension.wasm").exists(),
        "extension.wasm missing — run tests/fixtures/dummy-bundle-ext/build.sh"
    );

    // Allow unsigned for test fixture.
    // SAFETY: single-threaded test; no concurrent env reads.
    unsafe { std::env::set_var("GREENTIC_EXT_ALLOW_UNSIGNED", "1") };

    let invoker = greentic_bundle::ext::wasm::WasmtimeBundleInvoker::from_ext_dirs(&[dir])
        .expect("invoker init");

    let session_json =
        r#"{"flows_json":"[]","contents_json":"[]","assets":[],"capabilities_used":[]}"#;
    let result = invoker
        .invoke(greentic_bundle::ext::wasm::WasmInvocation {
            extension_id: "greentic.dummy-bundle-ext",
            recipe_id: "dummy",
            config_json: "{}",
            session_json,
        })
        .expect("invoke");

    assert_eq!(result.filename, "dummy-0.0.1.gtpack");
    assert_eq!(result.bytes, b"PK\x03\x04dummy-pack-bytes");
    assert_eq!(result.sha256.len(), 64);
}

#[test]
fn invoke_unknown_extension_id() {
    let invoker = greentic_bundle::ext::wasm::WasmtimeBundleInvoker::from_ext_dirs(&[])
        .expect("invoker init");

    let err = invoker
        .invoke(greentic_bundle::ext::wasm::WasmInvocation {
            extension_id: "missing.ext",
            recipe_id: "x",
            config_json: "{}",
            session_json: "{}",
        })
        .unwrap_err();

    assert!(
        !matches!(
            err,
            greentic_bundle::ext::errors::ExtensionError::ModeBNotImplemented
        ),
        "got ModeBNotImplemented — implementation regressed"
    );
}

#[test]
#[ignore = "requires bundle-standard-0.2.0 installed at ~/.greentic/extensions/bundle/ — run with --ignored"]
fn bundle_standard_0_2_0_end_to_end_render() {
    // SAFETY: single-threaded test; no concurrent env reads.
    unsafe {
        std::env::set_var("GREENTIC_EXT_ALLOW_UNSIGNED", "1");
    }

    let home = dirs::home_dir().expect("home dir");
    let ext_dir = home.join(".greentic/extensions/bundle/greentic.bundle-standard-0.2.0");
    assert!(
        ext_dir.exists(),
        "bundle-standard 0.2.0 not installed at {ext_dir:?}"
    );

    let invoker = greentic_bundle::ext::wasm::WasmtimeBundleInvoker::from_ext_dirs(&[ext_dir])
        .expect("invoker init");

    // Minimal cards input: one AdaptiveCard "welcome".
    let session_json = r#"{"flows_json":"","contents_json":"[{\"id\":\"welcome\",\"json\":{\"type\":\"AdaptiveCard\",\"version\":\"1.5\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"hello\"}]}}]","assets":[],"capabilities_used":[]}"#;
    let config_json = r#"{"metadata":{"name":"smoke","version":"0.0.1"},"channels":["webchat"],"format":"gtpack-legacy"}"#;

    let result = invoker
        .invoke(greentic_bundle::ext::wasm::WasmInvocation {
            extension_id: "greentic.bundle-standard",
            recipe_id: "standard",
            config_json,
            session_json,
        })
        .expect("render");

    assert_eq!(result.filename, "smoke-0.0.1.gtpack");
    assert!(!result.bytes.is_empty());
    assert_eq!(result.sha256.len(), 64);

    // Verify the output is a valid ZIP with expected files.
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(result.bytes)).expect("valid zip");
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "bundle.yaml"),
        "missing bundle.yaml in output: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains("flows/main.ygtc")),
        "missing flows/main.ygtc: {names:?}"
    );
    assert!(
        names
            .iter()
            .any(|n| n.contains("assets/cards/welcome.json")),
        "missing welcome.json: {names:?}"
    );
}
