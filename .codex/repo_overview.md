# Repository Overview

## 1. High-Level Purpose
This repository is a Rust workspace for `greentic-bundle`, a future bundle-authoring tool. After the current PR-BUNDLE-06A follow-on work and the staged wizard redesign in PR-BUNDLE-02A/02B, the repo now provides real baseline authoring paths for guided bundle composition, access mutation, workspace initialization, authored dependency mutation, artifact build/export, structured inspection, and a separate read-only bundle reader crate rather than only CLI scaffolding: the wizard can collect bundle basics, require at least one app pack, add packs one at a time, map them immediately to global or tenant scopes, generate the resulting access rules internally, add extension providers, replay answers from `AnswerDocument` JSON, emit normalized answers, build a deterministic plan envelope, validate replayed input without side effects, and apply a starter workspace layout through the same authored workspace model used by `init`; `init`, `add`, and `remove` can initialize or mutate `bundle.yaml` plus aligned lock/resolved outputs; the access commands can mutate tenant/team gmap files and rerender generated resolved outputs; `build`, `export`, `doctor`, and `inspect` operate on either workspaces or built artifacts; and `crates/greentic-bundle-reader` can open built `.gtbundle` artifacts and normalized build directories behind one typed API.

The codebase is still intentionally early-stage. Remote catalog fetching now has a defined GHCR/OCI reference contract, the repo itself is now pinned to Rust 1.91, and the artifact contents are still intentionally incremental rather than final. Its current responsibilities are the authoring CLI surface, locale-aware help/prompt rendering, replayable answer schema handling, staged wizard composition, deterministic gmap mutation, starter workspace/resolved-file generation, deterministic lock generation, workspace-local catalog caching, setup-form normalization for replay flows, normalized build-state generation, deterministic SquashFS artifact creation, and repository CI/release scaffolding.

## 2. Main Components and Functionality
- **Path:** `Cargo.toml`
  - **Role:** Root workspace and publishable CLI package manifest.
  - **Key functionality:**
    - Declares the workspace root crate plus `crates/greentic-bundle-reader`.
    - Pins the root crate to `rust-version = "1.91"`.
    - Adds runtime dependencies for `clap`, `serde`, `serde_json`, `semver`, `sha2`, `serde_yaml_bw`, and `anyhow`.
    - Adds integration-test dependencies including `assert_cmd`, `predicates`, and `tempfile`.
    - Includes docs, tests, i18n assets, and CI helpers in the published crate package.
  - **Key dependencies / integration points:**
    - Drives workspace package detection for `ci/workspace_publish.py`.
    - Depends on the in-repo `greentic-bundle-reader` crate for artifact/build-directory inspection.
    - Keeps the reader crate in the workspace as a versioned runtime dependency of the CLI package.

- **Path:** `rust-toolchain.toml`
  - **Role:** Repo-wide Rust toolchain pin.
  - **Key functionality:**
    - Pins local and CI development to Rust `1.91.0`.
    - Requests `rustfmt` and `clippy` alongside the toolchain.
  - **Key dependencies / integration points:**
    - Used by local `cargo`/`rustup` resolution and by GitHub Actions jobs that install a generic stable toolchain.

- **Path:** `src/lib.rs` and `src/main.rs`
  - **Role:** Root library and binary entrypoints.
  - **Key functionality:**
    - Expose the CLI, i18n, answers, wizard, and future feature modules.
    - Route the binary through the shared `main_entry()` path so tests and the CLI exercise the same code.
  - **Key dependencies / integration points:**
    - Used by all command handling and integration tests.

- **Path:** `src/cli/mod.rs`
  - **Role:** Top-level `clap` command tree and help localization hook.
  - **Key functionality:**
    - Defines the root commands `wizard`, `doctor`, `build`, `export`, `inspect`, `add`, `remove`, `access`, and `init`.
    - Wires the required baseline flags: `--locale`, `--offline`, `--answers`, `--emit-answers`, `--schema-version`, `--migrate`, `--dry-run`, and `--execute` where applicable.
    - Rewrites `clap` help/about/arg strings through the embedded i18n catalog before parsing.
  - **Key dependencies / integration points:**
    - Depends on `src/i18n/mod.rs` for translation lookups.

