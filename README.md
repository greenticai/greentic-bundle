# greentic-bundle

`greentic-bundle` is now a small Rust workspace with a CLI crate and an in-repo read-only reader crate. PR-BUNDLE-01 established the command surface, embedded i18n, answer-document contract, and workspace layout. PR-BUNDLE-02 added the first real wizard orchestration path. PR-BUNDLE-02A replaces that flat form with a staged composition wizard for bundle creation and update: bundle basics, app-pack add/map loops, access-rule review, extension-provider selection, and a final build/dry-run/save-answers review step. PR-BUNDLE-03 added the first real authored-workspace mutation path for tenant/team gmap files and deterministic resolved-output sync. PR-BUNDLE-04 added a single catalog resolution/cache seam, real deterministic `bundle.lock.json` contents, offline replay from the workspace-local cache, and `inspect` output for the current lock state. PR-BUNDLE-05 added setup-spec and provider-QA form bridging plus bundle-owned composition-time setup persistence under `state/setup/`. PR-BUNDLE-06 added the first real build/export pipeline: normalized build state under `state/build/`, deterministic SquashFS `.gtbundle` output, and structured `doctor` / `inspect` reports for both workspaces and artifacts. PR-BUNDLE-06A makes `crates/greentic-bundle-reader` the typed read-only surface for built artifacts and normalized build directories.

Current baseline:

- `clap`-based command surface for `wizard`, `doctor`, `build`, `export`, `inspect`, `add`, `remove`, `access`, and `init`
- embedded `i18n/en.json` catalog with locale normalization and fallback
- semver-based `AnswerDocument` model and migration stub path
- early workspace layout with `crates/greentic-bundle-reader`
- documented future contract that `build` will become the `.gtbundle` SquashFS-producing command
- wizard `run` / `validate` / `apply` flow with deterministic plan output and replayable AnswerDocument JSON
- real `access allow` / `access forbid` mutations for tenant and team gmaps, including preview mode and generated `resolved/...` sync
- workspace-local catalog caching under `state/cache/catalogs`
- deterministic `bundle.lock.json` with catalog/app-pack/provider lock material
- `inspect --root <DIR>` output for the current bundle lock
- setup bridging from legacy setup specs and provider QA payloads into normalized forms
- composition-time setup persistence under `state/setup/*.json`
- real SquashFS `.gtbundle` build output
- normalized build state under `state/build/<bundle>/normalized`
- structured `doctor` and `inspect` reports for workspaces and built artifacts
- real `init`, `add`, and `remove` commands for authored workspace initialization and dependency mutation
- repo toolchain pinned to Rust 1.91 via `rust-toolchain.toml` and `rust-version`
- embedded locale bundle covering the approved 66-locale set from `i18n-locales.json`
- translation validation/status tooling in `ci/i18n_check.py`

## CI and Releases

Local validation uses a single entrypoint:

```bash
bash ci/local_check.sh
```

This runs:

1. `python3 ci/i18n_check.py validate`
2. `python3 ci/i18n_check.py status`
3. `cargo fmt --all -- --check`
4. `cargo clippy --all-targets --all-features -- -D warnings`
5. `cargo test --all-features`
6. `cargo build --all-features`
7. `cargo doc --no-deps --all-features`
8. crates.io packaging checks for every publishable crate in the workspace

For crates with workspace-internal publishable dependencies, local packaging verifies the source tree contract (`Cargo.toml`, `src/`, `README`, `LICENSE`, and runtime asset directories) and defers the real `cargo package` / `cargo publish --dry-run` to the release job after dependency crates have been published in order. This avoids false failures against crates.io for crates that depend on a sibling crate version not yet available in the public index.

GitHub Actions:

- `.github/workflows/ci.yml` runs lint, tests, and package dry-runs on pushes and pull requests.
- `.github/workflows/publish.yml` runs on pushes to `main` / `master` (and manual dispatch), derives `vX.Y.Z` from the primary crate version, skips itself when that version tag already exists, reruns the full local check on Linux, publishes crates to crates.io, builds six `cargo-binstall` archives, creates the GitHub release/tag, and pushes the archives to GHCR as OCI artifacts.
- The publish workflow uses workspace dependency order and performs the real `cargo publish --dry-run` for dependent crates immediately before publish, after earlier workspace crates have already been released.

