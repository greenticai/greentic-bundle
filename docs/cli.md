# CLI Baseline

`greentic-bundle` now has the PR-06A baseline in place. The crate is still intentionally narrow, but it now has a real build/export loop as well as setup, wizard, access, lock handling, and a typed reader surface:

- interactive `wizard run` for staged bundle composition
- guided create/update menus for bundle basics, app packs with scope mapping, and extension providers
- deterministic plan generation
- `--emit-answers` output using `AnswerDocument`
- `wizard validate` as a side-effect-free replay path
- `wizard apply` as a real starter-workspace replay path
- `access allow` / `access forbid` as deterministic tenant/team gmap mutation paths
- `inspect` as deterministic `bundle.lock.json` output
- setup-form bridging and `state/setup/*.json` persistence during wizard execution
- `build` as deterministic SquashFS `.gtbundle` production
- `export` as deterministic artifact materialization from normalized build state
- `doctor` / `inspect` as structured JSON for workspaces and artifacts
- `init`, `add`, and `remove` as real authored workspace mutation commands
- `crates/greentic-bundle-reader` as the read-only typed API for `.gtbundle` artifacts and normalized build directories

Current command shape:

```text
greentic-bundle
greentic-bundle wizard
greentic-bundle wizard run
greentic-bundle wizard validate
greentic-bundle wizard apply
greentic-bundle doctor
greentic-bundle build
greentic-bundle export
greentic-bundle inspect
greentic-bundle add app-pack
greentic-bundle add extension-provider
greentic-bundle remove app-pack
greentic-bundle remove extension-provider
greentic-bundle access allow
greentic-bundle access forbid
greentic-bundle init
```

Implemented baseline decisions:

- `clap` drives parsing and help output.
- Embedded i18n is loaded from `i18n/en.json`.
- Locale normalization and fallback already exist even though only `en.json` ships today.
- `AnswerDocument` uses semver-based `schema_version`.
- The repo starts as a small workspace so the read-only bundle reader crate can live under `crates/`.
- The repo is now pinned to Rust 1.91 via `rust-toolchain.toml` and crate-level `rust-version`.
- The embedded locale bundle is now generated from `i18n/locales.json` and compiled into the binary by `build.rs`.
- `build` is reserved as the future artifact-producing command for deterministic `.gtbundle` SquashFS output.
- `wizard run`, `wizard validate`, and `wizard apply` share a normalized request model and deterministic plan envelope.
- Bare `greentic-bundle wizard` now executes by default; use `--dry-run` when you only want the plan preview.
- `wizard run --mode update` asks for the existing bundle root first, then re-prompts the editable bundle fields with the current workspace values as defaults before re-entering the staged composition flow.
- The create/update wizard requires at least one app pack, adds packs one at a time, maps each pack immediately to `global`, `tenant`, or `tenant/team`, and turns those mappings into gmap mutations plus `app_pack_mappings` in `bundle.yaml`.
- The normal access step is now expressed as pack scope editing (`change scope`, `add tenant access`, `add tenant/team access`, `remove scope`) instead of raw rule editing.
- Executed wizard flows now materialize app packs into `packs/`, `tenants/<tenant>/packs/`, or `tenants/<tenant>/teams/<team>/packs/` based on the selected mapping.
- Extension providers are added through a guided loop and are treated as composition-only in this wizard; provider setup is intentionally absent from the interactive create/update flow.
- Executed wizard flows also materialize extension providers into `providers/<domain>/`.
- `wizard apply` initializes the workspace through the authored workspace model, writes `bundle.lock.json`, and applies the mapped access grants for selected app packs.
- `access` mutations use `bundle.yaml`-rooted workspace layout and sync generated `resolved/...` plus `state/resolved/...` manifests.
- Wizard execution resolves `remote_catalogs` through one catalog/cache seam and writes a real `bundle.lock.json`.
- Remote catalog refs now have a defined bundle-side contract:
  `ghcr://catalogs/well-known[:tag|@sha256:...]` maps to `ghcr.io/greenticai/catalogs/well-known[:tag|@sha256:...]`, and `oci://...` keeps the raw OCI form.
