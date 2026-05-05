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
    let cache_dir = build_dir.join(".cache");
    let output = Command::new(WARMUP_TOOL)
        .arg("warmup")
        .arg("--bundle")
        .arg(build_dir)
        .arg("--cache-dir")
        .arg(&cache_dir)
        .arg("--strict")
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => anyhow::anyhow!(
                "required tool `{WARMUP_TOOL}` was not found on PATH; install greentic-start to embed precompiled component cache, or run `greentic-bundle build` with `--no-warmup` to skip"
            ),
            _ => anyhow::Error::new(error).context(format!("spawn {WARMUP_TOOL} warmup")),
        })?;
    if !output.status.success() {
        bail!(
            "{WARMUP_TOOL} warmup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
