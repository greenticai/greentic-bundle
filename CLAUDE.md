# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust CLI for authoring, scaffolding, and managing Greentic bundles. Provides an interactive wizard (with answer-document contracts), project initialization, pack resolution from OCI registries, build/export tooling, and access control. All CLI help text is localized via embedded i18n (66 locale files).

- **Edition:** Rust 2024 | **MSRV:** 1.91
- **Workspace version** is defined once in root `Cargo.toml` `[workspace.package]`
- **Workspace member:** `crates/greentic-bundle-reader` (bundle parsing library)

## Build, Test, and Lint Commands

```bash
# Full local CI (all steps sequentially -- run before any PR)
./ci/local_check.sh

# Build
cargo build
cargo build --workspace

# Test
cargo test --workspace

# Format and lint
cargo fmt --all --check
cargo clippy --workspace -- -D warnings

# Validate i18n locale files (all locales must have identical keys)
python3 ci/i18n_check.py
```

## CLI Subcommands

The binary is `greentic-bundle` (also embedded as `gtc wizard` via `greentic/greentic`).

| Subcommand | Purpose |
|------------|---------|
| `wizard` | Interactive bundle creation wizard (subcommands: `run`, `validate`, `apply`) |
| `wizard run` | Run wizard interactively or from `--answers` file; `--dry-run` for preview |
| `wizard validate` | Validate an answers document without applying |
| `wizard apply` | Apply a validated answers document to scaffold/update a bundle |
| `doctor` | Diagnose bundle health, validate structure and pack integrity |
| `build` | Build bundle artifact (`.gtbundle` archive) |
| `export` | Export a build directory to a portable artifact |
| `inspect` | Inspect a bundle workspace or `.gtbundle` artifact |
| `unbundle` | Extract a `.gtbundle` artifact to disk |
| `init` | Initialize a new bundle workspace (`bundle.yaml`) |
| `add` | Add a pack or provider to the bundle |
| `remove` | Remove a pack or provider from the bundle |
| `access` | Manage tenant access control (subcommands: `allow`, etc.) |

Global flags: `--locale <LOCALE>`, `--offline`

## Architecture

### Directory Layout

- `src/cli/` -- Clap argument definitions and dispatch for each subcommand
- `src/wizard/` -- Wizard engine: interactive prompts, answer-document I/O, execution modes
- `src/project/` -- Bundle workspace definition (`bundle.yaml`), scaffold, pack resolution, asset scaffolding
- `src/build/` -- Artifact creation, export, unbundle, inspect, demo export
- `src/catalog/` -- OCI registry resolution via `greentic-distributor-client`
- `src/access/` -- Tenant access control and permission management
- `src/answers/` -- Answer-document schema, migration, serialization
- `src/setup/` -- Provider setup orchestration
- `src/i18n/` -- Locale detection, translation lookup (`tr()`)
- `i18n/` -- 66 JSON locale files (en, es, de, fr, ja, zh, ar variants, etc.)
- `ci/` -- `local_check.sh`, `i18n_check.py` (key parity), `workspace_publish.py`
- `crates/greentic-bundle-reader/` -- Standalone bundle parsing library
- `registries/` -- Embedded OCI registry metadata
- `tests/` -- Integration tests using `assert_cmd` + `predicates`

### Wizard Engine

The wizard supports three execution modes:

1. **Interactive** -- TTY prompts with localized text
2. **Answers file** -- `--answers answers.json` for headless/CI execution
3. **Dry run** -- `--dry-run` shows the plan without applying changes

Wizard subcommands (`run`, `validate`, `apply`) share `WizardRunArgs`/`WizardValidateArgs`/`WizardApplyArgs` with common flags: `--answers`, `--emit-answers`, `--schema-version`, `--migrate`, `--mode`.

Answer documents are versioned and support migration via `migrate_document()`.

### Bundle Assets Capability System

Packs can declare they need access to bundle-level assets. The wizard and project module manage four capabilities:

| Constant | Value |
|----------|-------|
| `CAP_BUNDLE_ASSETS_READ_V1` | `greentic.cap.bundle_assets.read.v1` |
| `CAP_WEBCHAT_OAUTH_V1` | `greentic.cap.webchat.oauth.v1` |
| `CAP_WEBCHAT_I18N_V1` | `greentic.cap.webchat.i18n.v1` |
| `CAP_WEBCHAT_EMBED_V1` | `greentic.cap.webchat.embed.v1` |

Key functions in `src/project/mod.rs`:

- **`scaffold_assets_from_packs()`** -- Copies skin `default/` to `skins/{tenant}/`, generates embed snippets when `CAP_WEBCHAT_EMBED_V1` is active
- **OAuth stripping** -- When `CAP_WEBCHAT_OAUTH_V1` is absent, the auth section is removed from tenant config

Key function in `src/wizard/mod.rs`:

- **`edit_bundle_capabilities()`** -- Prompts user to toggle each of the 4 capabilities during wizard flow

Assets use the generic `./assets/` capability pattern -- packs must NOT hardcode specific asset paths.

### Catalog / OCI Resolution

`src/catalog/` resolves pack references from OCI registries using `greentic-distributor-client`. The default provider registry is `oci://ghcr.io/greenticai/greentic-bundle/providers:latest`.

### i18n

- 66 locale files in `i18n/` (JSON key-value format)
- All locales must have identical key sets -- enforced by `ci/i18n_check.py`
- CLI help text keys follow `cli.<subcommand>.<field>` convention
- Locale auto-detected from system via `sys-locale`, overridable with `--locale`
- Build step (`build.rs`) compiles `i18n-locales.json` listing available locales

## Key Dependencies

- `clap` -- CLI argument parsing with derive macros
- `greentic-distributor-client` -- OCI pack fetching and resolution
- `greentic-qa-lib` -- Wizard driver, QA spec, i18n config
- `serde_yaml_bw` (alias `serde_yaml_gtc`) -- YAML parsing (Greentic fork)
- `zip` -- `.gtbundle` / `.gtpack` archive handling
- `tokio` -- Async runtime for OCI registry operations

## Git Conventions

- Use conventional commit format: `feat:`, `fix:`, `docs:`, `chore:`, etc.
- Do NOT add `Co-Authored-By: Claude` or AI attribution in commits/PRs
- Always use feature branches, never commit directly to main/master
