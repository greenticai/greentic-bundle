# Bundle Extension Migration — Design

- **Date:** 2026-04-17
- **Status:** Draft, pending user review
- **Branch:** `spec/wasm-bundle-extensions`
- **Owner:** TBD
- **Related:**
  - `greentic-designer-extensions` (WIT contracts, `greentic-ext-runtime`, `greentic-ext-contract`) — specification stage
  - `spec/wasm-deploy-extensions` (sibling migration committed same day, commit `ffbb96c` in `greentic-deployer`) — mirrored pattern
  - Hand-off doc from prior session proposing `bundle-core` + `cards2pack-core` extraction
  - Memory: `~/.claude/projects/-home-bimbim-works-greentic/memory/bundle-extension-migration.md`
- **Repo split:** Hybrid (Option C). This repo (`greentic-bundle`) owns host integration + the built-in `Standard` recipe handler + fixture extension. A sibling repo `greentic-bundle-extensions` (not yet created) owns shippable reference extensions, starting with `bundle-standard`. Symmetric with the existing `greentic-designer` / `greentic-designer-extensions` split and with `spec/wasm-deploy-extensions`.

## 1. Context & motivation

### Current state

`greentic-bundle` is a mature Rust CLI (workspace root + `crates/greentic-bundle-reader` sibling, Rust 1.94.0, edition 2024) that ships one authoring and build pipeline:

- `cli/` — subcommands: `wizard`, `build`, `inspect`, `doctor`, `access`, `init`, `add`, `remove`, `export`, `unbundle`
- `project/` — `BundleWorkspaceDefinition` backed by `bundle.yaml`
- `build/` — deterministic normalize → SquashFS assembly via `mksquashfs` subprocess
- `catalog/`, `access/`, `answers/`, `setup/`, `wizard/`, `i18n/` — supporting subsystems

Separately, `greentic-designer-extensions` defines frozen WIT contracts for the full extension ecosystem — `greentic:extension-base@0.1.0`, `greentic:extension-bundle@0.1.0`, `greentic:extension-deploy@0.1.0`, `greentic:extension-design@0.1.0`, `greentic:extension-host@0.1.0` — and a runtime crate (`greentic-ext-runtime`) with wasmtime Component Model support, hot reload, and capability resolution. One reference extension (`adaptive-cards`) ships today. No bundle reference extension exists yet.

The sibling effort `spec/wasm-deploy-extensions` (committed `ffbb96c` in `greentic-deployer` the same day as this spec) establishes an Option C hybrid pattern: feature-gated `src/ext/` module in the existing repo plus a new sibling repo for shippable reference extensions, with execution-mode selection (builtin delegated vs full WASM) declared in `describe.json`.

### Goal

Add WASM bundle extension handling to `greentic-bundle` without altering existing subprocess paths. All current CLI invocations (`greentic-bundle wizard`, `greentic-bundle build`, etc.) remain bit-for-bit unchanged. The extension surface is additive, feature-gated, and default-off. The designer UI eventually calls `runtime.invoke_bundle(ext_id, recipe_id, config, session)` and receives a `bundle-artifact { filename, bytes, sha256 }` back — the existing CLI users see no behavior change.

### Non-goals (explicit)

- **Do not** rewrite `build/`, `wizard/`, `project/`, `catalog/`, `access/`, or any other existing subsystem.
- **Do not** implement Mode B full-WASM execution in Phase A — only the contract is defined; dispatch returns `ExtensionError::ModeBNotImplemented`.
- **Do not** extract `bundle-core` as a pure-Rust crate — deferred to Mode B or to when test determinism requires it.
- **Do not** touch `greentic-cards2pack` — card→flow conversion happens upstream in the designer before bundle-ext is invoked. Cards2pack is unrelated to this migration.
- **Do not** migrate `greentic-designer/src/orchestrate/*` off subprocess invocations in this effort.
- **Do not** convert `greentic-bundle` to a Cargo workspace root. The existing `crates/greentic-bundle-reader` layout is preserved. Revisit only if `src/ext/` exceeds ~1500 LoC.
- **Do not** ship reference extensions from this repo. `bundle-standard` and all future reference extensions belong in the new `greentic-bundle-extensions` repo (see §5, §10).

## 2. Architecture

### Layering