Release flow:

1. Bump the version in `Cargo.toml`.
2. Commit the change and push it to `main` or `master`.
3. `publish.yml` derives `vX.Y.Z` from `Cargo.toml`, creates that tag/release for the pushed commit, and skips publication if that version already exists.

Required secrets:

- `CARGO_REGISTRY_TOKEN` for crates.io publishing.
- `GHCR_TOKEN` for GHCR publication if `GITHUB_TOKEN` is insufficient for package writes in the target repository settings.

## Workspace Notes

The repo is intentionally a workspace already so the runtime-facing reader can live beside the authoring CLI. `crates/greentic-bundle-reader` now opens built `.gtbundle` SquashFS artifacts and normalized build directories, validates the basic manifest/lock contract, and exposes a stable typed runtime surface for bundle metadata, dependency refs, catalogs, resolved files, and setup-state files without leaking the on-disk layout into consumers. Because the main CLI depends on it at runtime, the reader crate is now a real publishable workspace crate rather than a private placeholder.

Current wizard behavior:

- `greentic-bundle wizard` now behaves like an interactive execute flow by default, so choosing `Create bundle` writes the workspace unless you explicitly use `--dry-run`.
- The create/update wizard is now staged around real composition steps instead of a flat form.
- App packs are mandatory and are added one at a time with immediate `global` / `tenant` / `tenant/team` mapping.
- Mapping decisions are turned into gmap mutations and stored in `bundle.yaml` as `app_pack_mappings`.
- Executed create/apply flows materialize app-pack `.gtpack` files into the bundle layout: `packs/` for global scope, `tenants/<tenant>/packs/` for tenant scope, and `tenants/<tenant>/teams/<team>/packs/` for team scope.
- Pack scope is handled inside the app-pack flow instead of a separate raw access stage; users choose scopes and the wizard generates gmap rules internally.
- Extension providers are added through a loop and are composition-only in this wizard; provider setup is not prompted here.
- Executed create/apply flows materialize extension-provider `.gtpack` files into `providers/<domain>/`.
- `greentic-bundle wizard run --mode update` asks for the current bundle root, loads the existing `bundle.yaml`, shows the current bundle values as editable defaults, and then re-enters the staged composition flow for packs and providers.
- `greentic-bundle wizard run` collects a guided bundle composition interactively unless `--answers` is supplied.
- `--dry-run` computes a deterministic plan and emits answers without writing workspace files.
- `greentic-bundle wizard validate --answers <FILE>` rebuilds the normalized plan without side effects.
- `greentic-bundle wizard apply --answers <FILE>` replays the answers, initializes the workspace through the authored workspace model, and applies the mapped access grants for selected app packs.

Current access behavior:

- `greentic-bundle access allow <RULE>` and `greentic-bundle access forbid <RULE>` target either `tenants/<tenant>/tenant.gmap` or `tenants/<tenant>/teams/<team>/team.gmap`.
- Access commands default to preview mode and print deterministic JSON describing the pending gmap mutation and generated resolved-output writes.
- `--execute` applies the gmap edit and reruns full deterministic resolution for the affected tenant/team, writing `resolved/...` and `state/resolved/...`.
- Dry-run access previews do not create directories or touch workspace files.

Current catalog and lock behavior:

- Wizard execution resolves `remote_catalogs` through a single catalog seam in `src/catalog/`.
- Local `file://...` or workspace-local catalog paths are parsed, digested, and cached into `state/cache/catalogs/`.
- Offline replay works from the workspace-local cache via digest/ref index lookup.
- `bundle.lock.json` now records catalog refs/digests plus authored app-pack and extension-provider refs in deterministic order.
- Remote non-file catalogs are centralized behind one client seam and now support a defined GHCR shorthand:
  `ghcr://catalogs/well-known[:tag|@sha256:...]` maps to `ghcr.io/greenticai/catalogs/well-known[:tag|@sha256:...]` and defaults to `:latest` when no tag/digest is supplied.
