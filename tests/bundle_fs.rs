use std::fs;

use greentic_bundle::bundle_fs::{
    BackhandBundleFsReader, BackhandBundleFsWriter, BundleFsReader, BundleFsWriter,
    read_bundle_file,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn staged_bundle() -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    fs::write(temp.path().join("pack.cbor"), b"pack").expect("pack");
    fs::write(temp.path().join("bundle.cbor"), b"bundle").expect("bundle");
    fs::create_dir_all(temp.path().join("assets")).expect("assets dir");
    fs::write(temp.path().join("assets").join("example.txt"), b"example").expect("asset");
    temp
}

fn create_bundle(input: &TempDir, output_name: &str) -> (TempDir, std::path::PathBuf) {
    let out = TempDir::new().expect("out dir");
    let bundle = out.path().join(output_name);
    BackhandBundleFsWriter
        .write_bundle(input.path(), &bundle)
        .expect("write bundle");
    (out, bundle)
}

#[test]
fn creates_gtbundle_with_backhand() {
    let input = staged_bundle();
    let (_out, bundle) = create_bundle(&input, "example.gtbundle");

    assert!(bundle.exists());
    assert!(bundle.metadata().expect("metadata").len() > 0);
}

#[test]
fn backhand_bundle_contains_expected_files() {
    let input = staged_bundle();
    let (_out, bundle) = create_bundle(&input, "example.gtbundle");

    let mut paths: Vec<_> = BackhandBundleFsReader
        .list_bundle(&bundle)
        .expect("list bundle")
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    paths.sort();

    assert!(paths.contains(&"assets".to_string()));
    assert!(paths.contains(&"assets/example.txt".to_string()));
    assert!(paths.contains(&"bundle.cbor".to_string()));
    assert!(paths.contains(&"pack.cbor".to_string()));
}

#[test]
fn backhand_bundle_uses_normalized_paths() {
    let input = staged_bundle();
    let (_out, bundle) = create_bundle(&input, "example.gtbundle");

    let paths: Vec<_> = BackhandBundleFsReader
        .list_bundle(&bundle)
        .expect("list bundle")
        .into_iter()
        .map(|entry| entry.path)
        .collect();

    assert!(paths.iter().all(|path| !path.contains('\\')));
    assert!(paths.iter().all(|path| !path.starts_with('/')));
}

#[test]
fn backhand_bundle_extracts_expected_files() {
    let input = staged_bundle();
    let (_out, bundle) = create_bundle(&input, "example.gtbundle");
    let extract = TempDir::new().expect("extract dir");

    BackhandBundleFsReader
        .extract_bundle(&bundle, extract.path())
        .expect("extract bundle");

    assert_eq!(
        fs::read_to_string(extract.path().join("assets").join("example.txt")).expect("asset"),
        "example"
    );
    assert_eq!(
        read_bundle_file(&bundle, "bundle.cbor").expect("read bundle.cbor"),
        b"bundle"
    );
}

#[test]
fn backhand_bundle_rebuild_is_deterministic() {
    let input = staged_bundle();
    let (_out_one, bundle_one) = create_bundle(&input, "one.gtbundle");
    let (_out_two, bundle_two) = create_bundle(&input, "two.gtbundle");

    let hash_one = Sha256::digest(fs::read(&bundle_one).expect("bundle one"));
    let hash_two = Sha256::digest(fs::read(&bundle_two).expect("bundle two"));

    assert_eq!(hash_one[..], hash_two[..]);
}