- **Path:** `src/cli/wizard.rs`
  - **Role:** Wizard CLI argument definitions and dispatch surface.
  - **Key functionality:**
    - Defines `wizard run`, `wizard validate`, and `wizard apply`.
    - Bare `greentic-bundle wizard` now executes the interactive flow instead of forcing dry-run preview.
    - Adds replay-related options and a `--mode` enum for `create`, `update`, and `doctor`.
    - Routes command execution into the shared wizard engine in `src/wizard/mod.rs`.
  - **Key dependencies / integration points:**
    - Thin adapter over the actual orchestration logic in `src/wizard/mod.rs`.

- **Path:** `src/cli/inspect.rs`
  - **Role:** Deterministic workspace/artifact inspection command.
  - **Key functionality:**
    - Reads either a workspace root or a built `.gtbundle` artifact.
    - Re-emits a structured manifest/lock report as pretty JSON so inspection is stable and machine-readable.
  - **Key dependencies / integration points:**
    - Uses `src/build/mod.rs`, which now delegates artifact reads to `crates/greentic-bundle-reader`.

- **Path:** `src/cli/build.rs`, `src/cli/export.rs`, and `src/cli/doctor.rs`
  - **Role:** Artifact build/export/validation CLI entrypoints.
  - **Key functionality:**
    - `build` computes normalized build state and materializes a deterministic `.gtbundle`, defaulting to `dist/<bundle>.gtbundle` inside the bundle root.
    - `export` rematerializes a `.gtbundle` from a normalized build directory.
    - `doctor` validates either a workspace or a built artifact and emits structured JSON.
  - **Key dependencies / integration points:**
    - Use `src/build/mod.rs`, which shares the reader crate for artifact-side validation.

- **Path:** `src/cli/init.rs`, `src/cli/add.rs`, and `src/cli/remove.rs`
  - **Role:** Authored workspace initialization and dependency-mutation entrypoints.
  - **Key functionality:**
    - `init` previews or creates a starter workspace rooted at `bundle.yaml`, writes `bundle.lock.json`, ensures tenant layout, and syncs generated resolved outputs.
    - `add` / `remove` mutate authored `app_packs` and `extension_providers` in `bundle.yaml`.
    - Keep `bundle.lock.json` aligned with authored dependency refs and rerender generated resolved outputs on execute.
    - Default to preview-first JSON output unless `--execute` is supplied.
  - **Key dependencies / integration points:**
    - Use `src/project/mod.rs` bundle-workspace read/write helpers.
    - Exercised by `tests/workspace_cli.rs`.

- **Path:** `src/wizard/mod.rs`
  - **Role:** Wizard orchestration core with deterministic lock generation and setup persistence hooks.
  - **Key functionality:**
    - Defines the normalized request model, execution mode, deterministic plan envelope, plan metadata, and ordered step kinds.
    - Renders a compact numbered `Bundle Wizard` main menu with create, open-existing, validate, and doctor entry points.
    - The create/update flow is staged around bundle basics, loop-based app-pack management, immediate app-pack mapping, pack-scope editing, extension-provider selection, and a final review/build step instead of the old flat field form.
    - Requires at least one app pack before the create flow can continue.
    - Auto-detects app-pack and custom extension-provider references as local path, `file://`, `oci://`, `repo://`, or `store://` without prompting for source type first.
    - Persists app-pack mapping decisions as `app_pack_mappings` in `bundle.yaml` and turns them into gmap mutations during apply, while hiding raw gmap rule-path mechanics from the normal wizard flow.
    - Derives internal app-pack access rules from stable pack ids rather than raw source paths, so local absolute-path app packs still render the correct resolved public policy after mapping.
    - Materializes app-pack `.gtpack` files into the bundle-local on-disk layout on execute/apply: `packs/*.gtpack` for global scope, `tenants/<tenant>/packs/*.gtpack` for tenant scope, and `tenants/<tenant>/teams/<team>/packs/*.gtpack` for team scope.
    - Treats extension providers as composition-only in the interactive create/update wizard; provider setup is not prompted there.
    - Materializes extension-provider `.gtpack` files into `providers/<domain>/*.gtpack` on execute/apply when the reference can be resolved locally or through the distributor client.
    - `update` mode now asks for the current bundle root first, loads the existing workspace definition, and re-prompts the editable bundle fields with the current values as defaults before re-entering the staged composition flow.
    - Loads `AnswerDocument` JSON, supports migration of legacy metadata-light payloads when `--migrate` is supplied, and normalizes replayed input into a stable internal request.
    - Resolves `remote_catalogs` through the bundle-local catalog seam, using workspace-local cache writes on execute and cache/offline replay during dry-run or validation when available.
    - Replay/apply still supports setup normalization and persistence when setup metadata is present in answers, but the interactive composition flow no longer prompts provider setup.
    - Implements the run/validate/apply split:
      - `run` collects prompts or replays answers and executes or dry-runs based on `--dry-run`
      - `validate` rebuilds the normalized plan without side effects
      - `apply` replays answers and writes a starter workspace
    - Emits deterministic JSON plan output to stdout and writes normalized `AnswerDocument` files when `--emit-answers` is used.
    - On execute/apply, initializes the workspace through `src/project/mod.rs`, applies default-tenant access grants for selected app packs, writes `bundle.lock.json`, rerenders generated resolved files, and persists any required workspace-local catalog cache files plus setup state JSON under `state/setup/`.
    - When the interactive review action is `Build bundle`, the wizard now immediately runs the build pipeline and writes the final SquashFS artifact to `dist/<bundle>.gtbundle` inside the bundle root.
  - **Key dependencies / integration points:**
    - Uses `src/answers/document.rs` and `src/answers/migrate.rs`.
    - Uses `src/project/mod.rs` for lock writing and workspace sync.
    - Uses `src/catalog/resolve.rs` for catalog resolution and cache behavior.
    - Covered heavily by `tests/wizard_flow.rs`.

