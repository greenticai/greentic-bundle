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
