//! Build a pack, unzip it, assert workspace structure intact.

use bundle_standard_core::*;
use serde_json::json;
use std::io::Read;

fn cfg(name: &str) -> StandardConfig {
    StandardConfig {
        metadata: StandardMetadata {
            name: name.into(),
            version: "0.1.0".into(),
            author: None,
        },
        channels: vec!["webchat".into()],
        embed_ui: "webchat".into(),
        i18n: I18nConfig::default(),
        format: "gtpack-legacy".into(),
    }
}

#[test]
fn pack_unzip_contains_expected_files() {
    let cfg = cfg("demo");
    let flows = vec![FlowEntry {
        name: "main".into(),
        yaml: "id: demo\nschema_version: 2\n".into(),
    }];
    let cards = vec![CardContentEntry {
        id: "welcome".into(),
        json: json!({"type":"AdaptiveCard"}),
    }];
    let inputs = PackInputs {
        config: &cfg,
        flows: &flows,
        cards: &cards,
        assets: &[],
        capabilities: &[],
    };

    let out = build_pack(&inputs).unwrap();
    assert_eq!(out.filename, "demo-0.1.0.gtpack");

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .collect();
    assert!(names.iter().any(|n| n == "bundle.yaml"));
    assert!(names.iter().any(|n| n == "flows/main.ygtc"));
    assert!(names.iter().any(|n| n == "assets/cards/welcome.json"));
    assert!(names.iter().any(|n| n == "tenants/default/tenant.gmap"));

    let mut f = zip.by_name("flows/main.ygtc").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    assert!(s.contains("id: demo"));
}

#[test]
fn assets_pass_through_verbatim() {
    let cfg = cfg("demo");
    let png_bytes = vec![0x89, 0x50, 0x4e, 0x47]; // PNG header
    let assets = vec![("logo.png".into(), png_bytes.clone())];
    let inputs = PackInputs {
        config: &cfg,
        flows: &[],
        cards: &[],
        assets: &assets,
        capabilities: &[],
    };

    let out = build_pack(&inputs).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
    let mut f = zip.by_name("assets/logo.png").unwrap();
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, png_bytes);
}
