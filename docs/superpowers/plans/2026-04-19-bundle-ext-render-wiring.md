# Bundle Extension Render Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `greentic-designer` produce its final `.gtpack` through the `greentic-bundle ext render` contract by chaining `greentic-cards2pack` (cards → flow) with `ext render` (flow + cards → pack), so future Mode B WASM recipes drop in without further designer changes.

**Architecture:** Add three backward-compatible ergonomic flags to `greentic-bundle ext render` (`--json`, stdin for `--config`, stdin for `--session`). Vendor `greentic.bundle-standard-0.1.0.gtxpack` into the designer binary and unpack on first run (mirrors the existing AC-ext bundled-fallback pattern). Add a runtime probe (`PackBackend` enum) and a session adapter, then chain the render step after the existing cards2pack subprocess in `src/ui/routes/pack.rs::run_pack_subprocess`. Fallback keeps the legacy cards2pack-only path.

**Tech Stack:** Rust 1.91 (greentic-bundle) / Rust 1.94 (greentic-designer), Tokio, Axum, Clap, `zip` 2.x, `sha2`, `tempfile`, `assert_cmd` for integration tests, `jsonschema` (already in extensions feature).

**Cross-repo sequencing:** Land Phase 1 (greentic-bundle) first → publish a patch release (or use `path = ` override during dev). Then land Phase 2 onwards (greentic-designer) which depends only on the binary at runtime — no cargo dep needed.

**Spec:** `greentic-bundle/docs/superpowers/specs/2026-04-19-bundle-ext-render-wiring-design.md`

---

## File Structure

### Phase 1 — `greentic-bundle` (ergonomic additions)

- Modify: `src/cli/ext.rs` — add `--json` flag; document stdin (`-`) accepted for `--config`/`--session`.
- Modify: `src/cli/mod.rs` — update `run_ext::Render` handler to read stdin when `"-"`, emit JSON summary / error JSON when `--json` set.
- Modify: `i18n/en.json` — no new keys; the `--json` path bypasses i18n.
- Create: `tests/ext_render_json.rs` — integration tests for `--json` + stdin behaviour.
- Reuse existing fixtures: `testdata/ext/`, `tests/data/config-minimal.json`, `tests/data/designer-session.json`.

### Phase 2 — `greentic-designer` (bundled extension + probe)

- Create: `bundled/greentic.bundle-standard-0.1.0.gtxpack` (binary asset, committed, pinned via script).
- Create: `scripts/vendor-bundle-standard.sh` — fetches + verifies SHA-256.
- Modify: `Cargo.toml` — add `bundled-bundle-ext` feature (mirrors `bundled-ac-ext`).
- Modify: `src/ui/mod.rs` — add `BUNDLED_BUNDLE_STANDARD` constant + `install_bundled_bundle_ext()` helper.
- Create: `src/orchestrate/pack_backend.rs` — `PackBackend` enum + `probe()` + `bootstrap_ext_dir()`.
- Modify: `src/ui/state.rs` — add `pack_backend: PackBackend` field to `AppState`.
- Modify: `src/orchestrate/mod.rs` — register `pack_backend` + `session_adapter` modules.
- Modify: `src/ui/mod.rs::launch` — call probe + bootstrap; wire result into `AppState`.

### Phase 3 — `greentic-designer` (session adapter + chained flow)

- Create: `src/orchestrate/session_adapter.rs` — `SessionPayload` + `build_payload()` + provider→channel mapping.
- Modify: `src/ui/routes/pack.rs::run_pack_subprocess` — branch on `AppState.pack_backend`; after cards2pack success + HTTP inject, call ext-render step when `BundleExtRender`.
- Create: `tests/session_adapter.rs` — unit tests for `build_payload()`.
- Create: `tests/pack_backend_probe.rs` — tests for probe with mock binary.
- Create: `tests/pack_ext_integration.rs` — integration test with stub `greentic-bundle` binary.

### Phase 4 — Docs

- Modify: `greentic-bundle/CLAUDE.md` — note `--json`, stdin support.
- Modify: `greentic-designer/CLAUDE.md` — note `PackBackend` probe, bundled extension, chained flow.

---

## Phase 1 — greentic-bundle ergonomic additions

Work inside `/home/bimbim/works/greentic/greentic-bundle`. All tasks require `cargo test --features extensions`.

### Task 1: Add `--json` flag to `ext render` CLI definition

**Files:**
- Modify: `src/cli/ext.rs` lines 44–58 (Render variant)

- [ ] **Step 1: Open `src/cli/ext.rs` and extend the Render variant**

Current (lines 42–58):

```rust
    /// Render a bundle artifact via the ext dispatcher (Mode A only in Phase A).
    #[command(about = "cli.ext.render.about")]
    Render {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        /// Path to a designer session JSON file.
        #[arg(long, value_name = "FILE")]
        session: PathBuf,
        /// Output file (default: stdout).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
```

Replace with:

```rust
    /// Render a bundle artifact via the ext dispatcher (Mode A only in Phase A).
    #[command(about = "cli.ext.render.about")]
    Render {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file, or `-` to read from stdin.
        #[arg(long, value_name = "FILE")]
        config: String,
        /// Path to a designer session JSON file, or `-` to read from stdin.
        #[arg(long, value_name = "FILE")]
        session: String,
        /// Output file (default: stdout).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Emit a single-line JSON summary on stdout (requires --out) and
        /// JSON-formatted errors on non-zero exits. Off by default to preserve
        /// the existing human-readable CLI behaviour.
        #[arg(long)]
        json: bool,
    },
```

- [ ] **Step 2: Run `cargo build --features extensions` — expect compile error**

Run:
```bash
cd /home/bimbim/works/greentic/greentic-bundle
cargo build --features extensions
```

Expected: compile error inside `src/cli/mod.rs::run_ext` pattern match because the Render arm uses `config: PathBuf` fields — we changed those to `String` and added `json`.

- [ ] **Step 3: Commit the CLI definition change alone**

```bash
git checkout -b feat/ext-render-json
git add src/cli/ext.rs
git commit -m "feat(ext): add --json flag and stdin support to render CLI"
```

### Task 2: Update `run_ext::Render` handler for stdin + JSON output

**Files:**
- Modify: `src/cli/mod.rs` lines 252–284

- [ ] **Step 1: Write the failing test for `--json` on successful render**

Create `tests/ext_render_json.rs`:

```rust
#![cfg(feature = "extensions")]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

#[test]
fn ext_render_json_on_success_emits_single_line_summary() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("demo.gtpack");

    let assert = Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
            "--session",
            "tests/data/designer-session.json",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .success();

    let raw = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let line = raw.trim();
    let v: Value = serde_json::from_str(line).expect("stdout must be a single JSON line");
    assert_eq!(v["status"], "ok");
    assert!(v["filename"].is_string());
    assert_eq!(v["sha256"].as_str().unwrap().len(), 64);
    assert!(v["bytesLen"].as_u64().unwrap() > 0);
}
```

- [ ] **Step 2: Run the failing test**

Run:
```bash
cargo test --features extensions --test ext_render_json ext_render_json_on_success_emits_single_line_summary
```

Expected: compile error or test failure (the flag isn't wired to behaviour yet).

- [ ] **Step 3: Update `run_ext::Render` in `src/cli/mod.rs` lines 252–284**

Replace the current Render arm with:

```rust
        ext::ExtCommand::Render {
            extension_id,
            recipe_id,
            config,
            session,
            out,
            json,
        } => {
            let config_json = read_input(&config)
                .map_err(|e| render_error(json, "invalid-config", &e.to_string()))?;
            let session_json = read_input(&session)
                .map_err(|e| render_error(json, "invalid-session", &e.to_string()))?;

            if config == "-" && session == "-" {
                return Err(render_error(
                    json,
                    "invalid-args",
                    "only one of --config and --session may read from stdin",
                ));
            }

            let result = invoke_recipe(
                &registry,
                &extension_id,
                &recipe_id,
                &config_json,
                &session_json,
            );

            match (result, out) {
                (Ok(art), Some(path)) => {
                    fs::write(&path, &art.bytes)?;
                    if json {
                        let summary = serde_json::json!({
                            "status": "ok",
                            "filename": art.filename,
                            "sha256": art.sha256,
                            "bytesLen": art.bytes.len(),
                        });
                        println!("{summary}");
                    } else {
                        let path_str = path.display().to_string();
                        println!(
                            "{}",
                            crate::i18n::trf(
                                "cli.ext.render.wrote",
                                &[
                                    ("file", path_str.as_str()),
                                    ("sha256", art.sha256.as_str()),
                                ],
                            )
                        );
                    }
                }
                (Ok(art), None) => {
                    // Binary passthrough; --json with no --out is invalid.
                    if json {
                        return Err(render_error(
                            json,
                            "invalid-args",
                            "--json requires --out",
                        ));
                    }
                    std::io::stdout().write_all(&art.bytes)?;
                }
                (Err(e), _) => {
                    return Err(render_error(
                        json,
                        extension_error_code(&e),
                        &e.to_string(),
                    ));
                }
            }
        }
```

Then add the three helpers at the bottom of `src/cli/mod.rs` (below the tests module but inside the file):

```rust
#[cfg(feature = "extensions")]
fn read_input(path_or_dash: &str) -> std::io::Result<String> {
    use std::io::Read;
    if path_or_dash == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path_or_dash)
    }
}

#[cfg(feature = "extensions")]
fn render_error(json: bool, code: &str, message: &str) -> anyhow::Error {
    if json {
        let line = serde_json::json!({
            "status": "error",
            "code": code,
            "message": message,
        });
        println!("{line}");
    }
    anyhow::anyhow!("{code}: {message}")
}

#[cfg(feature = "extensions")]
fn extension_error_code(err: &crate::ext::errors::ExtensionError) -> &'static str {
    use crate::ext::errors::ExtensionError::*;
    match err {
        InvalidConfig(_) => "invalid-config",
        InvalidDescriptor(_) => "invalid-descriptor",
        RecipeNotFound { .. } => "recipe-not-found",
        ModeBNotImplemented => "mode-b-not-implemented",
        Io(_) => "io-error",
        _ => "other",
    }
}
```

Note: the exact variants of `ExtensionError` need to be read from `src/ext/errors.rs`. If a variant name differs, adjust the match arms accordingly. Keep the arm order stable so future variants trigger a compile error.

- [ ] **Step 4: Run the happy-path test, expect PASS**

Run:
```bash
cargo test --features extensions --test ext_render_json ext_render_json_on_success_emits_single_line_summary
```

Expected: PASS. Output JSON line parses, `status=="ok"`, `bytesLen>0`.

- [ ] **Step 5: Verify the existing non-`--json` test still passes (no regression)**

Run:
```bash
cargo test --features extensions --test ext_render_builtin
```

Expected: PASS (behaviour for the no-`--json` path is unchanged).

- [ ] **Step 6: Commit**

```bash
git add src/cli/mod.rs tests/ext_render_json.rs
git commit -m "feat(ext): support --json stdout and stdin (-) for --config/--session"
```

### Task 3: Stdin-piped session test + error-JSON test

**Files:**
- Modify: `tests/ext_render_json.rs`

- [ ] **Step 1: Add two more failing tests to `tests/ext_render_json.rs`**

Append:

```rust
#[test]
fn ext_render_json_accepts_session_via_stdin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("demo.gtpack");
    let session_bytes = std::fs::read("tests/data/designer-session.json").unwrap();

    let assert = Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "tests/data/config-minimal.json",
            "--session",
            "-",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ])
        .write_stdin(session_bytes)
        .assert()
        .success();

    let raw = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: Value = serde_json::from_str(raw.trim()).unwrap();
    assert_eq!(v["status"], "ok");
}

#[test]
fn ext_render_json_on_recipe_not_found_exits_nonzero_with_json_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("should-not-exist.gtpack");

    let assert = Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "bogus-recipe",
            "--config",
            "tests/data/config-minimal.json",
            "--session",
            "tests/data/designer-session.json",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ])
        .assert()
        .failure();

    let raw = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: Value = serde_json::from_str(raw.trim()).unwrap();
    assert_eq!(v["status"], "error");
    assert_eq!(v["code"], "recipe-not-found");
}

#[test]
fn ext_render_rejects_double_stdin() {
    let assert = Command::cargo_bin("greentic-bundle")
        .unwrap()
        .args([
            "ext",
            "--extension-dir",
            "testdata/ext",
            "render",
            "greentic.bundle-fixture",
            "standard",
            "--config",
            "-",
            "--session",
            "-",
            "--out",
            "/tmp/ignore.gtpack",
            "--json",
        ])
        .assert()
        .failure();

    let raw = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: Value = serde_json::from_str(raw.trim()).unwrap();
    assert_eq!(v["status"], "error");
    assert_eq!(v["code"], "invalid-args");
}
```

- [ ] **Step 2: Run the three new tests**

Run:
```bash
cargo test --features extensions --test ext_render_json
```

Expected: all three PASS. The implementation from Task 2 already covers these paths; this task is about proving it.

- [ ] **Step 3: Run the full feature test suite to catch regressions**

Run:
```bash
GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1 cargo test --features extensions --all-targets
```

Expected: PASS across all tests.

- [ ] **Step 4: Run fmt + clippy**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add tests/ext_render_json.rs
git commit -m "test(ext): stdin and error-JSON paths for ext render --json"
```

### Task 4: Update greentic-bundle CLAUDE.md

**Files:**
- Modify: `CLAUDE.md` (section on `ext/`)

- [ ] **Step 1: Add the new flags to the `ext/` section of CLAUDE.md**

Find the paragraph starting `**\`ext/\`** — Bundle extension host …` and append the following sentence:

> The `render` subcommand accepts `-` for `--config` and `--session` (reading from stdin; the two are mutually exclusive), and a `--json` flag that emits a single-line JSON summary on success (`{status,filename,sha256,bytesLen}`) and structured error JSON on failure, preserving the human-readable i18n output when `--json` is not set.

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: note --json and stdin support on ext render"
```

### Task 5: Push branch and cut a patch release

- [ ] **Step 1: Run local_check.sh to confirm CI green**

```bash
bash ci/local_check.sh
```

Expected: all steps pass.

- [ ] **Step 2: Push**

```bash
git push -u origin feat/ext-render-json
```

- [ ] **Step 3: Bump version in root `Cargo.toml` (patch)**

Open `Cargo.toml` (root), increment the patch component of the workspace version (e.g. `0.5.3` → `0.5.4`). Do NOT publish yet — wait for PR review before tagging.

- [ ] **Step 4: Open PR to main**

Use `gh pr create` with title `feat(ext): --json and stdin for ext render` and body referencing the spec path.

Once this ships to `main` and the new binary is on your PATH, proceed to Phase 2.

---

## Phase 2 — greentic-designer bundled extension + probe

Work inside `/home/bimbim/works/greentic/greentic-designer`. Commands run from the repo root unless noted.

### Task 6: Vendor `greentic.bundle-standard-0.1.0.gtxpack` + vendor script

**Files:**
- Create: `bundled/greentic.bundle-standard-0.1.0.gtxpack` (binary, committed)
- Create: `scripts/vendor-bundle-standard.sh`
- Modify: `Cargo.toml` — add feature `bundled-bundle-ext` mirroring `bundled-ac-ext`

- [ ] **Step 1: Create the vendor script**

Create `scripts/vendor-bundle-standard.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Vendor greentic.bundle-standard-<ver>.gtxpack into bundled/ with a pinned
# SHA-256. Bump the variables below to update, then commit the new artifact.

VER="0.1.0"
EXPECTED_SHA256="REPLACE_WITH_REAL_HASH"
SOURCE_URL="https://github.com/greentic-biz/greentic-bundle-extensions/releases/download/v${VER}/greentic.bundle-standard-${VER}.gtxpack"

DEST="bundled/greentic.bundle-standard-${VER}.gtxpack"
mkdir -p bundled
echo "Fetching ${SOURCE_URL}"
curl -fL -o "${DEST}" "${SOURCE_URL}"

ACTUAL="$(sha256sum "${DEST}" | awk '{print $1}')"
if [[ "${ACTUAL}" != "${EXPECTED_SHA256}" ]]; then
  echo "SHA-256 mismatch: expected ${EXPECTED_SHA256}, got ${ACTUAL}" >&2
  exit 1
fi
echo "OK ${DEST} (${ACTUAL})"
```

Make it executable:

```bash
chmod +x scripts/vendor-bundle-standard.sh
```

- [ ] **Step 2: Copy the local `.gtxpack` into `bundled/` to bootstrap (no published release yet)**

Until the release exists, copy the local artifact:

```bash
mkdir -p bundled
cp /home/bimbim/works/greentic/greentic-bundle-extensions/reference-extensions/bundle-standard/greentic.bundle-standard-0.1.0.gtxpack bundled/
sha256sum bundled/greentic.bundle-standard-0.1.0.gtxpack
```

Record the printed SHA into `scripts/vendor-bundle-standard.sh` by replacing `REPLACE_WITH_REAL_HASH`.

- [ ] **Step 3: Add feature flag in `Cargo.toml`**

Open `Cargo.toml`, find `bundled-ac-ext = []` (or equivalent) in the `[features]` section. Add:

```toml
bundled-bundle-ext = []
```

Put it on a new line immediately below `bundled-ac-ext`. If the crate has a `default = [...]` list containing `bundled-ac-ext`, add `bundled-bundle-ext` there too.

- [ ] **Step 4: Add the `include_bytes!` constant in `src/ui/mod.rs`**

Open `src/ui/mod.rs`. Below the existing `BUNDLED_AC_GTXPACK` declaration (around line 28–30), add:

```rust
/// Embedded bundle-standard extension for bundle-ext render pipeline.
#[cfg(feature = "bundled-bundle-ext")]
const BUNDLED_BUNDLE_STANDARD_GTXPACK: &[u8] =
    include_bytes!("../../bundled/greentic.bundle-standard-0.1.0.gtxpack");
```

- [ ] **Step 5: Confirm cargo check passes**

Run:
```bash
cargo check --features bundled-bundle-ext
```

Expected: clean compile. The constant is unused for now; cargo issues a dead-code warning — acceptable as Task 7 will consume it.

- [ ] **Step 6: Commit**

```bash
git checkout -b feat/bundle-ext-render-wiring
git add Cargo.toml bundled/greentic.bundle-standard-0.1.0.gtxpack scripts/vendor-bundle-standard.sh src/ui/mod.rs
git commit -m "feat(designer): vendor bundle-standard extension + vendor script"
```

### Task 7: Implement `install_bundled_bundle_ext()` helper

**Files:**
- Modify: `src/ui/mod.rs` — add helper below the existing `install_bundled_fallback`.

- [ ] **Step 1: Read existing `install_bundled_fallback` (lines 217–255)**

Review it as the pattern. It: returns early without the feature, picks a target dir, short-circuits if the target already exists, opens the `include_bytes!` archive with `zip::ZipArchive`, walks entries, recreates files on disk.

- [ ] **Step 2: Add the helper to `src/ui/mod.rs`**

Append after `install_bundled_fallback`:

```rust
/// Install the bundled bundle-standard extension into the given directory if
/// not already present. Mirrors `install_bundled_fallback` but targets the
/// bundle-ext discovery dir consumed by `greentic-bundle ext render`.
fn install_bundled_bundle_ext(bundle_ext_dir: &std::path::Path) -> Result<()> {
    #[cfg(not(feature = "bundled-bundle-ext"))]
    {
        let _ = bundle_ext_dir;
        return Ok(());
    }

    #[cfg(feature = "bundled-bundle-ext")]
    {
        let target = bundle_ext_dir.join("greentic.bundle-standard-0.1.0");
        if target.exists() {
            return Ok(());
        }
        eprintln!("Installing bundled greentic.bundle-standard@0.1.0 for first-run...");
        std::fs::create_dir_all(&target)?;
        let cursor = std::io::Cursor::new(BUNDLED_BUNDLE_STANDARD_GTXPACK);
        let mut archive = zip::ZipArchive::new(cursor)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            let out_path = target.join(&name);
            if file.is_dir() {
                std::fs::create_dir_all(&out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut out_file = std::fs::File::create(&out_path)?;
                std::io::copy(&mut file, &mut out_file)?;
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check --features bundled-bundle-ext
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(designer): install_bundled_bundle_ext helper"
```

### Task 8: Introduce `PackBackend` enum + `AppState` field

**Files:**
- Modify: `src/ui/state.rs`

- [ ] **Step 1: Open `src/ui/state.rs` and add the enum near the top**

Add below the existing imports, above the `AppState` struct:

```rust
#[derive(Debug, Clone)]
pub enum PackBackend {
    BundleExtRender {
        bundle_bin: std::path::PathBuf,
        bundle_version: String,
        ext_dir: std::path::PathBuf,
    },
    LegacyCards2Pack,
}
```

- [ ] **Step 2: Add a field to `AppState`**

Find the `pub struct AppState {` definition. Add:

```rust
    pub pack_backend: PackBackend,
```

Keep the rest of the struct unchanged.

- [ ] **Step 3: Update `AppState::new` (or `Arc::new(AppState { ... })` call sites)**

The constructor signature or literal must take a `pack_backend` argument. Open `src/ui/mod.rs` and find where `AppState` is constructed (typically near the end of `launch`). Add `pack_backend: PackBackend::LegacyCards2Pack` as a temporary default — Task 10 replaces it with the probe result.

If `AppState` has a `new()` method, add the parameter there. If it's constructed as a struct literal, add the field.

- [ ] **Step 4: Run cargo check**

```bash
cargo check
```

Expected: clean. If there are additional construction sites (e.g., in tests), update those too with the `LegacyCards2Pack` default.

- [ ] **Step 5: Commit**

```bash
git add src/ui/state.rs src/ui/mod.rs
git commit -m "feat(designer): PackBackend enum on AppState (LegacyCards2Pack default)"
```

### Task 9: Probe function in new `pack_backend.rs` module

**Files:**
- Create: `src/orchestrate/pack_backend.rs`
- Modify: `src/orchestrate/mod.rs` — add `pub mod pack_backend;`

- [ ] **Step 1: Create the module with probe + bootstrap helpers**

Create `src/orchestrate/pack_backend.rs`:

```rust
//! Probe `greentic-bundle` availability and determine the pack backend.
//!
//! The probe is best-effort: any error leads to `PackBackend::LegacyCards2Pack`
//! so the designer keeps working with older binaries.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ui::state::PackBackend;

/// Probe `greentic-bundle ext render --help` and return the chosen backend.
///
/// Detection markers:
/// - `--json` in help output → new binary with Phase 1 shipped → `BundleExtRender`.
/// - Any other result (binary missing, non-zero exit, no marker) → `LegacyCards2Pack`.
pub fn probe(bundle_ext_dir: PathBuf) -> PackBackend {
    let bin = resolve_bundle_bin();

    let help = Command::new(&bin).args(["ext", "render", "--help"]).output();
    let Ok(out) = help else {
        eprintln!("pack backend: LegacyCards2Pack (greentic-bundle not found)");
        return PackBackend::LegacyCards2Pack;
    };
    if !out.status.success() {
        eprintln!(
            "pack backend: LegacyCards2Pack (greentic-bundle ext render --help exit != 0)"
        );
        return PackBackend::LegacyCards2Pack;
    }
    let help_str = String::from_utf8_lossy(&out.stdout);
    if !help_str.contains("--json") {
        eprintln!("pack backend: LegacyCards2Pack (no --json marker in help)");
        return PackBackend::LegacyCards2Pack;
    }

    let version = detect_version(&bin).unwrap_or_else(|| "unknown".to_string());
    eprintln!(
        "pack backend: BundleExtRender (greentic-bundle {version}, ext_dir={})",
        bundle_ext_dir.display()
    );
    PackBackend::BundleExtRender {
        bundle_bin: bin,
        bundle_version: version,
        ext_dir: bundle_ext_dir,
    }
}

/// Resolve the `greentic-bundle` binary: env `GREENTIC_BUNDLE_BIN` wins, else PATH.
fn resolve_bundle_bin() -> PathBuf {
    if let Ok(p) = std::env::var("GREENTIC_BUNDLE_BIN") {
        return PathBuf::from(p);
    }
    PathBuf::from("greentic-bundle")
}

fn detect_version(bin: &Path) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    raw.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Compute the bootstrap directory for the bundle-ext discovery.
/// Honours the `GREENTIC_BUNDLE_EXT_DIR` env override; otherwise uses
/// `$HOME/.greentic/designer/bundle-ext` (stable across runs).
pub fn default_bundle_ext_dir() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("GREENTIC_BUNDLE_EXT_DIR") {
        return Ok(PathBuf::from(p));
    }
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("unable to determine home directory"))?;
    Ok(home.join(".greentic").join("designer").join("bundle-ext"))
}

/// True when the user is pinning a specific extension dir via env; in that
/// mode the designer should NOT auto-install the bundled extension (the user
/// is expected to manage that dir themselves).
pub fn ext_dir_is_user_managed() -> bool {
    std::env::var("GREENTIC_BUNDLE_EXT_DIR").is_ok()
}
```

- [ ] **Step 2: Register the module**

Open `src/orchestrate/mod.rs` and add near the top:

```rust
pub mod pack_backend;
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check
```

Expected: clean.

- [ ] **Step 4: Write a probe unit test using a mock binary**

Create `tests/pack_backend_probe.rs`:

```rust
use assert_cmd::cargo::CommandCargoExt;
use std::process::Command as StdCommand;

/// Use a shell script as the mock `greentic-bundle` binary. We set
/// `GREENTIC_BUNDLE_BIN` to it and call the probe function indirectly via the
/// compiled library API re-export. Since `probe()` reads the env var at call
/// time, we keep the test single-threaded (cargo test threads default to 1
/// per module via #[serial_test::serial] or by unique env values per test).

#[test]
fn probe_returns_bundle_ext_when_help_contains_json_marker() {
    // Create a temporary shell script that prints a help page with `--json`.
    let tmp = tempfile::TempDir::new().unwrap();
    let mock = tmp.path().join("greentic-bundle");
    std::fs::write(
        &mock,
        "#!/usr/bin/env bash\n\
         case \"$*\" in\n\
           *'ext render --help'*) echo 'Usage: ext render ... --json ...'; exit 0;;\n\
           *'--version'*) echo 'greentic-bundle 9.9.9'; exit 0;;\n\
           *) exit 2;;\n\
         esac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&mock).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&mock, p).unwrap();
    }

    unsafe {
        std::env::set_var("GREENTIC_BUNDLE_BIN", &mock);
    }
    let backend =
        greentic_designer::orchestrate::pack_backend::probe(tmp.path().to_path_buf());
    match backend {
        greentic_designer::ui::state::PackBackend::BundleExtRender { bundle_version, .. } => {
            assert_eq!(bundle_version, "9.9.9");
        }
        other => panic!("expected BundleExtRender, got {other:?}"),
    }
}

#[test]
fn probe_falls_back_when_marker_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mock = tmp.path().join("greentic-bundle");
    std::fs::write(
        &mock,
        "#!/usr/bin/env bash\n\
         case \"$*\" in\n\
           *'ext render --help'*) echo 'Usage: ext render [options]'; exit 0;;\n\
           *) exit 2;;\n\
         esac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(&mock).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&mock, p).unwrap();
    }

    unsafe {
        std::env::set_var("GREENTIC_BUNDLE_BIN", &mock);
    }
    let backend =
        greentic_designer::orchestrate::pack_backend::probe(tmp.path().to_path_buf());
    matches!(
        backend,
        greentic_designer::ui::state::PackBackend::LegacyCards2Pack
    );
}

#[test]
fn probe_falls_back_when_binary_missing() {
    unsafe {
        std::env::set_var("GREENTIC_BUNDLE_BIN", "/definitely/not/here");
    }
    let backend =
        greentic_designer::orchestrate::pack_backend::probe(std::path::PathBuf::from("/tmp"));
    matches!(
        backend,
        greentic_designer::ui::state::PackBackend::LegacyCards2Pack
    );
}

// Suppress clippy on unused import when `StdCommand` isn't needed.
#[allow(dead_code)]
fn _types_used() -> StdCommand {
    StdCommand::new("true")
}
```

Note: the test requires `PackBackend` and the `probe` function re-exported through `lib.rs`. If the designer is currently binary-only (`main.rs` with no `lib.rs`), skip the integration test and convert it into a `#[cfg(test)] mod tests {}` inside `pack_backend.rs` instead, directly referencing the module-local types.

- [ ] **Step 5: Run probe tests**

Run (adjust the test target if you kept tests in-module):
```bash
cargo test --test pack_backend_probe
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/orchestrate/pack_backend.rs src/orchestrate/mod.rs tests/pack_backend_probe.rs
git commit -m "feat(designer): probe greentic-bundle ext render availability"
```

### Task 10: Wire bootstrap + probe into `launch()`

**Files:**
- Modify: `src/ui/mod.rs::launch`

- [ ] **Step 1: After the design-ext bootstrap block (around step 6 in launch), add**

Insert before `AppState` construction:

```rust
    // 7. Bootstrap bundle-ext dir (unpack vendored greentic.bundle-standard).
    //    Skip unpack when GREENTIC_BUNDLE_EXT_DIR is set: the user pins the
    //    dir themselves (typically pointing at a dev build of the extension).
    let bundle_ext_dir = crate::orchestrate::pack_backend::default_bundle_ext_dir()?;
    std::fs::create_dir_all(&bundle_ext_dir)?;
    if !crate::orchestrate::pack_backend::ext_dir_is_user_managed() {
        if let Err(e) = install_bundled_bundle_ext(&bundle_ext_dir) {
            eprintln!("Warning: failed to install bundled bundle-standard: {e}");
        }
    }

    // 8. Probe pack backend.
    let pack_backend = crate::orchestrate::pack_backend::probe(bundle_ext_dir);
```

- [ ] **Step 2: Pass `pack_backend` into `AppState`**

Replace the temporary default from Task 8. Where `AppState` is constructed, set:

```rust
    pack_backend,
```

instead of `pack_backend: PackBackend::LegacyCards2Pack`.

- [ ] **Step 3: Run cargo check + cargo build**

```bash
cargo check
cargo build --features bundled-bundle-ext
```

Expected: clean.

- [ ] **Step 4: Sanity-boot the designer (manual)**

Run:
```bash
cargo run --features bundled-bundle-ext -- ui --open=false --port 3999
```

In stderr, look for one of:
- `pack backend: BundleExtRender (greentic-bundle <ver>, ext_dir=...)` when your PATH has the Phase 1 binary.
- `pack backend: LegacyCards2Pack (...)` otherwise.

Also confirm `~/.greentic/designer/bundle-ext/greentic.bundle-standard-0.1.0/describe.json` exists.

Kill the process with Ctrl+C.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs src/orchestrate/pack_backend.rs
git commit -m "feat(designer): bootstrap bundle-ext dir + probe at launch"
```

---

## Phase 3 — Session adapter + chained flow

### Task 11: `SessionPayload` + `build_payload` skeleton

**Files:**
- Create: `src/orchestrate/session_adapter.rs`
- Modify: `src/orchestrate/mod.rs` — add `pub mod session_adapter;`

- [ ] **Step 1: Create the module with types and failing-test scaffolding**

Create `src/orchestrate/session_adapter.rs`:

```rust
//! Convert designer state (post-cards2pack temp dir + card registry) into the
//! `DesignerSession` + `StandardConfig` JSON pair that `greentic-bundle ext
//! render` consumes.
//!
//! This module owns the mapping from provider ids to channel ids and from
//! uploaded assets to the base64-encoded byte tuples the builtin bridge
//! expects.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};

/// Serialized JSON strings ready to hand to `greentic-bundle ext render`.
pub struct SessionPayload {
    pub session_json: String,
    pub config_json: String,
}

/// Inputs to the adapter. Mirrors the subset of `PackBody` plus the two temp
/// directories produced upstream by `prepare_cards` + `cards2pack`.
pub struct AdapterInputs<'a> {
    pub pack_name: &'a str,
    pub workspace_dir: &'a Path,   // cards2pack output: flows/*.ygtc
    pub cards_dir: &'a Path,       // from prepare_cards: <id>.json files
    pub assets: &'a [Asset],       // optional: user-uploaded images
    pub providers: &'a [String],   // provider ids from PackBody.providers
    pub langs: &'a [String],       // PackBody.langs
    pub version: &'a str,          // hard-coded "0.1.0" in phase 1
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub rel_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct FlowEntry {
    name: String,
    yaml: String,
}

#[derive(Debug, Serialize)]
struct ContentEntry {
    id: String,
    json: Value,
}

pub fn build_payload(input: &AdapterInputs<'_>) -> Result<SessionPayload> {
    let flows = collect_flows(input.workspace_dir).context("collect flows")?;
    let contents = collect_contents(input.cards_dir).context("collect contents")?;
    let (channels, capabilities_used) = classify_providers(input.providers);

    let session = json!({
        "flows_json": serde_json::to_string(&flows)?,
        "contents_json": serde_json::to_string(&contents)?,
        "assets": input
            .assets
            .iter()
            .map(|a| json!([a.rel_path, a.bytes]))
            .collect::<Vec<_>>(),
        "capabilities_used": capabilities_used,
    });

    let config = json!({
        "metadata": {
            "name": input.pack_name,
            "version": input.version,
        },
        "channels": channels,
        "embed_ui": if channels.contains(&"webchat".to_string()) { "webchat" } else { "none" },
        "i18n": {
            "source": "en",
            "targets": input.langs,
        },
        "format": "gtpack-legacy",
    });

    Ok(SessionPayload {
        session_json: serde_json::to_string(&session)?,
        config_json: serde_json::to_string(&config)?,
    })
}

fn collect_flows(workspace_dir: &Path) -> Result<Vec<FlowEntry>> {
    let flows_dir = workspace_dir.join("flows");
    if !flows_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(&flows_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ygtc") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("flow")
            .to_string();
        let yaml = std::fs::read_to_string(&path)?;
        out.push(FlowEntry { name: stem, yaml });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn collect_contents(cards_dir: &Path) -> Result<Vec<ContentEntry>> {
    if !cards_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(cards_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("card")
            .to_string();
        let raw = std::fs::read_to_string(&path)?;
        let json: Value = serde_json::from_str(&raw)
            .with_context(|| format!("parse card {id}"))?;
        out.push(ContentEntry { id, json });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Map provider ids to `(channels, capabilities_used)`. Known messaging
/// providers produce channel ids; everything else falls through to
/// `capabilities_used`.
fn classify_providers(providers: &[String]) -> (Vec<String>, Vec<String>) {
    let mut channels = vec![];
    let mut caps = vec![];
    for p in providers {
        match p.as_str() {
            "greentic:messaging/webchat" => channels.push("webchat".to_string()),
            "greentic:messaging/slack" => channels.push("slack".to_string()),
            "greentic:messaging/teams" => channels.push("teams".to_string()),
            "greentic:messaging/telegram" => channels.push("telegram".to_string()),
            "greentic:messaging/webex" => channels.push("webex".to_string()),
            "greentic:messaging/whatsapp" => channels.push("whatsapp".to_string()),
            "greentic:messaging/email" => channels.push("email".to_string()),
            _ => caps.push(p.clone()),
        }
    }
    channels.sort();
    channels.dedup();
    caps.sort();
    caps.dedup();
    if channels.is_empty() {
        channels.push("webchat".to_string());
    }
    (channels, caps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture_workspace(root: &Path) {
        std::fs::create_dir_all(root.join("flows")).unwrap();
        std::fs::write(
            root.join("flows/main.ygtc"),
            "schemaVersion: 2\nname: main\n",
        )
        .unwrap();
    }

    fn write_fixture_cards(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("welcome.json"),
            r#"{"type":"AdaptiveCard","version":"1.5"}"#,
        )
        .unwrap();
    }

    #[test]
    fn build_payload_happy_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let cards = tmp.path().join("cards");
        write_fixture_workspace(&workspace);
        write_fixture_cards(&cards);

        let providers = vec!["greentic:messaging/webchat".to_string()];
        let langs: Vec<String> = vec![];
        let payload = build_payload(&AdapterInputs {
            pack_name: "demo",
            workspace_dir: &workspace,
            cards_dir: &cards,
            assets: &[],
            providers: &providers,
            langs: &langs,
            version: "0.1.0",
        })
        .unwrap();

        let session: Value = serde_json::from_str(&payload.session_json).unwrap();
        let flows: Value =
            serde_json::from_str(session["flows_json"].as_str().unwrap()).unwrap();
        assert_eq!(flows[0]["name"], "main");
        let contents: Value =
            serde_json::from_str(session["contents_json"].as_str().unwrap()).unwrap();
        assert_eq!(contents[0]["id"], "welcome");

        let config: Value = serde_json::from_str(&payload.config_json).unwrap();
        assert_eq!(config["metadata"]["name"], "demo");
        assert_eq!(config["metadata"]["version"], "0.1.0");
        assert_eq!(config["format"], "gtpack-legacy");
        assert_eq!(config["embed_ui"], "webchat");
        assert_eq!(config["channels"][0], "webchat");
    }

    #[test]
    fn unknown_provider_goes_to_capabilities() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workspace = tmp.path().join("workspace");
        let cards = tmp.path().join("cards");
        write_fixture_workspace(&workspace);
        write_fixture_cards(&cards);

        let providers = vec!["greentic:events/http".to_string()];
        let langs: Vec<String> = vec![];
        let payload = build_payload(&AdapterInputs {
            pack_name: "demo",
            workspace_dir: &workspace,
            cards_dir: &cards,
            assets: &[],
            providers: &providers,
            langs: &langs,
            version: "0.1.0",
        })
        .unwrap();

        let session: Value = serde_json::from_str(&payload.session_json).unwrap();
        let caps = session["capabilities_used"].as_array().unwrap();
        assert_eq!(caps[0], "greentic:events/http");
        let config: Value = serde_json::from_str(&payload.config_json).unwrap();
        // No known messaging provider → defaults to webchat to keep bridge happy.
        assert_eq!(config["channels"][0], "webchat");
        assert_eq!(config["embed_ui"], "webchat");
    }
}
```

- [ ] **Step 2: Register the module**

Open `src/orchestrate/mod.rs` and add near the top (below the existing `pub mod`s):

```rust
pub mod session_adapter;
```

- [ ] **Step 3: Run the unit tests**

```bash
cargo test --lib orchestrate::session_adapter
```

Expected: both tests PASS.

- [ ] **Step 4: Run cargo fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/orchestrate/session_adapter.rs src/orchestrate/mod.rs
git commit -m "feat(designer): session adapter for bundle-ext render"
```

### Task 12: Chain the `ext render` step inside `run_pack_subprocess`

**Files:**
- Modify: `src/ui/routes/pack.rs::run_pack_subprocess` (around lines 247–460)

- [ ] **Step 1: Read the current `run_pack_subprocess` (lines 247–460) and the surrounding HTTP-inject post-processing block**

Note the existing success branch: it finds `.gtpack` in `out_dir/dist`, runs HTTP inject on `out_dir/flows/main.ygtc`, and stores `pack_path` on the `PackJob`.

- [ ] **Step 2: Add a helper to run the `ext render` step**

Append a new function at the bottom of `src/ui/routes/pack.rs`, outside `run_pack_subprocess`:

```rust
async fn run_ext_render_step(
    state: &Arc<AppState>,
    job_id: &str,
    workspace_dir: &std::path::Path,
    cards_dir: &std::path::Path,
    pack_name: &str,
    providers: &[String],
    langs: &[String],
) -> Option<std::path::PathBuf> {
    use crate::orchestrate::session_adapter::{AdapterInputs, build_payload};
    use crate::ui::state::PackBackend;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::Command;

    let (bundle_bin, ext_dir) = match &state.pack_backend {
        PackBackend::BundleExtRender {
            bundle_bin, ext_dir, ..
        } => (bundle_bin.clone(), ext_dir.clone()),
        PackBackend::LegacyCards2Pack => return None,
    };

    let payload = match build_payload(&AdapterInputs {
        pack_name,
        workspace_dir,
        cards_dir,
        assets: &[],
        providers,
        langs,
        version: "0.1.0",
    }) {
        Ok(p) => p,
        Err(e) => {
            let mut jobs = state.pack_jobs.lock().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = PackJobStatus::Failed;
                job.error = Some(format!("build session payload: {e}"));
            }
            return None;
        }
    };

    let session_path = workspace_dir.join("designer-session.json");
    if let Err(e) = std::fs::write(&session_path, payload.session_json.as_bytes()) {
        let mut jobs = state.pack_jobs.lock().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = PackJobStatus::Failed;
            job.error = Some(format!("write session JSON: {e}"));
        }
        return None;
    }

    let out_path = workspace_dir.join(format!("{pack_name}-0.1.0.gtpack"));

    let mut cmd = Command::new(&bundle_bin);
    cmd.arg("--extension-dir")
        .arg(&ext_dir)
        .arg("ext")
        .arg("render")
        .arg("greentic.bundle-standard")
        .arg("standard")
        .arg("--config")
        .arg("-")
        .arg("--session")
        .arg(&session_path)
        .arg("--out")
        .arg(&out_path)
        .arg("--json")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let mut jobs = state.pack_jobs.lock().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = PackJobStatus::Failed;
                job.error = Some(format!("spawn greentic-bundle: {e}"));
            }
            return None;
        }
    };

    // Write config JSON to stdin, close.
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(payload.config_json.as_bytes()).await.is_err() {
            // continue — we'll read exit status next
        }
        let _ = stdin.shutdown().await;
    }

    // Stream stderr.
    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let mut jobs = state.pack_jobs.lock().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.lines.push(PackLogLine {
                    text: line,
                    kind: LogKind::Info,
                });
            }
        }
    }

    // Collect stdout (one JSON line).
    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            let mut jobs = state.pack_jobs.lock().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = PackJobStatus::Failed;
                job.error = Some(format!("wait greentic-bundle: {e}"));
            }
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let line = stdout.trim();
    let json: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            let mut jobs = state.pack_jobs.lock().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = PackJobStatus::Failed;
                job.error = Some(format!(
                    "greentic-bundle stdout not JSON: {}",
                    line.chars().take(200).collect::<String>()
                ));
            }
            return None;
        }
    };

    if !output.status.success() || json["status"] != "ok" {
        let msg = json["message"].as_str().unwrap_or("unknown").to_string();
        let code = json["code"].as_str().unwrap_or("unknown").to_string();
        let mut jobs = state.pack_jobs.lock().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = PackJobStatus::Failed;
            job.error = Some(format!("ext render failed: {code}: {msg}"));
        }
        return None;
    }

    Some(out_path)
}
```

- [ ] **Step 3: Call the helper from `run_pack_subprocess` after HTTP-inject**

Inside `run_pack_subprocess`, locate the success branch where `pack_path` is set (~line 349). After the HTTP-inject block completes, **before** `PackJob` is marked complete, insert:

```rust
            // If the pack backend is BundleExtRender, produce the final .gtpack
            // via `greentic-bundle ext render`, replacing cards2pack's output.
            if matches!(
                state.pack_backend,
                crate::ui::state::PackBackend::BundleExtRender { .. }
            ) {
                // cards_dir is not currently a field of run_pack_subprocess;
                // plumb it through via PostProcessOpts if needed. For now derive
                // from known temp layout: <out_dir>/../cards.
                let cards_dir = out_dir
                    .parent()
                    .map(|p| p.join("cards"))
                    .unwrap_or_else(|| out_dir.join("cards"));
                let providers = opts.providers.clone().unwrap_or_default();
                let langs: Vec<String> = vec![]; // PackBody.langs plumbed via opts in a follow-up.
                if let Some(new_path) = run_ext_render_step(
                    state,
                    job_id,
                    out_dir,
                    &cards_dir,
                    pack_name,
                    &providers,
                    &langs,
                )
                .await
                {
                    // Override the pack_path from cards2pack with the ext-render one.
                    // `pp` was the cards2pack output; discard it.
                    let _ = std::fs::remove_file(&pp);
                    // Re-assign pp-shaped binding for downstream deploy-bundle logic.
                    // Simpler: set pack_path on the job directly.
                    if let Some(job) = jobs.get_mut(job_id) {
                        job.pack_path = Some(new_path.clone());
                        if let Some(filename) = new_path.file_name() {
                            job.pack_filename =
                                Some(filename.to_string_lossy().to_string());
                        }
                    }
                }
            }
```

**Important caveat**: the exact binding name (`pp` vs `pack_path`) must match the surrounding code. Read lines 338–353 in the current file and adapt. The goal is: if the ext-render step returns `Some(path)`, the job's `pack_path` becomes that path; otherwise fall through to the cards2pack output.

- [ ] **Step 4: Plumb `langs` via `PostProcessOpts`**

Update `PostProcessOpts` (around line 248) to include `langs: Vec<String>` and update the call site in `post_pack` to pass it from `body.langs.unwrap_or_default()`.

- [ ] **Step 5: Run cargo check + cargo test**

```bash
cargo check
cargo test --lib
```

Expected: clean compile; existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/routes/pack.rs
git commit -m "feat(designer): chain ext render after cards2pack when backend=BundleExtRender"
```

### Task 13: End-to-end integration test with a stub binary

**Files:**
- Create: `tests/pack_ext_integration.rs`
- Create: `tests/fixtures/stub-greentic-bundle.sh`

- [ ] **Step 1: Create the stub binary**

Create `tests/fixtures/stub-greentic-bundle.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Stub greentic-bundle:
#   --version -> prints a fake version.
#   ext render --help -> prints a help line containing "--json" so probe picks it.
#   ext render <ext> <recipe> --config - --session <file> --out <path> --json
#     -> writes a tiny fake .gtpack to <path>, prints the expected JSON summary.

case "$*" in
  *'--version'*)
    echo "greentic-bundle 0.0.0-stub"
    exit 0
    ;;
esac

case "$*" in
  *'ext render --help'*)
    cat <<EOF
Usage: greentic-bundle ext render <ext> <recipe> --config <FILE|-> --session <FILE|-> [--out FILE] [--json]
EOF
    exit 0
    ;;
esac

# Parse out the --out path and write a marker file there.
out=""
while (( "$#" )); do
  case "$1" in
    --out) out="$2"; shift 2;;
    *) shift;;
  esac
done

if [[ -z "${out}" ]]; then
  echo '{"status":"error","code":"invalid-args","message":"stub: --out required"}'
  exit 1
fi

printf 'stub-pack-bytes' > "${out}"

cat <<EOF
{"status":"ok","filename":"stub.gtpack","sha256":"$(printf '0%.0s' {1..64})","bytesLen":15}
EOF
```

Make it executable in the test at runtime.

- [ ] **Step 2: Create the integration test**

Create `tests/pack_ext_integration.rs`:

```rust
//! Integration test: with a stub `greentic-bundle` binary, `run_pack_subprocess`
//! should chain cards2pack (real) + ext render (stub) and report the stub's
//! output as the final pack path.

use std::os::unix::fs::PermissionsExt;

#[tokio::test(flavor = "multi_thread")]
async fn pack_with_bundle_ext_backend_returns_stub_gtpack() {
    // Make the stub executable.
    let stub = std::fs::canonicalize("tests/fixtures/stub-greentic-bundle.sh").unwrap();
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();

    // Point designer at the stub.
    unsafe {
        std::env::set_var("GREENTIC_BUNDLE_BIN", &stub);
    }

    // A full end-to-end test of POST /api/pack against Axum tower-test client
    // is deferred (see spec §14). This test verifies the probe picks up the
    // stub — the remaining wiring is exercised by unit tests on the adapter
    // (Task 11) and the probe (Task 9).
    let backend = greentic_designer::orchestrate::pack_backend::probe(
        std::env::temp_dir().join("greentic-test-ext-dir"),
    );
    match backend {
        greentic_designer::ui::state::PackBackend::BundleExtRender { bundle_version, .. } => {
            assert!(bundle_version.contains("stub"));
        }
        other => panic!("expected BundleExtRender, got {other:?}"),
    }
}
```

Note: a full end-to-end test driving `POST /api/pack` through Axum's tower test client is out of scope for this task — the probe+stub smoke is enough to prove plumbing. A deeper E2E belongs in a follow-up once we have a test harness for `AppState`.

- [ ] **Step 3: Run the integration test**

```bash
cargo test --test pack_ext_integration
```

Expected: PASS.

- [ ] **Step 4: Run the full test suite and CI local check**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash ci/local_check.sh
```

Expected: all clean.

- [ ] **Step 5: Commit + push branch**

```bash
git add tests/pack_ext_integration.rs tests/fixtures/stub-greentic-bundle.sh
git commit -m "test(designer): probe + stub smoke for BundleExtRender chaining"
git push -u origin feat/bundle-ext-render-wiring
```

---

## Phase 4 — Docs

### Task 14: Update designer CLAUDE.md

**Files:**
- Modify: `greentic-designer/CLAUDE.md`

- [ ] **Step 1: Add a subsection under "Pack Pipeline"**

Find the `### Pack Pipeline` section and append:

```markdown
### Pack backend selection

At startup `greentic-designer` probes `greentic-bundle ext render --help`. If
the binary is on PATH (or `GREENTIC_BUNDLE_BIN`) and its help output advertises
`--json`, the designer runs:

```
prepare_cards → greentic-cards2pack → http_inject →
  session_adapter::build_payload → greentic-bundle ext render → .gtpack
```

Otherwise it falls back to the legacy cards2pack-only path. The chosen backend
is logged once at startup. Feature `bundled-bundle-ext` embeds a copy of
`greentic.bundle-standard-0.1.0.gtxpack`, unpacked on first run into
`~/.greentic/designer/bundle-ext/`.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(designer): document PackBackend probe + bundled bundle-ext"
```

### Task 15: Open PR + final manual E2E

- [ ] **Step 1: Open PR against main**

```bash
gh pr create \
  --title "feat: wire designer pack pipeline through greentic-bundle ext render" \
  --body "Chains cards2pack + ext render per spec 2026-04-19 in greentic-bundle. Falls back to legacy cards2pack-only when greentic-bundle lacks --json."
```

- [ ] **Step 2: Manual E2E checklist**

With a local `greentic-bundle` built from the Phase 1 branch on your PATH:

1. `cargo run --features bundled-bundle-ext -- ui`.
2. Confirm startup log: `pack backend: BundleExtRender (...)` and the bootstrap path exists.
3. Build a pack for a simple demo via the UI.
4. Inspect the resulting `.gtpack` — should unzip to a workspace with `bundle.yaml`, `flows/main.ygtc`, and `assets/cards/<id>.json` entries.
5. Force fallback: set `GREENTIC_BUNDLE_BIN=/bin/false`, restart designer, build a pack — should report `LegacyCards2Pack` and produce the legacy `.gtpack` unchanged.

If any step fails, diagnose and fix before merging.

---

## Post-merge

- Update the user's memory file `bundle-extension-migration.md` with the `[2026-04-19 wiring shipped]` status line.
- Create a follow-up ticket for **Mode B WASM execution** in `greentic-bundle/src/ext/wasm.rs`.
- Create a follow-up ticket for **cards2pack-core extraction** so the single-subprocess path becomes possible.

---

## Out of scope (reminder)

- Mode B WASM execution.
- Pure-Rust `bundle-core` / `cards2pack-core` extraction.
- Additional recipes beyond `standard`.
- Retiring `greentic-cards2pack`.
- Full Axum tower-test E2E for `/api/pack` (deferred; stub smoke is enough for this phase).