- **Path:** `src/i18n/mod.rs` and `i18n/en.json`
  - **Role:** Embedded locale catalog loader and prompt/help translation layer.
  - **Key functionality:**
    - Uses `build.rs` plus `i18n/locales.json` to compile the approved locale set into the binary.
    - Normalizes locale tags with `unic-langid`, supports fallback (`exact -> language -> en`), and resolves locale with the precedence `--locale`, environment, OS locale, then `en`.
    - Provides both `tr` and formatted `trf` lookups for root help output, wizard subcommand help, interactive wizard prompts, plan step descriptions, and answer-document errors.
  - **Key dependencies / integration points:**
    - Used by the CLI and wizard engine.
    - Validated by unit tests and CLI/integration tests.

- **Path:** `build.rs`, `i18n/locales.json`, and `ci/i18n_check.py`
  - **Role:** Locale-bundle generation and translation validation tooling.
  - **Key functionality:**
    - Treats `i18n/locales.json` as the source-of-truth approved language list.
    - Generates a Rust source file at build time that embeds every locale JSON file into the CLI binary.
    - Validates locale presence, missing/stale keys, placeholder counts, newline counts, and backtick spans.
    - Reports per-locale key status for local and CI validation.
  - **Key dependencies / integration points:**
    - Run by `build.rs`, `ci/local_check.sh`, and `.github/workflows/ci.yml`.

- **Path:** `tools/i18n.sh`
  - **Role:** Repo i18n operator entrypoint.
  - **Key functionality:**
    - Mirrors the Greentic component-style `translate` / `validate` / `status` / `all` command surface.
    - Runs `translate` through the sibling `greentic-i18n-translator` manifest by default.
    - Passes an explicit translation batch size via `BATCH_SIZE`, defaulting to `200`.
    - Routes `validate` and `status` through the repo-local `ci/i18n_check.py` checks so daily validation matches CI behavior.
  - **Key dependencies / integration points:**
    - Intended for developers managing locale files in this repo.

- **Path:** `src/answers/document.rs` and `src/answers/migrate.rs`
  - **Role:** Replayable answer-document schema and migration hook.
  - **Key functionality:**
    - Defines `AnswerDocument` with semver `schema_version`, locale, answers, and locks.
    - Uses `BTreeMap` so serialized output remains deterministic.
    - Validates required metadata and supports schema-version advancement while rejecting downgrades.
  - **Key dependencies / integration points:**
    - Used directly by the wizard engine for load/migrate/emit behavior.

