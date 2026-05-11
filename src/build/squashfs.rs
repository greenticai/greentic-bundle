use std::path::Path;

use anyhow::Result;

pub fn build_artifact(source_dir: &Path, artifact: &Path) -> Result<()> {
    crate::bundle_fs::write_bundle(source_dir, artifact)
}

pub fn read_artifact_file(artifact: &Path, inner_path: &str) -> Result<String> {
    Ok(String::from_utf8(crate::bundle_fs::read_bundle_file(
        artifact, inner_path,
    )?)?)
}

pub fn list_artifact_contents(artifact: &Path) -> Result<Vec<String>> {
    Ok(crate::bundle_fs::list_bundle(artifact)?
        .into_iter()
        .map(|entry| entry.path)
        .collect())
}

pub fn unpack_artifact(artifact: &Path, output_dir: &Path) -> Result<()> {
    crate::bundle_fs::extract_bundle(artifact, output_dir)
}