- `oci://<registry>/<repo>[:tag|@sha256:...]` is also accepted as the explicit remote format.
- Remote GHCR/OCI catalogs now fetch through `greentic-distributor-client` and are cached back into the bundle workspace cache after resolution.
- The checked-in source for the default bundled provider catalog is `registries/providers.json`, using OCI registry format with top-level `categories` and `items`.
- Pushing changes under `registries/` can publish the provider registry through `.github/workflows/publish-registry.yml` to `ghcr.io/<repo>/providers:latest`.
- The repo now declares Rust 1.91 as its required toolchain, which aligns it with newer Greentic crate releases and supports the live distributor-client integration.

Current i18n behavior:

- `i18n-locales.json` is the source-of-truth language list for the embedded locale bundle.
- Locale selection precedence is `--locale`, then `LC_ALL` / `LC_MESSAGES` / `LANG`, then OS locale via `sys-locale`, then `en`.
- Locale normalization now accepts forms like `en_US.UTF-8`, `en_US`, `en-US`, and `de_DE@euro`.
- All locale JSON files are compiled into the binary by `build.rs`; there is no runtime translation install step.
- `tools/i18n.sh` is the repo entrypoint for `translate`, `validate`, `status`, and `all`.
- `tools/i18n.sh translate` now passes an explicit `--batch-size`, controlled by `BATCH_SIZE` and defaulting to `200`.
- `ci/i18n_check.py validate` enforces key presence plus placeholder/newline/backtick invariants.
- `ci/i18n_check.py status` reports missing/stale key counts per locale and now runs in local checks and CI.
- The non-English locale files are currently seeded from `en.json` so the repo has a complete embedded bundle and passing validation without depending on translator auth during development. Real translated content still needs a later authenticated translation-generation pass.

Current setup behavior:

- Setup normalization/persistence still exists for replayed answer flows, but provider setup is now intentionally absent from the create/update composition wizard.
- Catalog items may still carry inline setup metadata:
  `{"setup":{"type":"legacy","spec":{...}}}` or `{"setup":{"type":"provider_qa","qa_output":{...},"i18n":{...}}}`.
- Setup specs are normalized from either legacy setup documents or provider QA payloads into a bundle-local `FormSpec` shape.
- Execute mode writes deterministic setup state JSON to `state/setup/<provider>.json`.
- Dry-run and validate paths preview the same setup writes without persisting files.
- Setup persistence is bundle-owned and does not write to operator runtime or secrets-store paths.

Current build/export behavior:

- `build` computes a canonical build state from `bundle.yaml`, `bundle.lock.json`, resolved files, and setup state.
- The normalized build state is written under `state/build/<bundle>/normalized`.
- `build` materializes a deterministic SquashFS `.gtbundle` artifact using `mksquashfs`, defaulting to `dist/<bundle>.gtbundle` inside the bundle root.
- `export` can rematerialize a `.gtbundle` from a normalized build directory, with `--dry-run` support.
- `inspect` and `doctor` accept either a workspace root or a built artifact and emit stable JSON.
- Artifact-side inspection and validation now flow through `crates/greentic-bundle-reader` rather than duplicating `unsquashfs` parsing in the main crate.
- Artifact/runtime metadata now carries `hooks`, `subscriptions`, and `capabilities` from `bundle.yaml` into the built manifest and reader surface.
- Generated resolved manifests are now richer: they carry bundle metadata, catalog refs, authored extension providers, and per-target app-pack policy summaries.

Current authored mutation behavior:

- `init` previews or creates a starter workspace with `bundle.yaml`, `bundle.lock.json`, tenant layout, and generated resolved outputs.
- `add app-pack` / `add extension-provider` update `bundle.yaml`, keep `bundle.lock.json` aligned, and rerender generated resolved outputs when executed.
- `remove app-pack` / `remove extension-provider` do the inverse with the same preview-first JSON contract.

Key planned paths:

- authored workspace root: `bundle.yaml`
- future lock file: `bundle.lock.json`
- tenant gmaps: `tenants/<tenant>/tenant.gmap`
- team gmaps: `tenants/<tenant>/teams/<team>/team.gmap`
- generated resolved output: `resolved/...` and `state/resolved/...`
- future setup-derived state: `state/setup/`