- **Path:** `src/access/mod.rs`, `src/access/parse.rs`, `src/access/edit.rs`, `src/access/eval.rs`, `src/access/gmap.rs`
  - **Role:** PR-BUNDLE-03 access-rule parsing, evaluation, and workspace mutation layer.
  - **Key functionality:**
    - Parse gmap rule lines such as `_ = forbidden` and `pack/flow = public` into typed rule structures.
    - Upsert tenant/team policies with canonical ordering, while preserving comments and blank lines when a source file already contains them.
    - Evaluate matching policy decisions by specificity and last-write-wins semantics, including team-over-tenant overlay behavior.
    - Apply `access allow` / `access forbid` mutations in preview mode or execute mode and return deterministic JSON describing intended writes.
  - **Key dependencies / integration points:**
    - Uses `src/project/mod.rs` for workspace layout and resolved-output sync.
    - Exercised by `tests/access_eval.rs` and `tests/access_workspace.rs`.

- **Path:** `src/catalog/mod.rs`, `src/catalog/cache.rs`, `src/catalog/client.rs`, `src/catalog/registry.rs`, `src/catalog/resolve.rs`
  - **Role:** PR-BUNDLE-04 catalog/distributor seam, workspace-local cache policy, and deterministic lock inputs.
  - **Key functionality:**
    - Defines one catalog resolver entrypoint instead of allowing ad hoc fetch logic across the repo.
    - Parses catalog JSON either as a simple listing array or as a provider-registry-style object with `items`.
    - Computes SHA-256 digests for catalog content, writes workspace-local cache entries under `state/cache/catalogs`, and maintains a digest/ref index.
    - Resolves catalogs from local files or from the workspace-local cache in offline mode.
    - Defines the bundle-side remote catalog reference contract: `ghcr://<path>[:tag|@sha256:...]` maps into `ghcr.io/greenticai/<path>...`, while `oci://...` keeps the explicit OCI form. The default wizard catalog now uses `ghcr://catalogs/well-known`.
    - Centralizes remote-catalog fetching behind a client trait and now uses `greentic-distributor-client`'s `OciPackFetcher` for uncached GHCR/OCI refs before writing the results back into the workspace-local cache.
    - Validates the checked-in default public catalog fixture at `packs/well-known.json`.
  - **Key dependencies / integration points:**
    - Consumed by `src/wizard/mod.rs`.
    - Exercised by `tests/catalog_resolution.rs`.

- **Path:** `packs/well-known.json` and `.github/workflows/catalog.yml`
  - **Role:** Source-of-truth default public extension-provider catalog and its publication workflow.
  - **Key functionality:**
    - Stores the checked-in JSON catalog that backs the wizard's default `ghcr://catalogs/well-known` reference.
    - Currently seeds seven fixture-oriented deployer OCI entries under `ghcr.io/greenticai/packs/deployer/`: `greentic.fixture.serverless`, `greentic.fixture.juju.machine`, `greentic.fixture.juju.k8s`, `greentic.fixture.snap`, `greentic.fixture.k8s.raw`, `greentic.fixture.helm`, and `greentic.fixture.terraform`.
    - Publishes the catalog to GHCR on pushes to `main` or `master`, tagging both `sha-<commit>` and `latest`.
    - Uses `GITHUB_TOKEN` and OCI source metadata so the GHCR package links back to `greenticai/greentic-bundle`; anonymous pulls still depend on the package being made public once in GitHub Packages settings.
  - **Key dependencies / integration points:**
    - Fetched by `src/catalog/client.rs` through the GHCR shorthand mapping used by the wizard's common-extension-provider flow.

- **Path:** `src/project/mod.rs`
  - **Role:** Starter authored/generated workspace layout, deterministic resolved-output sync, and root lock contract.
  - **Key functionality:**
    - Creates the expected baseline structure rooted at `bundle.yaml`, `tenants/...`, `resolved/...`, and `state/resolved/...`.
    - Defines the parsed/writable `BundleWorkspaceDefinition` model for `bundle.yaml`, including authored refs, `app_pack_mappings`, and `hooks`, `subscriptions`, and `capabilities`.
    - Ensures default tenant/team gmap files exist with `_ = forbidden`.
    - Computes canonical gmap paths and generated resolved-output paths for tenant/team targets.
    - Rerenders richer resolved manifest YAML files for tenants and teams after access mutations, including bundle metadata, catalog refs, extension providers, hooks/subscriptions/capabilities, and per-target app-pack policy summaries.
    - Defines and reads/writes the deterministic `bundle.lock.json` structure used by wizard execution and inspect output, including setup-state file references.
    - Initializes new workspaces, keeps authored dependency refs synchronized into the lock file, and materializes supported app-pack/provider references into the operator-compatible bundle filesystem layout.
  - **Key dependencies / integration points:**
    - Called by `src/access/mod.rs` and the wizard apply path.

