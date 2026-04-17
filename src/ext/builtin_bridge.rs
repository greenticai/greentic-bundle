//! Builtin bridge: `BuiltinRecipeId::Standard` → ephemeral workspace + ZIP output.

use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::RenderedArtifact;

#[derive(Debug, Deserialize)]
pub struct DesignerSession {
    pub flows_json: String,
    pub contents_json: String,
    #[serde(default)]
    pub assets: Vec<(String, Vec<u8>)>,
    #[serde(default)]
    pub capabilities_used: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StandardConfig {
    pub metadata: StandardMetadata,
    pub channels: Vec<String>,
    #[serde(default = "default_embed_ui")]
    pub embed_ui: String,
    #[serde(default)]
    pub i18n: I18nConfig,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct StandardMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_source")]
    pub source: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_embed_ui() -> String {
    "none".into()
}
fn default_format() -> String {
    "gtpack-legacy".into()
}
fn default_i18n_source() -> String {
    "en".into()
}

pub fn handle_standard(
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    let config: StandardConfig = serde_json::from_str(config_json)?;
    let session: DesignerSession = serde_json::from_str(session_json)?;

    if config.format != "gtpack-legacy" {
        return Err(ExtensionError::InvalidConfig(format!(
            "format '{}' not supported in Phase A (only 'gtpack-legacy')",
            config.format,
        )));
    }

    let session_id = compute_session_id(&session, config_json);
    let tmp_root = tempfile::Builder::new()
        .prefix(&format!("ext-render-{session_id}-"))
        .tempdir()?;

    write_ephemeral_workspace(tmp_root.path(), &session, &config)?;
    let bytes = zip_workspace(tmp_root.path())?;

    let sha256 = hex_sha256(&bytes);
    let filename = format!("{}-{}.gtpack", config.metadata.name, config.metadata.version);
    Ok(RenderedArtifact {
        filename,
        bytes,
        sha256,
    })
}

/// Deterministic 16-hex-char session id.
fn compute_session_id(session: &DesignerSession, config_json: &str) -> String {
    let mut h = Sha256::new();
    h.update(session.flows_json.as_bytes());
    h.update(b"\x00");
    h.update(session.contents_json.as_bytes());
    h.update(b"\x00");
    let mut assets = session.assets.clone();
    assets.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in &assets {
        h.update(k.as_bytes());
        h.update(b"\x00");
        h.update(v);
        h.update(b"\x00");
    }
    h.update(config_json.as_bytes());
    let out = h.finalize();
    hex_encode(&out[..8])
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_encode(&h.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn write_ephemeral_workspace(
    root: &Path,
    session: &DesignerSession,
    config: &StandardConfig,
) -> Result<(), ExtensionError> {
    use std::fs;
    fs::create_dir_all(root.join("flows"))?;
    fs::create_dir_all(root.join("assets").join("cards"))?;
    fs::create_dir_all(root.join("tenants").join("default"))?;

    // flows — session.flows_json is a JSON array of { "name": ..., "yaml": ... }.
    let flows: Vec<serde_json::Value> = serde_json::from_str(&session.flows_json)?;
    for (i, f) in flows.iter().enumerate() {
        let name = f
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("flow-{i:03}"));
        let yaml = f
            .get("yaml")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExtensionError::InvalidConfig(format!("flow '{name}' missing 'yaml'")))?;
        fs::write(root.join("flows").join(format!("{name}.ygtc")), yaml)?;
    }

    // contents — array of { "id": ..., "json": ... }.
    let contents: Vec<serde_json::Value> = serde_json::from_str(&session.contents_json)?;
    for c in contents.iter() {
        let id = c
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExtensionError::InvalidConfig("content missing 'id'".into()))?;
        let json = c.get("json").ok_or_else(|| {
            ExtensionError::InvalidConfig(format!("content '{id}' missing 'json'"))
        })?;
        fs::write(
            root.join("assets").join("cards").join(format!("{id}.json")),
            serde_json::to_vec_pretty(json)?,
        )?;
    }

    // raw assets.
    for (rel, bytes) in &session.assets {
        let dst = root.join("assets").join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dst, bytes)?;
    }

