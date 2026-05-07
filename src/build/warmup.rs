use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

const WARMUP_TOOL: &str = "greentic-start";

/// Run `greentic-start warmup` against `build_dir`, writing precompiled cwasm
/// artifacts under `<build_dir>/.cache/v1/<engine_profile_id>/artifacts/` so
/// the resulting `.gtbundle` ships a warm component cache.
///
/// The runner-host (`greentic-runner-host`) reads the cache at start when
/// `GREENTIC_CACHE_DIR` points at `<bundle>/.cache`. greentic-start auto-adopts
/// that directory when the bundle ships one, so consumers of warmup-baked
/// bundles get faster cold start without further configuration.
pub fn warmup_build_dir(build_dir: &Path) -> Result<()> {
    warmup_with_tool(WARMUP_TOOL, build_dir)
}

fn warmup_with_tool(tool: &str, build_dir: &Path) -> Result<()> {
    let cache_dir = build_dir.join(".cache");
    let output = Command::new(tool)
        .arg("warmup")
        .arg("--bundle")
        .arg(build_dir)
        .arg("--cache-dir")
        .arg(&cache_dir)
        .arg("--strict")
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `{tool}` was not found on PATH; install greentic-start to embed precompiled component cache, or run `greentic-bundle build` with `--no-warmup` to skip"
            ),
            _ => anyhow::Error::new(error).context(format!("spawn {tool} warmup")),
        })?;
    if !output.status.success() {
        bail!(
            "{tool} warmup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_reports_friendly_path_hint() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = warmup_with_tool(
            "__greentic_warmup_tool_definitely_does_not_exist__",
            dir.path(),
        )
        .expect_err("missing tool must error");
        let msg = err.to_string();
        assert!(
            msg.contains("not found on PATH"),
            "expected NotFound hint, got: {msg}"
        );
        assert!(
            msg.contains("--no-warmup"),
            "expected hint about --no-warmup, got: {msg}"
        );
    }

    #[test]
    fn nonzero_exit_reports_warmup_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = warmup_with_tool("false", dir.path()).expect_err("`false` exits non-zero");
        assert!(
            err.to_string().starts_with("false warmup failed"),
            "got: {}",
            err
        );
    }

    #[test]
    fn successful_tool_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        warmup_with_tool("true", dir.path()).expect("`true` exits zero");
    }
}