- **Path:** `src/build/mod.rs`, `src/build/manifest.rs`, `src/build/lock.rs`, `src/build/plan.rs`, `src/build/export.rs`, `src/build/squashfs.rs`
  - **Role:** PR-BUNDLE-06 canonical build model and artifact pipeline.
  - **Key functionality:**
    - Defines the build-state model that collects `bundle.yaml`, `bundle.lock.json`, resolved files, setup-state files, and materialized `packs/` / `providers/` assets into a normalized staging directory.
    - Defines the artifact manifest embedded into the built bundle, including authored refs plus `hooks`, `subscriptions`, `capabilities`, and structured resolved-target summaries.
    - Writes normalized build state under `state/build/<bundle>/normalized`.
    - Builds deterministic SquashFS `.gtbundle` artifacts using `mksquashfs`, with the default artifact target at `dist/<bundle>.gtbundle`.
    - Can write a normalized build directory without producing an artifact, allowing workspace-side inspect/doctor to exercise the same reader path as unpacked builds.
    - Uses the same reader validation rules for workspace-side doctor/inspect paths and reports concrete reader contract failures in doctor output.
    - Implements lock-drift checks between current workspace inputs and `bundle.lock.json`.
  - **Key dependencies / integration points:**
    - Used by the CLI build/export/doctor/inspect commands.
    - Delegates artifact parsing to `crates/greentic-bundle-reader`.
    - Exercised by `tests/build_artifact.rs`.

- **Path:** `src/runtime.rs`
  - **Role:** Process-local CLI runtime settings.
  - **Key functionality:**
    - Carries the global `--offline` decision from the CLI into lower-level catalog resolution code.
  - **Key dependencies / integration points:**
    - Set by `src/cli/mod.rs` and read by `src/wizard/mod.rs`.

- **Path:** `src/setup/mod.rs`, `src/setup/legacy_formspec.rs`, `src/setup/qa_bridge.rs`, `src/setup/persist.rs`, `src/setup/backend.rs`
  - **Role:** PR-BUNDLE-05 setup bridge and composition-time persistence layer.
  - **Key functionality:**
    - Defines the bundle-local setup form model and persisted setup-state schema.
    - Converts legacy setup specs and provider-QA JSON into normalized forms.
    - Supports catalog-embedded setup metadata so selected providers or packs can contribute setup contracts through the same catalog resolution path used for bundle refs.
    - Normalizes replayed setup answers, separating non-secret config from secret values.
    - Persists deterministic JSON setup state under `state/setup/` through a backend seam that supports file-backed and no-op modes.
  - **Key dependencies / integration points:**
    - Invoked by `src/wizard/mod.rs`, which now auto-discovers missing setup specs from matching catalog items during replay when `setup_execution_intent` is enabled.
    - Exercised by `tests/setup_flow.rs`.

- **Path:** `crates/greentic-bundle-reader/`
  - **Role:** Read-only bundle reader crate for runtime-facing bundle consumption.
  - **Key functionality:**
    - Pins the reader crate to `rust-version = "1.91"`.
    - Opens built `.gtbundle` SquashFS artifacts by reading embedded `bundle-manifest.json` and `bundle-lock.json` through `unsquashfs`.
    - Opens normalized unpacked build directories by reading the same manifest/lock files directly from disk.
    - Owns the validated `OpenedBundle::from_parts` constructor and `open_build_dir_with_source`, allowing the main crate to reuse the build-dir reader path while preserving workspace-root error/reporting context.
    - Validates the basic bundle structure, including supported format version, expected workspace/lock file names, manifest/lock consistency, setup-state file agreement, and presence of manifest-listed resolved/setup files.
    - Exposes a stable typed runtime surface for bundle metadata, app packs, extension providers, catalogs, hooks, subscriptions, capabilities, resolved-target summaries, resolved files, and setup-state files without leaking on-disk layout details.
  - **Key dependencies / integration points:**
    - Used by `src/build/mod.rs` for artifact-side inspect/doctor behavior.
    - Publishable as a workspace crate because the main CLI depends on it at runtime.