    // synthesize bundle.yaml.
    let channels_yaml: String = config
        .channels
        .iter()
        .map(|c| format!("  - {c}\n"))
        .collect();
    let bundle_yaml = format!(
        "apiVersion: greentic.ai/v1\nkind: BundleWorkspace\nmetadata:\n  name: {}\n  version: {}\nchannels:\n{}",
        config.metadata.name, config.metadata.version, channels_yaml,
    );
    fs::write(root.join("bundle.yaml"), bundle_yaml)?;

    // synthesize tenant.gmap.
    let caps_yaml: String = session
        .capabilities_used
        .iter()
        .map(|c| format!("  - {c}\n"))
        .collect();
    let tenant_gmap = format!(
        "# generated by ext bridge\ntenant: default\ncapabilities:\n{caps_yaml}",
    );
    fs::write(
        root.join("tenants").join("default").join("tenant.gmap"),
        tenant_gmap,
    )?;

    Ok(())
}

/// Walk the workspace directory and produce a ZIP in memory (Deflated, sorted entries for determinism).
fn zip_workspace(workspace_root: &Path) -> Result<Vec<u8>, ExtensionError> {
    use std::fs;
    use std::io::Write;

    // Collect + sort paths for deterministic ZIP ordering.
    let entries: Vec<_> = walkdir::WalkDir::new(workspace_root)
        .sort_by_file_name()
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ExtensionError::Io(std::io::Error::other(e)))?;

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for entry in entries {
            let path = entry.path();
            let rel = path.strip_prefix(workspace_root).unwrap_or(path);
            if rel.as_os_str().is_empty() {
                continue;
            }
            if entry.file_type().is_dir() {
                zip.add_directory(rel.to_string_lossy(), options)
                    .map_err(zip_io)?;
                continue;
            }
            if entry.file_type().is_file() {
                zip.start_file(rel.to_string_lossy(), options)
                    .map_err(zip_io)?;
                let bytes = fs::read(path)?;
                zip.write_all(&bytes)?;
            }
        }
        zip.finish().map_err(zip_io)?;
    }
    Ok(buf)
}

fn zip_io(e: zip::result::ZipError) -> ExtensionError {
    ExtensionError::Io(std::io::Error::other(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_CONFIG: &str = r#"{
      "metadata": { "name": "demo", "version": "0.1.0" },
      "channels": ["webchat"],
      "format": "gtpack-legacy"
    }"#;

    const MIN_SESSION: &str = r#"{
      "flows_json": "[{\"name\":\"main\",\"yaml\":\"schemaVersion: 2\\nname: main\"}]",
      "contents_json": "[{\"id\":\"welcome\",\"json\":{\"type\":\"AdaptiveCard\",\"version\":\"1.5\"}}]",
      "assets": [],
      "capabilities_used": ["greentic:adaptive-cards/schema"]
    }"#;

    #[test]
    fn session_id_deterministic() {
        let a = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        let b = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn session_id_differs_on_different_inputs() {
        let a = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            MIN_CONFIG,
        );
        let other_cfg = MIN_CONFIG.replace("demo", "other");
        let b = compute_session_id(
            &serde_json::from_str::<DesignerSession>(MIN_SESSION).unwrap(),
            &other_cfg,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_unsupported_format() {
        let bad_cfg = MIN_CONFIG.replace("gtpack-legacy", "apack");
        let err = handle_standard(&bad_cfg, MIN_SESSION).unwrap_err();
        assert!(matches!(err, ExtensionError::InvalidConfig(_)));
    }

    #[test]
    fn happy_path_produces_artifact() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        assert_eq!(out.filename, "demo-0.1.0.gtpack");
        assert!(!out.bytes.is_empty());
        assert_eq!(out.sha256.len(), 64);
        let again = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        // Deterministic output — same inputs → same sha256.
        assert_eq!(out.sha256, again.sha256);
    }

    #[test]
    fn artifact_is_a_valid_zip_containing_bundle_yaml() {
        let out = handle_standard(MIN_CONFIG, MIN_SESSION).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("bundle.yaml")));
        assert!(names.iter().any(|n| n.ends_with("flows/main.ygtc")));
        assert!(names
            .iter()
            .any(|n| n.ends_with("assets/cards/welcome.json")));
    }
}
