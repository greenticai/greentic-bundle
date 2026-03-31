# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Full local validation (i18n, fmt, clippy, test, build, doc, packaging)
bash ci/local_check.sh

# Individual commands
cargo build --all-features
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1 cargo test --all-features

# Run a single test
GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1 cargo test --all-features <test_name>

# Documentation
cargo doc --no-deps --all-features
```

Tests require `GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1` to use the embedded provider catalog instead of fetching from a remote registry.

## Architecture

**greentic-bundle** is a Rust CLI for authoring Greentic bundles — containerized app collections with deterministic, reproducible builds packaged as SquashFS `.gtbundle` artifacts.

### Workspace Layout

- **Root crate** (`greentic-bundle`): CLI + authoring logic
- **`crates/greentic-bundle-reader`**: Read-only typed API for inspecting built `.gtbundle` artifacts and normalized build directories

### Core Subsystems (under `src/`)

- **`cli/`** — Clap-based command router: `wizard`, `build`, `inspect`, `doctor`, `access`, `init`, `add`, `remove`, `export`, `unbundle`
- **`wizard/`** — Interactive staged composition flow (bundle basics → app-pack add/map → extension-providers → access review → build/dry-run/save). The largest module (~4500 LOC)
- **`project/`** — `BundleWorkspaceDefinition` model backed by `bundle.yaml`. Defines app-pack mappings, tenant/team layout, and resolved output generation
- **`catalog/`** — Registry resolution and caching. Supports `file://`, `ghcr://`, and `oci://` catalog URIs. Caches under `state/cache/catalogs/`
- **`build/`** — Deterministic build pipeline: plan computation → normalized state under `state/build/<bundle>/normalized` → SquashFS artifact via `mksquashfs`
- **`access/`** — Gmap rule parsing, evaluation, and mutation for tenant/team access control
- **`answers/`** — Semver-versioned answer documents for replaying wizard sessions
- **`setup/`** — Bridges legacy setup specs and provider QA payloads into normalized `FormSpec` persisted under `state/setup/`
- **`i18n/`** — 66 locales embedded at compile time via `build.rs`. Locale precedence: `--locale` flag → `LC_ALL`/`LC_MESSAGES`/`LANG` → OS locale → `en`

### Key Conventions

- **Dry-run by default**: Most mutations preview changes as deterministic JSON; `--execute` is required for side effects
- **Deterministic output**: All builds, plans, and resolved outputs produce sorted, reproducible JSON/YAML
- **Workspace-local state**: Mutable state lives under `state/` (cache, build artifacts, setup, resolved output)
- **Answer replay**: Wizard outputs are replayable via `wizard apply --answers <FILE>`
- **Offline mode**: `--offline` flag; catalogs are cached for replay from `state/cache/catalogs/`

### Bundle Workspace Paths

- `bundle.yaml` — workspace definition
- `bundle.lock.json` — catalog/app-pack/provider lock material
- `tenants/<tenant>/tenant.gmap` — tenant access rules
- `tenants/<tenant>/teams/<team>/team.gmap` — team access rules
- `resolved/` and `state/resolved/` — generated resolved manifests
- `registries/providers.json` — bundled provider catalog source (OCI registry format)

### i18n

- `i18n-locales.json` is the source-of-truth locale list
- `i18n/en.json` is the source translation; other locale files are seeded copies
- `build.rs` embeds all locale JSON into the binary at compile time
- Validation: `python3 ci/i18n_check.py validate` (key presence, placeholders, backticks)

### Toolchain

Rust 1.94.0 pinned via `rust-toolchain.toml`. Edition 2024. YAML parsing uses `serde_yaml_gtc` (imported as `serde_yaml_bw`).

### Release Flow

Bump version in root `Cargo.toml`, push to `main`. `publish.yml` derives the `vX.Y.Z` tag, publishes workspace crates to crates.io in dependency order, builds cargo-binstall archives, and pushes to GHCR.

## Git Conventions

Do NOT add Claude co-author attribution to commits or PRs.