```
┌──────────────────────────────────────────────────────────────────┐
│  greentic-bundle CLI (main.rs)                                   │
│  - Existing subcommands (wizard, build, …) UNCHANGED             │
│  - New subcommand: `ext [list|info|validate|install-dir|render]` │
└────────────────────────────┬─────────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────────┐
│  src/ext/dispatcher (NEW, feature-gated `extensions`)            │
│  - resolve recipe-id → BuiltinRecipeId or WASM ext               │
│  - read describe.json execution.kind                             │
│  - route by mode                                                 │
└────────────┬─────────────────────────────────┬───────────────────┘
             │ kind=builtin                    │ kind=wasm (Phase B)
             ▼                                 ▼
    ┌─────────────────────┐         ┌─────────────────────────────┐
    │ src/ext/builtin_    │         │ greentic-ext-runtime        │
    │ bridge              │         │ (git dep, pinned rev)       │
    │ BuiltinRecipeId::   │         │ — returns                    │
    │ Standard →          │         │   ModeBNotImplemented        │
    │ existing build      │         │   in Phase A                 │
    │ pipeline            │         └─────────────────────────────┘
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────────────┐
    │ Existing build/ pipeline    │
    │ - project::load_workspace   │
    │ - build::assemble           │
    │ - ZIP or mksquashfs         │
    │ UNCHANGED                   │
    └─────────────────────────────┘
```

### Principles

1. **Existing paths untouched.** Files `build/*.rs`, `wizard/*.rs`, `project/*.rs`, `catalog/*.rs`, `access/*.rs`, `main.rs` core logic receive **zero line changes** in Phase A beyond the additive feature-gated `Ext(ExtCommand)` CLI variant and `#[cfg(feature = "extensions")] pub mod ext;` at the lib level.
2. **Feature-gated default-off.** The new behavior ships behind `--features extensions`. When disabled, the `ext` module is excluded from compilation entirely; binary size and compile time stay unchanged for existing users. A compile-regression guard caps additional binary size at <500 KB when the feature is off.
3. **Unified registry.** A single in-memory `ExtensionRegistry` holds both built-in recipe entries (from the `BuiltinRecipeId` enum) and loaded WASM extension entries. Conflict detection is unified: two recipes cannot share the same id regardless of source.
4. **Single-crate.** No workspace root conversion. The existing `crates/greentic-bundle-reader` sibling is untouched. Revisit after 6 months if `src/ext/` exceeds ~1500 LoC.
5. **Git-dep cross-repo.** `greentic-ext-runtime` and `greentic-ext-contract` from `greentic-designer-extensions` are pinned via `git+rev` in `Cargo.toml`, matching the pattern already in use for `adaptive-card-core`. Upgrading the runtime is a controlled operation, not a floating main branch.

### Extension execution modes

Each bundle recipe contribution declares one of two execution modes in its `describe.json`:

| Mode | `execution.kind` | `validate-config` + schemas + `render` metadata | `render` body (output bytes) |
|------|------------------|-------------------------------------------------|------------------------------|
| **A — Builtin delegated** | `"builtin"` | WASM extension | Existing native build pipeline via `builtin_bridge` |
| **B — Full WASM** | `"wasm"` | WASM extension | WASM extension via `greentic-ext-runtime` (Phase B) |

Phase A implements **Mode A only**. Mode B is declared in the describe schema but all dispatch calls return `ExtensionError::ModeBNotImplemented`. The host dispatcher reads `describe.json` before any WASM instantiation; for Mode A, the WASM `render` export is never invoked — the path routes directly through `builtin_bridge`.

## 3. Module layout

### New module tree

```
src/ext/
├── mod.rs                   Public API surface, feature-gate guard
├── describe.rs              Parse describe.json + bundle-specific `execution` field
├── loader.rs                Filesystem discovery, signature verification hook
├── registry.rs              Unify built-in + WASM contracts, conflict detection
├── dispatcher.rs            Route Execution::Builtin | ::Wasm
├── builtin_bridge.rs        Glue: BuiltinRecipeId::Standard → existing build pipeline
├── wasm.rs                  Thin stub over greentic_ext_runtime::ExtensionRuntime (Mode B seam)
└── errors.rs                ExtensionError enum (thiserror)
```

### Changes to existing files (minimal)