- **Path:** `README.md` and `docs/cli.md`
  - **Role:** User-facing baseline documentation.
  - **Key functionality:**
    - Document the command surface, the early workspace/path contract, the wizard behavior, the access/gmap mutation behavior, the catalog/lock behavior, the setup behavior, the PR-BUNDLE-06 build/export behavior, and the new reader-crate contract.
    - Record that `build` is now the primary `.gtbundle`-producing command.
  - **Key dependencies / integration points:**
    - Included in the published crate package.

- **Path:** `tests/answer_document.rs`, `tests/i18n_smoke.rs`, `tests/wizard_flow.rs`, `tests/access_eval.rs`, `tests/access_workspace.rs`, `tests/catalog_resolution.rs`, `tests/setup_flow.rs`, `tests/build_artifact.rs`, and `tests/workspace_cli.rs`
  - **Role:** Baseline test coverage for schema, localization, wizard orchestration, access/workspace mutation, catalog/lock behavior, setup persistence, and artifact build/export behavior.
  - **Key functionality:**
    - Verify deterministic answer-document round-tripping and migration behavior.
    - Verify localized help output and locale fallback.
    - Verify wizard answer emission, side-effect-free validation, replayed apply behavior, migration of older answer payloads, dry-run plan output, locale-aware prompt rendering, lock writing, wizard help flags, and bare `wizard` execute-by-default behavior.
    - Verify gmap rule precedence, team-over-tenant overlays, dry-run access previews without writes, executed tenant/team gmap updates, resolved-output generation, and comment-preserving gmap edits.
    - Verify local catalog resolution, deterministic lock output, offline replay from cached catalogs, inspect output, GHCR shortcut mapping through the client seam, and uncached-offline recovery hints.
    - Verify deterministic legacy-setup conversion, stable provider-QA bridging, dry-run setup non-persistence, and replayed setup-answer persistence equivalence.
    - Verify byte-stable `.gtbundle` output, stable inspect output, workspace/artifact doctor checks, dry-run export planning, lock-drift detection, direct reader-crate opening of both artifacts and normalized build directories, rejection of invalid manifest/lock file agreements, concrete workspace reader-validation error reporting, rejection of bundles whose listed files are missing, richer resolved-target runtime-surface output, and real `init` / `add` / `remove` CLI behavior.
  - **Key dependencies / integration points:**
    - Run by `cargo test` and `ci/local_check.sh`.

- **Path:** `ci/local_check.sh`, `.github/workflows/ci.yml`, `.github/workflows/publish.yml`
  - **Role:** Repo validation and release automation.
  - **Key functionality:**
    - Continue to validate the expanded workspace and test suite with lint, tests, docs, and crates.io dry-run packaging.
    - Now package and publish both the root CLI crate and the reader crate in workspace dependency order.
    - Run full `cargo package` / `cargo publish --dry-run` locally for independent crates, but only verify the source-tree packaging contract for crates with unpublished workspace-internal dependencies and defer their final `cargo package` / `cargo publish --dry-run` to the release workflow after earlier workspace crates have been published.
    - Trigger the release workflow from pushes to `main` / `master`, derive `vX.Y.Z` from the primary crate version, skip publication when that tag already exists, and create the tag/release automatically before attaching release assets.
  - **Key dependencies / integration points:**
    - Use `ci/workspace_publish.py` to determine publishable packages.

- **Path:** `.codex/PR-BUNDLE.md` and `.codex/global_rules.md`
  - **Role:** Local execution policy and staged plan.
  - **Key functionality:**
    - Record the operator-first macro-plan and the bundle-side implementation decisions that now shape PR-BUNDLE-01 through PR-BUNDLE-06.
  - **Key dependencies / integration points:**
    - Continue to guide future PR work in this repo.

## 3. Work In Progress, TODOs, and Stubs
- **Location:** `src/wizard/mod.rs` plan model
  - **Status:** partial
  - **Short description:** The deterministic plan envelope and step kinds exist, but the plan is still an internal test/preview artifact rather than a fully implemented execution engine.

- **Location:** `src/access/mod.rs` and `src/project/mod.rs`
  - **Status:** partial
  - **Short description:** Access mutation, resolved-output sync, and root lock writing now exist, and the resolved YAML now includes bundle/runtime metadata plus per-target app-pack policy summaries, but it is still not a final runtime-resolution format.

