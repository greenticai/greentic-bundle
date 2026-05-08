use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

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
    let _deployer_guard = temporarily_hide_deployer_packs(build_dir)?;
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

fn temporarily_hide_deployer_packs(build_dir: &Path) -> Result<Option<HiddenDeployerPacks>> {
    let deployer_dir = build_dir.join("providers").join("deployer");
    if !deployer_dir.exists() {
        return Ok(None);
    }
    let stash_root = std::env::temp_dir().join(format!(
        "greentic-warmup-hidden-deployer-{}",
        std::process::id()
    ));
    fs::create_dir_all(&stash_root)
        .with_context(|| format!("create deployer stash root {}", stash_root.display()))?;
    let hidden_dir = unique_stash_path(&stash_root, build_dir);
    fs::rename(&deployer_dir, &hidden_dir).with_context(|| {
        format!(
            "temporarily hide deployer packs {} -> {}",
            deployer_dir.display(),
            hidden_dir.display()
        )
    })?;
    Ok(Some(HiddenDeployerPacks {
        original_dir: deployer_dir,
        hidden_dir,
        stash_root,
    }))
}

fn unique_stash_path(stash_root: &Path, build_dir: &Path) -> PathBuf {
    let mut base = sanitize_for_path(build_dir);
    if base.is_empty() {
        base = "bundle".to_string();
    }
    let mut candidate = stash_root.join(format!("{base}-deployer"));
    let mut idx = 1usize;
    while candidate.exists() {
        candidate = stash_root.join(format!("{base}-deployer-{idx}"));
        idx += 1;
    }
    candidate
}

fn sanitize_for_path(path: &Path) -> String {
    let raw: OsString = path.as_os_str().to_os_string();
    raw.to_string_lossy()
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect()
}

struct HiddenDeployerPacks {
    original_dir: PathBuf,
    hidden_dir: PathBuf,
    stash_root: PathBuf,
}

impl Drop for HiddenDeployerPacks {
    fn drop(&mut self) {
        if !self.hidden_dir.exists() {
            return;
        }
        if let Some(parent) = self.original_dir.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::rename(&self.hidden_dir, &self.original_dir);
        let _ = fs::remove_dir(&self.stash_root);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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

    #[test]
    fn warmup_hides_deployer_packs_from_tool_and_restores_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let providers_dir = dir.path().join("providers");
        let deployer_dir = providers_dir.join("deployer");
        fs::create_dir_all(&deployer_dir).expect("mkdir deployer");
        fs::write(deployer_dir.join("aws.gtpack"), b"not-a-runtime-pack").expect("write pack");

        let tool = dir.path().join("fake-warmup.sh");
        fs::write(
            &tool,
            "#!/usr/bin/env bash\nset -e\nbundle=\"$2\"\nif [ -d \"$bundle/providers/deployer\" ]; then echo visible >&2; exit 42; fi\nif [ -d \"$bundle/.greentic-warmup-hidden-deployer\" ]; then echo hidden-visible >&2; exit 43; fi\n",
        )
        .expect("write tool");
        let mut perms = fs::metadata(&tool).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tool, perms).expect("chmod");

        warmup_with_tool(tool.to_str().expect("tool utf8"), dir.path()).expect("warmup ok");
        assert!(
            deployer_dir.exists(),
            "deployer dir should be restored after warmup"
        );
        assert!(
            deployer_dir.join("aws.gtpack").exists(),
            "deployer pack should be restored after warmup"
        );
    }
}