| File | Change |
|------|--------|
| `Cargo.toml` | Add `[features] extensions = ["dep:greentic-ext-runtime", "dep:greentic-ext-contract"]`; add optional deps via `git+rev` |
| `src/lib.rs` | `#[cfg(feature = "extensions")] pub mod ext;` |
| `src/cli/mod.rs` | Add `Ext(ExtCommand)` variant to the top-level CLI enum (feature-gated variant) |
| `src/main.rs` | Route `Ext(cmd)` to `ext::run_cli(cmd)` when the feature is enabled; print a clear "extensions feature not enabled" error otherwise |
| `i18n/en.json` + all locale files | Add i18n keys under `ext.*` namespace for the new subcommand — every user-facing string goes through the i18n catalog per project convention |

Every other existing file is untouched.

### Key types

```rust
// src/ext/describe.rs
#[derive(Deserialize)]
pub struct BundleRecipeContribution {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub icon_path: Option<String>,
    pub config_schema: String,          // relative path
    pub supported_capabilities: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Execution {
    Builtin { builtin_id: BuiltinRecipeIdStr },
    Wasm,
}

// src/ext/registry.rs
pub struct ExtensionRegistry {
    pub builtin: Vec<BuiltinRecipeEntry>,
    pub wasm: Vec<WasmExtensionEntry>,
}

impl ExtensionRegistry {
    pub fn resolve(&self, ext_id: &str, recipe_id: &str) -> Result<ResolvedRecipe, ExtensionError>;
    pub fn list(&self) -> impl Iterator<Item = RecipeSummary>;
}

// src/ext/errors.rs
#[derive(thiserror::Error, Debug)]
pub enum ExtensionError {
    #[error("extension not found: {0}")]
    NotFound(String),
    #[error("recipe not found: {ext}/{recipe}")]
    RecipeNotFound { ext: String, recipe: String },
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("conflict: recipe id `{0}` offered by multiple extensions")]
    Conflict(String),
    #[error("Mode B (full WASM) not implemented in Phase A")]
    ModeBNotImplemented,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Runtime(#[from] greentic_ext_runtime::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
```

## 4. Recipe `standard` — config schema + handler flow

### Config schema

Location in reference ext: `schemas/standard.config.schema.json`. Shipped with `describe.json` via `recipes[0].configSchema`.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["metadata", "channels"],
  "additionalProperties": false,
  "properties": {
    "metadata": {
      "type": "object",
      "required": ["name", "version"],
      "properties": {
        "name":    { "type": "string", "pattern": "^[a-z][a-z0-9-]{1,62}$" },
        "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" },
        "author":  { "type": "string" }
      }
    },
    "channels": {
      "type": "array",
      "items": { "enum": ["slack","teams","webchat","telegram","whatsapp","webex","email"] },
      "uniqueItems": true,
      "minItems": 1
    },
    "embed_ui": { "enum": ["none","webchat"], "default": "none" },
    "i18n": {
      "type": "object",
      "properties": {
        "source":  { "type": "string", "default": "en" },
        "targets": { "type": "array", "items": { "type": "string" }, "default": [] }
      }
    },
    "format": { "enum": ["gtpack-legacy"], "default": "gtpack-legacy" }
  }
}
```

Phase A locks `format` to `gtpack-legacy` (the existing ZIP-based pack path). When the Module 8 Application Pack spec freezes, the enum extends to `["apack", "gtpack-legacy"]` and the default shifts.

### Mode A handler flow (`builtin_bridge::handle_standard`)

Invoked via `src/ext/dispatcher.rs` after `describe.json` read confirms `execution.kind == "builtin"` and `builtin_id == "standard"`. The WASM `bundling::render` export is never invoked in Mode A.

```
Input:
  - DesignerSession { flows_json, contents_json, assets, capabilities_used }
  - config_json (validated against standard.config.schema.json)