- **Location:** `src/catalog/client.rs`
  - **Status:** partial
  - **Short description:** The catalog seam exists, remote GHCR/OCI ref formats are now defined, and uncached remote refs now fetch through `greentic-distributor-client`, but only OCI/GHCR catalog delivery is implemented so far.

- **Location:** `src/setup/persist.rs` and `src/wizard/mod.rs`
  - **Status:** partial
  - **Short description:** Setup normalization and persistence still exist for replay/apply flows, but the staged composition wizard intentionally no longer prompts provider setup; later environment-specific flows still need a clearer home.

- **Location:** `src/build/manifest.rs` and artifact contents overall
  - **Status:** partial
  - **Short description:** Real `.gtbundle` output now exists and carries hooks/subscriptions/capabilities plus resolved-target summaries, but the embedded contract still does not enumerate a full runtime pack/provider/hook object model.

- **Location:** `crates/greentic-bundle-reader/src/lib.rs`
  - **Status:** partial
  - **Short description:** The reader crate now opens `.gtbundle` artifacts and normalized build directories and exposes typed bundle/dependency/catalog/file views plus hooks/subscriptions/capabilities and resolved-target summaries, but richer runtime views beyond these high-level structures are still pending.

- **Location:** `i18n/*.json` for non-`en` locales
  - **Status:** partial
  - **Short description:** The approved 66-locale set now exists in-repo and is embedded in the binary, but the non-English files are currently seeded from `en.json` and still need a later authenticated translation-generation pass for real translated content.

- **Location:** repository-wide marker scan
  - **Status:** note
  - **Short description:** No `TODO`, `FIXME`, `XXX`, `HACK`, `todo!`, or `unimplemented!` markers were present after PR-BUNDLE-06A; unfinished areas are represented by explicit partial implementations rather than inline markers.

## 4. Broken, Failing, or Conflicting Areas
- **Location:** `.codex/repo_overview_task.md`
  - **Evidence:** The file referenced by the repo-level global instructions is still missing.
  - **Likely cause / nature of issue:** Repo guidance drift; overview maintenance still relies on the explicit user prompt and `.codex/global_rules.md` rather than a local task file.

- **Location:** `.github/workflows/publish.yml` runner matrix
  - **Evidence:** `bash ci/local_check.sh` passes locally, but the six-platform release matrix plus GHCR publication path still cannot be exercised from inside the repo alone.
  - **Likely cause / nature of issue:** Cross-platform release verification remains GitHub-infrastructure-dependent.

- **Location:** setup input contract in `src/wizard/mod.rs` and `src/setup/persist.rs`
  - **Evidence:** The wizard now routes both the root request form and discovered setup forms through `greentic-qa-lib`, and secret questions use no-echo terminal input on TTYs, but the surrounding authoring flow still feeds setup forms one at a time.
  - **Likely cause / nature of issue:** The repo now reuses the shared QA driver throughout the wizard, but the higher-level bundle orchestration around it remains intentionally minimal.

- **Location:** `src/build/squashfs.rs`
  - **Evidence:** Artifact creation and inspection rely on external `mksquashfs` / `unsquashfs` binaries being present on the host.
  - **Likely cause / nature of issue:** PR-BUNDLE-06 chose the real SquashFS path now, so the build pipeline depends on host tooling until a bundled/library-backed SquashFS implementation exists.

## 5. Notes for Future Work
- Replace the internal plan-preview model with execution logic tied to real workspace mutation and reference resolution.
- Expand the resolved-output generation beyond the current summary-focused tenant/team metadata once catalog/distributor resolution lands.
- Expand remote catalog support beyond the current OCI/GHCR delivery path if additional distributor source kinds become part of the public bundle contract.
- Improve the higher-level wizard orchestration around the new end-to-end `greentic-qa-lib` integration, especially for richer multi-form flows.
- Enrich `bundle.lock.json` with resolved digests for app packs and extension providers once those resolution flows exist.
- Expand the embedded artifact manifest beyond the current minimal lock/summary view once runtime-consumer needs are finalized.
- Expand the reader crate once `.gtbundle` format work starts landing.
- Add more locale catalogs after the `en.json` baseline and prompt keys stabilize.
- Add the missing `.codex/repo_overview_task.md` if the repo wants its maintenance workflow to be self-contained.