- The repo now targets Rust 1.91, and uncached remote GHCR/OCI catalogs fetch through `greentic-distributor-client` before being written into the workspace-local cache.
- The checked-in source for the default public catalog is `packs/well-known.json`, stored as a top-level list of category objects with per-category `items`.
- `.github/workflows/catalog.yml` publishes that file to `ghcr.io/greenticai/catalogs/well-known` on pushes to `main` or `master`.
- The catalog workflow uses `GITHUB_TOKEN` plus OCI source annotations so the GHCR package is linked to this repository; package visibility still must be switched to `Public` once in GitHub if anonymous pulls are expected.
- `.github/workflows/publish.yml` now triggers from pushes to `main` or `master`, derives `vX.Y.Z` from the primary crate version, creates the release tag itself, and skips publication when that version tag already exists.
- Replay/apply execution can still normalize setup specs/answers and persist composition-time setup state under `state/setup/`.
- The interactive create/update flow is now a staged composition UX rather than the old flat root form.
- Locale selection precedence is now `--locale`, then `LC_ALL` / `LC_MESSAGES` / `LANG`, then OS locale via `sys-locale`, then `en`.
- `build` writes normalized metadata under `state/build/<bundle>/normalized` before materializing the final `.gtbundle` into `dist/<bundle>.gtbundle` by default.
- The normalized build directory now carries the materialized `packs/` and `providers/` trees forward into the final `.gtbundle`.
- `init` writes a starter workspace scaffold and lock.
- `add` / `remove` mutate authored app-pack and extension-provider refs in `bundle.yaml` and keep `bundle.lock.json` aligned.

Current authored/generated path contract:

- Authored root: `bundle.yaml`
- Future lock file: `bundle.lock.json`
- Tenant gmaps: `tenants/<tenant>/tenant.gmap`
- Team gmaps: `tenants/<tenant>/teams/<team>/team.gmap`
- Generated resolved outputs: `resolved/...` and `state/resolved/...`
- Future setup state: `state/setup/`

Current access semantics:

- Access commands are preview-first; they print deterministic JSON unless `--execute` is supplied.
- `access allow pack/flow` writes `public` policy entries.
- `access forbid pack/flow/node` writes `forbidden` policy entries.
- Tenant/team resolution currently uses a full deterministic rerender rather than an incremental update path.

Current catalog/lock semantics:

- Cache policy is explicitly `workspace-local`.
- Cached catalogs live under `state/cache/catalogs/` with `by-ref`, `by-digest`, and `index.json`.
- `bundle.lock.json` is now the public root lock contract and records catalog refs/digests, app-pack refs, extension-provider refs, tool version, cache policy, and lock/build format markers.
- Bare non-file catalog refs are treated as remote refs. They resolve from the workspace cache in offline mode, and otherwise flow through one remote-client seam.
- Remote GHCR/OCI refs now fetch through `greentic-distributor-client` and are then cached back into the workspace-local catalog cache.

Current setup semantics:

- Setup persistence is bundle-owned, not operator-runtime-owned.
- Two setup input shapes are supported for normalization:
  `legacy` setup specs and `provider_qa` payloads.
- Catalog entries can also embed setup metadata inline under `setup`, using the same tagged JSON shape.
- When `setup_execution_intent` is enabled, wizard replay auto-discovers missing setup specs from matching catalog items for selected `extension_providers` or `app_packs`.
- Interactive wizard execution uses `greentic-qa-lib` for both the root bundle request and any missing setup answers before applying the workspace.
- Secret questions use no-echo terminal input when stdin/stdout are attached to a TTY.
- Persisted setup state is JSON under `state/setup/<provider>.json`.
- Persisted state separates `normalized_answers`, `non_secret_config`, and `secret_values`.
- Dry-run setup preview reports the same writes without touching the filesystem.

Current build/export semantics:

- `build` is the primary artifact-producing command.
- `.gtbundle` is now a real SquashFS artifact produced via `mksquashfs`.
- `export` consumes a normalized build directory rather than rebuilding the workspace itself.
- `doctor` validates either a workspace or an artifact.
- `inspect` reads either a workspace or an artifact and always emits stable JSON.
- Artifact reads are performed via `greentic-bundle-reader`, which hides the SquashFS/build-dir file layout from callers and exposes typed bundle metadata, dependency refs, catalog refs, and generated file views.
- The manifest and reader surface now also carry `hooks`, `subscriptions`, and `capabilities` from `bundle.yaml`.
- The manifest and reader surface now also carry structured resolved-target summaries so runtime consumers can see tenant/team targets and per-target app-pack policy summaries without reparsing the YAML files themselves.
- Workspace packaging checks run full `cargo package` / `cargo publish --dry-run` locally for independent crates; when a crate depends on another publishable workspace crate, local checks verify the source tree contract and defer the real package/dry-run to the release workflow after dependency crates are live on crates.io.
- `ci/i18n_check.py validate` and `ci/i18n_check.py status` now run before Rust formatting/lint/test checks and are also enforced in GitHub CI.
- `tools/i18n.sh` mirrors the Greentic component-style entrypoint and routes `translate` through `greentic-i18n-translator` while using the repo-local validator/status checks for day-to-day validation.
- The approved 66-locale set exists in-repo today, but non-English files are currently seeded from `en.json` until a later authenticated translation-generation pass replaces them with real translated text.