Flow:
  1. Parse config_json → StandardConfig
  2. Create a deterministic tmpdir under state/ext-render/<session-id>/
  3. Synthesize ephemeral BundleWorkspaceDefinition:
       - Write flows from flows_json → flows/*.ygtc
       - Write contents from contents_json → assets/cards/*.json
       - Write raw assets (bytes) → assets/...
       - Synthesize bundle.yaml from config.metadata + config.channels
       - Synthesize tenants/default/tenant.gmap from capabilities_used + channel scope
  4. If config.i18n.targets non-empty:
       - Invoke existing i18n extraction + translation pipeline (native, as today)
       - Write per-locale bundles under assets/i18n/
  5. If config.embed_ui == "webchat":
       - Copy bundled WebChat UI assets (from crate resources) into assets/ui/
  6. Invoke existing build pipeline:
       - project::load_workspace(tmpdir)
       - build::assemble() → state/build/.../normalized
       - Archive as ZIP (format == "gtpack-legacy") → Vec<u8>
  7. Compute sha256(bytes)
  8. Return bundle-artifact {
       filename: format!("{}-{}.gtpack", metadata.name, metadata.version),
       bytes,
       sha256,
     }

Output:
  - bundle-artifact record (matches WIT definition)
  - tmpdir retained under state/ for debugging; cleaned by `greentic-bundle doctor --clean`
```

The ephemeral workspace approach guarantees zero state leakage between invocations, deterministic per-invocation output, and reuse of the existing build pipeline without modification. This is critical for the "existing paths untouched" principle.

## 5. Reference extension structure (new repo `greentic-bundle-extensions`)

A new sibling repo, under `greenticai` org, mirrors the AC ext and deploy-ext pattern:

```
greentic-bundle-extensions/
├── Cargo.toml                               workspace root
├── README.md
├── CLAUDE.md                                specification-stage until implementation begins
├── LICENSE
├── ci/
│   └── local_check.sh
├── rust-toolchain.toml                      1.94.0, edition 2024
├── rustfmt.toml
├── wit/                                     vendored copy of frozen contracts
│   ├── extension-base.wit
│   ├── extension-host.wit
│   └── extension-bundle.wit
├── reference-extensions/
│   └── bundle-standard/
│       ├── Cargo.toml                       [package] greentic-ext-bundle-standard
│       ├── build.sh                         cargo component build → .gtxpack packaging
│       ├── describe.json                    execution.kind="builtin", single recipe
│       ├── schemas/
│       │   └── standard.config.schema.json
│       ├── i18n/
│       │   ├── en.json
│       │   └── id.json
│       ├── src/
│       │   ├── lib.rs                       impl recipes + bundling + manifest + lifecycle
│       │   └── bindings.rs                  wit-bindgen output (gitignored)
│       ├── tests/
│       │   └── validate_config_smoke.rs
│       └── examples/
│           └── minimal.json                 minimal valid config
└── greentic.bundle-standard-0.1.0.gtxpack   built artifact (optional committed; mirror AC ext)
```

**`describe.json` shape:**

```json
{
  "apiVersion": "greentic.ai/v1",
  "kind": "BundleExtension",
  "metadata": {
    "id": "greentic.bundle-standard",
    "name": "Standard Bundle Recipe",
    "version": "0.1.0",
    "summary": "Package designer session into a Greentic pack (.gtpack ZIP)",
    "author": { "name": "Greentic" },
    "license": "MIT"
  },
  "engine": {
    "greenticDesigner": ">=0.6.0",
    "extRuntime": "^0.1.0"
  },
  "capabilities": {
    "offered": [{ "id": "greentic:bundle/standard", "version": "0.1.0" }],
    "required": []
  },
  "runtime": {
    "component": "extension.wasm",
    "memoryLimitMB": 128,
    "permissions": { "network": [], "secrets": [], "callExtensionKinds": [] }
  },
  "execution": {
    "kind": "builtin",
    "builtinId": "standard"
  },
  "contributions": {
    "recipes": [
      {
        "id": "standard",
        "displayName": "Standard Greentic Pack",
        "description": "Package designer session into a .gtpack archive",
        "configSchema": "schemas/standard.config.schema.json",
        "supportedCapabilities": [
          "greentic:adaptive-cards/*",
          "greentic:flows/*",
          "greentic:channels/*"
        ]
      }
    ]
  }
}
```

**Build output:** `greentic.bundle-standard-0.1.0.gtxpack` — installable via `greentic-bundle ext install <artifact>` or picked up by the bundle loader from an install directory.

## 6. WIT contract usage (unchanged)

The `bundle-standard` extension implements the existing **frozen** `greentic:extension-bundle@0.1.0` world from `greentic-designer-extensions/wit/extension-bundle.wit`.

**Exports:**
- `greentic:extension-base/manifest` — `get-identity`, `get-offered`, `get-required`
- `greentic:extension-base/lifecycle` — `init(config)`, `shutdown`
- `greentic:extension-bundle/recipes` — `list-recipes`, `recipe-config-schema`, `supported-capabilities`
- `greentic:extension-bundle/bundling` — `validate-config`, `render` (stub body returns `Internal("routed via builtin")`; host never calls it in Mode A)

**Imports (host):**
- `greentic:extension-base/types`
- `greentic:extension-host/logging`
- `greentic:extension-host/i18n`
- `greentic:extension-host/broker`

The WIT is **not modified**. Phase A does not require new WIT versions or new interfaces. Phase B (Mode B full-WASM execution) can likely be implemented using existing imports alone — only if future recipe variants require filesystem output or credential access will `host::fs` / `host::secrets` additions be needed, consistent with deploy-ext Phase B prerequisites.

## 7. Testing strategy

### Unit tests (`greentic-bundle/src/ext/`)

- `describe.rs` — parse valid + invalid `describe.json` fixtures (missing `execution`, unknown `kind`, wrong schema path, malformed JSON)
- `registry.rs` — conflict detection (two recipes with same id, two extensions offering the same capability)
- `dispatcher.rs` — route by `execution.kind` (builtin vs wasm vs unknown → proper error; Mode B → `ModeBNotImplemented`)
- `builtin_bridge.rs` — `handle_standard` with fixture `DesignerSession` bytes → expected pack structure + deterministic sha256
- `errors.rs` — error display, `thiserror` `#[from]` wiring, error chain propagation

### Integration tests (`tests/ext_*.rs` in `greentic-bundle`)

- `ext_list_smoke.rs` — `greentic-bundle ext list --extension-dir testdata/ext/` → lists fixture extension
- `ext_info_smoke.rs` — `greentic-bundle ext info greentic.bundle-fixture` → prints recipe metadata
- `ext_validate_smoke.rs` — valid and invalid config JSON → expected diagnostics output
- `ext_render_builtin.rs` — end-to-end: fixture extension + synthetic `DesignerSession` bytes → produces valid ZIP → unzip and validate `pack.yaml`, `flows/`, `assets/` presence

### Fixture extension (`greentic-bundle/testdata/ext/fixture-bundle/`)

A minimal bundle extension with `execution.kind="builtin"`, `builtinId="standard"`, and one test recipe. Built once, committed as a `.gtxpack` artifact (~few KB). CI rebuilds only if the WIT vendored copy changes. Used by integration tests to exercise the end-to-end load-and-dispatch path without requiring the sibling repo to be present.

### CI integration (`ci/local_check.sh`)

- `cargo test --features extensions` — mandatory additional run on top of existing
- `cargo test` (default features, no `extensions`) — existing suite must still pass; proves feature gate is solid
- `cargo clippy --features extensions --all-targets -- -D warnings`
- `python3 ci/i18n_check.py validate` — catches new `ext.*` keys missing from locale files

### Reference ext repo (`greentic-bundle-extensions/ci/`)

- `cargo component build --release` produces valid `.wasm`
- `build.sh` packages into `.gtxpack` with `describe.json` + `schemas/` + `i18n/` bundled
- Round-trip test: install to tmpdir → `greentic-bundle ext info greentic.bundle-standard` loads and prints metadata

## 8. Acceptance criteria — Phase A done definition

### Bundle repo PR (`greentic-bundle/feat/ext-phase-a`)

- [ ] `--features extensions` compiles clean on Linux, macOS, Windows
- [ ] `cargo build` without the feature is unchanged (binary SHA may differ due to version string, but argv surface is identical)
- [ ] `cargo test` (default features) passes — existing behavior unchanged
- [ ] `cargo test --features extensions` passes — all new extension tests green
- [ ] `cargo clippy --features extensions --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] `ci/local_check.sh` green
- [ ] Binary size delta `< 500 KB` with feature off (compile regression guard)
- [ ] `greentic-bundle` without `--features extensions`: running `greentic-bundle ext …` prints a clear "extensions feature not enabled" error with a pointer to documentation; no panic, no silent exit
- [ ] `greentic-bundle wizard|build|inspect|doctor|access|init|add|remove|export|unbundle` behavior unchanged — bit-for-bit identical dry-run outputs against `main`
- [ ] Fixture extension loads and lists under `greentic-bundle ext list`
- [ ] Fixture extension renders a synthetic `DesignerSession` into a valid ZIP that `greentic-bundle inspect` can parse
- [ ] i18n keys for the `ext` subcommand are present in `en.json` and in 5 target locales (id, ja, zh, es, de) minimum; remaining locales are stubbed via the `greentic-i18n-translator` follow-up flow
- [ ] New i18n keys pass `python3 ci/i18n_check.py validate`
- [ ] The WIT vendored copy matches `greentic-designer-extensions` at a pinned commit recorded in a header comment of the vendored `.wit` file

### Reference ext repo PR (`greentic-bundle-extensions/feat/bundle-standard-0.1.0`)

- [ ] `cargo component build --release` produces valid WASM component
- [ ] `build.sh` produces an installable `.gtxpack` artifact
- [ ] `describe.json` validates against the extension schema (`execution.kind="builtin"` accepted by the host parser)
- [ ] `greentic-bundle ext install <artifact>` places the extension under the install directory and it appears in `ext list`
- [ ] `greentic-bundle ext validate greentic.bundle-standard --config examples/minimal.json` returns an empty diagnostic list
- [ ] `greentic-bundle ext render greentic.bundle-standard --recipe standard --config examples/minimal.json --session testdata/designer-session.json` writes a valid `.gtpack` ZIP to stdout redirection or `--out`
- [ ] The unzipped output matches a deterministic fixture (reproducible build, sha256 pinned in the test)
- [ ] README documents install path, minimal config example, and troubleshooting tips

## 9. Open items / deferred decisions

| Item | Status | Resolution |
|------|--------|------------|
| Module 8 `.apack` format freeze (ZIP vs SquashFS vs OCI) | OPEN | Phase A locks `format="gtpack-legacy"`. `format="apack"` enum value is enabled once Module 8 closes and the standard is published in `greentic-designer-extensions/docs/superpowers/specs/2026-04-17-designer-extension-system-design.md` §12 |
| Mode B full-WASM execution | DEFERRED | Phase B. Prerequisites: `host::storage` interface (likely breaking host version bump to `0.2.0`), pure-Rust build pipeline (bundle-core extraction) |
| Bundle-core pure-Rust extraction | DEFERRED | Triggered when Mode B begins or when extension tests demand deterministic non-subprocess pipelines |
| Cards2pack-core extraction | NOT IN SCOPE | Unrelated — designer performs card→flow conversion upstream, before bundle-ext invocation. `greentic-cards2pack` stays independent |
| Hot-reload of installed extensions | DEFERRED | Phase A uses filesystem scan on CLI invocation; Phase B can adopt `greentic-ext-runtime::hot_reload` |
| Ed25519 signing integration | DEFERRED | Layered as a post-build step or within deploy-ext, not a bundle-ext core concern |
| Multi-tenant extension namespacing | DEFERRED | Phase A uses a single install directory; tenant scoping happens at the host registry level in Phase B |
| Designer orchestrator migration | DEFERRED | Separate effort — migrating `greentic-designer/src/orchestrate/cards2pack.rs` and `orchestrate/deployer.rs` to `runtime.invoke_bundle` lands after both migrations stabilize |

## 10. Timeline & rollout

- **Week 1–2 (2026-04-17 → 2026-04-30):** PR #1 to `greentic-bundle` on branch `feat/ext-phase-a` — `src/ext/` module, `Ext` CLI subcommand, `BuiltinRecipeId::Standard` handler, fixture extension, i18n keys.
- **Week 2–3 (overlapping with last days of PR #1):** Create `greentic-bundle-extensions` repo; PR #2 — workspace scaffold, vendored WIT, `reference-extensions/bundle-standard/` with `describe.json`, schemas, i18n, source, tests, and build script.
- **Parallel throughout:** Coordinate review attention with the deploy-ext sibling migration (`spec/wasm-deploy-extensions`) — symmetric timelines, overlapping reviewer set, similar PR structure. Review effort is amortized across both migrations.
- **Week 3 (2026-05-01 → 2026-05-07):** Merge both PRs after acceptance gates pass. Update the memory doc at `~/.claude/projects/-home-bimbim-works-greentic/memory/bundle-extension-migration.md`. Update `greentic-docs` with a `how-to-write-a-bundle-extension.md` tutorial (mirror `greentic-designer-extensions/docs/how-to-write-a-bundle-extension.md` if present) referencing the reference ext as the canonical example.
- **Week 4+:** Phase B decision gate. Mode B is implemented only if (a) an actual user requests a recipe that cannot delegate to a builtin, or (b) Module 8 publishes an `.apack` format requiring per-recipe assembly logic that cannot live on the native side.

## 11. Implementation plan handoff

This spec becomes the input to `superpowers:writing-plans`. The resulting plan file at `docs/superpowers/plans/2026-04-17-bundle-extension-phase-a.md` will decompose the work into ordered, independently verifiable tasks suitable for `superpowers:subagent-driven-development`, mirroring the execution pattern already proven on `greentic-adaptive-card-mcp` Plan A/B.
