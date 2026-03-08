use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=i18n/locales.json");
    println!("cargo:rerun-if-changed=i18n");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let i18n_dir = manifest_dir.join("i18n");
    let locales_path = i18n_dir.join("locales.json");
    let locales_raw = fs::read_to_string(&locales_path).expect("read i18n/locales.json");
    let locales: Vec<String> = serde_json::from_str(&locales_raw).expect("parse locales.json");

    let mut catalogs = BTreeMap::new();
    for locale in locales {
        let path = i18n_dir.join(format!("{locale}.json"));
        let catalog_raw = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let _: BTreeMap<String, String> = serde_json::from_str(&catalog_raw)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        catalogs.insert(locale, path);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let dest = out_dir.join("embedded_i18n.rs");
    let mut generated = String::from(
        "pub fn load_embedded_catalogs() -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {\n    let mut catalogs = std::collections::BTreeMap::new();\n",
    );
    for (locale, path) in catalogs {
        generated.push_str(&format!(
            "    catalogs.insert({locale:?}.to_string(), serde_json::from_str::<std::collections::BTreeMap<String, String>>(include_str!({path:?})).expect(\"embedded locale catalog must be valid\"));\n",
            locale = locale,
            path = path.display(),
        ));
    }
    generated.push_str("    catalogs\n}\n");
    fs::write(dest, generated).expect("write embedded_i18n.rs");
}
