# CLAUDE.md — greentic-bundle

Bundle composer and wizard CLI producing signed `.gtbundle` SquashFS deployments
from packs, tenant/team config, and access maps. Part of the `gtc` delegation
chain (`gtc bundle` routes here).

For wizard-replay semantics, cross-repo ownership rules, and agent documentation
conventions see [docs/coding-agents.md](docs/coding-agents.md).

## Workspace Members

| Crate | Purpose |
|-------|---------|
| `greentic-bundle` (root) | CLI + library: wizard, build, setup, catalog, access, i18n |
| `greentic-bundle-reader` | Read-only `.gtbundle` consumer (used by runner/start) |
| `bundle-standard-core` | Shared standard-bundle types |
| `cards2pack-core` | Adaptive Card JSON to pack conversion core |

Workspace version: `1.1.0-dev.0`. Edition 2024, `rust-version = "1.91"`.
Toolchain pinned to 1.95.0 via `rust-toolchain.toml` (managed centrally).

## Source Layout (`src/`)

| Module | What it does |
|--------|-------------|
| `access/` | Access-map (`.gmap`) authoring and validation |
| `answers/` | Answer-document model (`AnswerDocument`), migration, persistence. Schema-version 2: `secret_refs` replace inline secrets |
| `build/` | Bundle build pipeline: `plan.rs` (build plan), `manifest.rs`, `lock.rs` (pack-list lock), `signing.rs` (DSSE+Ed25519 `.gtbundle` signing), `doctor_secrets.rs` (secret-leak scanner), `export.rs`, `squashfs.rs`, `warmup.rs` |
| `bundle_fs/` | SquashFS read/write: `backhand_writer.rs`, `native_mksquashfs_writer.rs`, `native_unsquashfs_reader.rs` — symlink-TOCTOU-hardened |
| `catalog/` | Pack catalog resolution and indexing |
| `cli/` | Clap CLI: `add`, `build`, `doctor`, `export`, `info/`, `init`, `inspect`, `remove`, `unbundle`, `wizard` |
| `i18n/` | Embedded i18n facade (50+ locales via `i18n-locales.json`) |
| `project/` | Bundle project model (on-disk layout, metadata) |
| `runtime.rs` | Global runtime flags (offline mode, refresh mode) |
| `setup/` | Setup-state bridge: QA backend, legacy FormSpec, persistence |
| `wizard/` | Interactive bundle-creation wizard with i18n prompts |

## Build and Test

```bash
cargo build                                            # build root crate
cargo test --workspace                                 # all workspace tests
cargo clippy --workspace --all-targets -- -D warnings  # lint
cargo fmt --all -- --check                             # format check
bash ci/local_check.sh                                 # full local CI gate
```

`ci/` also contains `i18n_check.py` (locale coverage) and `workspace_publish.py`
(ordered crate publish).

## Key Dependencies and Invariants

- **greentic-secrets-spec**: Owns `SecretRef` (the `secret://` URI newtype).
  Depend on it directly — do **not** reach `SecretRef` through
  `greentic-deploy-spec`'s re-export: `greentic-deployer` depends on
  `greentic-bundle`, so that route reintroduces a publish cycle.
- **serde_yaml_gtc** (imported as `serde_yaml_bw`): Hardened YAML fork. Never
  use upstream `serde_yaml`.
- **DSSE signing** (`build/signing.rs`): Ed25519 DSSE envelopes (in-toto
  Statement v1, SHA-256 subject digest) written as `<artifact>.sig` sidecars.
- **Secret refs**: Answer documents at schema-version 2 carry
  `secret://<env>/<bundle>/<provider>/<question>` URI references instead of
  inline secret values. The `doctor_secrets` scanner gates builds against
  plaintext leaks.
- **SquashFS safety**: `bundle_fs/` writers validate symlink targets against
  TOCTOU races; paths are containment-checked before write.
- **greentic-distributor-client**: Pack fetching and cache management (features
  `dist-client`, `pack-fetch`).
- **Answer-document env scoping**: `AnswerDocument.env_id` binds answers to an
  environment (C7); migration logic in `answers/migrate.rs`.

## CLI Quick Reference

```bash
greentic-bundle --help          # top-level
greentic-bundle wizard --help   # interactive bundle creation
greentic-bundle build --help    # build .gtbundle from project
greentic-bundle doctor --help   # diagnose bundle health
greentic-bundle inspect --help  # inspect .gtbundle contents
```
