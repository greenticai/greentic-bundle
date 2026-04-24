# Bundle Extension Render Wiring — Design

- **Date**: 2026-04-19
- **Status**: Draft, awaiting user approval
- **Primary repo**: `greentic-designer` (bulk of work)
- **Touched repo**: `greentic-bundle` (small ergonomic additions only)
- **Related**: `greentic-bundle-extensions` (ships the `bundle-standard` `.gtxpack`)
- **Follows**: PR #46 `feat/ext-phase-a` — extension host + `ext render` already
  shipped

## 1. Summary

Wire `greentic-designer`'s `/api/pack` endpoint to produce its final
`.gtpack` via `greentic-bundle ext render greentic.bundle-standard
standard`. The render verb, `DesignerSession` struct, builtin bridge for
the `standard` recipe, and the `bundle-standard.gtxpack` reference
extension already exist.

**Important constraint** (see §7.0): `bundle-standard` consumes
pre-compiled YGTc flow YAML, not cards. Today the designer has no
in-process cards → flow converter (that logic lives inside
`greentic-cards2pack`). This spec therefore does **not** remove the
cards2pack subprocess; it **chains** cards2pack (for cards → flow) and
`ext render` (for flow + cards → pack). Avoiding the chain requires
Phase B (`cards2pack-core` extraction) which is out of scope.

Remaining work:

1. Bootstrap: ensure `greentic.bundle-standard` is discoverable by the
   `greentic-bundle` binary the designer spawns (unpack the bundled
   `.gtxpack` into a known directory).
2. Adapter: transform the designer's existing workspace (cards2pack
   output + card JSONs from `prepare_cards`) into the `DesignerSession`
   + `StandardConfig` JSON pair that the existing `ext render` call
   expects.
3. Add a second subprocess hop in `orchestrate/cards2pack.rs`: after
   cards2pack succeeds, call `ext render` and return its output as the
   final `.gtpack`.
4. Runtime probe + fallback to the legacy cards2pack-only path (without
   the trailing `ext render`) when `greentic-bundle` is missing the
   extensions feature.
5. Small ergonomic additions to `greentic-bundle ext render` (stdin for
   `--config`/`--session`, structured JSON summary on `--out` success) to
   make the designer integration clean. Optional, not blocking.

## 2. Motivation

Today the designer spawns `greentic-cards2pack`. The bundle-extension host
(`list`, `info`, `validate`, `render`, `install-dir` subcommands, registry,
dispatcher, builtin bridge) landed in PR #46 but nothing invokes it from
the designer. `extension-bundle.wit` defines the shape; the builtin bridge
implements it for the `standard` recipe; `bundle-standard-0.1.0.gtxpack`
ships the descriptor + schemas. We want to close the loop so designer
flows actually build packs through this pipeline — and so future WASM
extensions (Mode B) drop in without another designer change.

## 3. Non-goals

- Extract pure-Rust `bundle-core` / `cards2pack-core` (Phase B).
- Implement Mode B WASM execution (still returns `ModeBNotImplemented`).
- Add new recipes beyond `standard`.
- Change `.gtpack` archive format, manifest shape, or signing.
- Retire the `greentic-cards2pack` binary; fallback path stays.
- Change designer UI.

## 4. Current state of `greentic-bundle ext render`

Already shipped, no redesign needed. Recap of what exists:

**CLI signature** (from `src/cli/ext.rs`):

```
greentic-bundle [--extension-dir <DIR>] ext render \
    <extension-id> <recipe-id> \
    --config <FILE> \
    --session <FILE> \
    [--out <FILE>]
```

Behaviour (from `src/cli/mod.rs::run_ext`):
- Loads discovered extensions from `--extension-dir` or `state/ext/`.
- Reads `--config` + `--session` as files (no stdin today).
- Calls `ext::dispatcher::invoke_recipe(registry, ext_id, recipe_id,
  config_json, session_json)`.
- With `--out`: writes bytes, prints `cli.ext.render.wrote` i18n line with
  `{file}` + `{sha256}` to stdout.
- Without `--out`: writes raw `.gtpack` bytes to stdout.

**Dispatcher** (`src/ext/dispatcher.rs`):
- Routes `Execution::Builtin { builtin_id: "standard" }` to
  `builtin_bridge::handle_standard`.
- `Execution::Wasm` returns `ExtensionError::ModeBNotImplemented`.

**Builtin bridge** (`src/ext/builtin_bridge.rs`):

```rust
pub struct DesignerSession {
    pub flows_json: String,       // JSON array: [{"name": "...", "yaml": "..."}]
    pub contents_json: String,    // JSON array: [{"id": "...", "json": {...}}]
    pub assets: Vec<(String, Vec<u8>)>,
    pub capabilities_used: Vec<String>,
}

pub struct StandardConfig {
    pub metadata: StandardMetadata,   // { name, version, author? }
    pub channels: Vec<String>,        // ["webchat", "slack", …]
    pub embed_ui: String,             // default "none"
    pub i18n: I18nConfig,             // { source: "en", targets: [] }
    pub format: String,               // must be "gtpack-legacy"
}
```

`handle_standard` synthesizes an ephemeral workspace:
`flows/<name>.ygtc`, `assets/cards/<id>.json`, `assets/<rel>`,
`bundle.yaml`, `tenants/default/tenant.gmap` — then ZIPs with deterministic
entry order, returns bytes + SHA-256 + filename
`{name}-{version}.gtpack`.

## 5. Ergonomic additions to `greentic-bundle ext render`

Small, backward-compatible. All optional from the designer's side; the
designer can work with the current signature using temp files, but these
improve stream handling and machine-readability.

### 5.1 Stdin for `--config` / `--session`

Accept `-` for either flag, reading from stdin until EOF. If both are `-`,
reject at parse time with a clear error (can't multiplex stdin).

Rationale: avoids two temp files per pack job; designer can pipe one
payload and keep the other on disk.

### 5.2 Structured JSON summary on `--out`

Replace the current i18n stdout line when `--out` is set **and** a new
`--json` flag is passed:

```json
{"status":"ok","filename":"demo-0.1.0.gtpack","sha256":"ab12…","bytesLen":12345}
```

`--json` is off by default to preserve current human-readable CLI
behaviour; the designer opts in. Without `--json`, current i18n line
stays.

### 5.3 Error JSON on `--json`

When `--json` is set, errors from the dispatcher are also emitted as one
JSON line on stdout and exit with a non-zero code. `code` values mirror
`ExtensionError` variants: `invalid-config`, `invalid-session`,
`recipe-not-found`, `mode-b-not-implemented`, `io-error`, `other`.

### 5.4 Feature gate

All additions live under `#[cfg(feature = "extensions")]` in the same
paths as the existing `ext` subcommand. No new crate deps.

## 6. Extension bootstrap (designer)

The designer spawns `greentic-bundle`, which loads extensions from
`--extension-dir` (or `state/ext/`). For the render call to succeed,
`greentic.bundle-standard` must exist there.

### 6.1 Bundled `.gtxpack` asset

Embed `greentic.bundle-standard-0.1.0.gtxpack` as a build-time asset in
the designer binary via `include_bytes!`:

- File: `greentic-designer/assets/ext/greentic.bundle-standard-0.1.0.gtxpack`
  — committed into the repo, checked out from the
  `greentic-bundle-extensions` release.
- Vendoring policy: bump via a small script
  (`scripts/vendor-bundle-standard.sh`) that downloads a pinned release
  artifact by URL + SHA-256. Updating the vendored version is a
  deliberate, reviewable commit.

### 6.2 Unpack on first use

At designer startup, unpack the `.gtxpack` (a ZIP) into
`~/.greentic/designer/ext/greentic.bundle-standard/<version>/` if not
already present with a matching manifest digest. The path is stable
across runs; cache key = SHA-256 of the bundled bytes. On mismatch, wipe
and re-extract.

Designer passes this parent directory as `--extension-dir` to every
`greentic-bundle ext render` invocation.

### 6.3 User override

`GREENTIC_BUNDLE_EXT_DIR` env var lets power users point the designer at
an alternative directory (e.g., a locally-built extension being tested).
When set, designer skips the unpack step.

## 7. Designer wiring (`greentic-designer/src/orchestrate/cards2pack.rs`)

### 7.0 Constraint discovered during spec review

`bundle-standard`'s `handle_standard` consumes a `DesignerSession` whose
`flows_json` is an array of pre-compiled YGTc YAML strings. It does **not**
run cards → flow conversion. Today the designer's temp workspace after
`prepare_cards` + HTTP-entry extraction contains only card JSONs; the
`.ygtc` flow is produced downstream by the `greentic-cards2pack`
subprocess. Two consequences:

- We can't drop `greentic-cards2pack` outright — cards → flow conversion
  lives there. Extracting it (pure-Rust `cards2pack-core`) is Phase B
  scope we agreed to avoid.
- The new pipeline therefore **chains** both subprocesses: cards2pack
  produces `.ygtc` files into a workspace directory; designer reads those
  files, builds a `DesignerSession`, then calls `greentic-bundle ext
  render` to produce the `.gtpack`. cards2pack's own `.gtpack` output is
  discarded.

Cost: one extra subprocess per pack build (render is fast — small ZIP
with deterministic ordering; expected overhead on the order of tens to
low hundreds of milliseconds for a demo-sized session). Benefit: the
`.gtpack` shipped to the user is produced by the bundle-extension
contract, so a future Mode B WASM recipe drops in without touching the
designer.

Alternative considered: **extend `bundle-standard` to accept cards
directly**. Rejected for this phase — it requires embedding cards→flow
logic into `greentic-bundle`, which means `cards2pack-core` extraction
(Phase B). Deferred to its own spec.

### 7.1 Current flow (to augment)

1. `prepare_cards()` — temp dir + card rewriting (KEEP).
2. Spawn `greentic-cards2pack` subprocess into a workspace directory
   (KEEP). Today it also produces a `.gtpack` at the end; we will ignore
   that artifact under the new path.
3. `http_inject::inject_http_nodes()` — rewrites the generated `.ygtc` to
   add HTTP nodes back (KEEP).
4. Designer reads the workspace's `.gtpack` output (REPLACE with §7.2).

### 7.2 New flow

Replace step 4 only. Steps 1–3 are unchanged. After step 3 the workspace
directory contains modified `.ygtc` flow files. Introduce
`greentic-designer/src/orchestrate/session_adapter.rs`:

```rust
pub struct SessionPayload {
    pub session_json: String,   // serialized DesignerSession
    pub config_json: String,    // serialized StandardConfig
}

pub fn build_payload(
    workspace_dir: &Path,         // cards2pack output dir, has flows/*.ygtc
    cards_dir: &Path,             // from prepare_cards, card JSONs by id
    req: &PackBody,
    providers: &[ProviderRef],
) -> anyhow::Result<SessionPayload>;
```

Mapping from designer state to the two structs:

| Target field              | Source                                           |
|---------------------------|--------------------------------------------------|
| `session.flows_json`      | array of `{name, yaml}` — one per `.ygtc` file in `workspace_dir/flows/` (name = file stem, yaml = file contents, with HTTP-inject modifications already applied) |
| `session.contents_json`   | array of `{id, json}` — one per card JSON file in `cards_dir`, id = file stem |
| `session.assets`          | raw image uploads from the designer's asset store, not from cards2pack output |
| `session.capabilities_used` | inferred from `providers` + flow inspection    |
| `config.metadata.name`    | `req.name`                                       |
| `config.metadata.version` | `"0.1.0"` (fixed for this phase; designer UI lacks a version field today) |
| `config.channels`         | channel ids derived from `providers`             |
| `config.embed_ui`         | `"webchat"` when designer has webchat enabled, else `"none"` |
| `config.i18n.source`      | `"en"`                                           |
| `config.i18n.targets`     | `req.langs.unwrap_or_default()`                  |
| `config.format`           | `"gtpack-legacy"` (only supported value)         |

After `build_payload` returns, the designer spawns `greentic-bundle ext
render` (see §7.3) and treats the resulting `.gtpack` as the output of
the pack job. The `.gtpack` produced by cards2pack in step 2 is deleted
as part of temp cleanup.

### 7.3 Spawn

```rust
Command::new(bundle_bin)
    .arg("--extension-dir").arg(&ext_dir)
    .arg("ext").arg("render")
    .arg("greentic.bundle-standard").arg("standard")
    .arg("--config").arg("-")           // piped via stdin
    .arg("--session").arg(&session_path) // temp file
    .arg("--out").arg(&out_path)
    .arg("--json")
    .stdin(Stdio::piped())
    .stderr(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;
```

Write `config_json` to stdin, close. Stream stderr into `PackLogLine`.
Parse single-line stdout JSON for `filename` + `sha256` on exit 0. On
non-zero exit, parse stdout JSON for `code` + `message` and fail the
`PackJob`.

`--session` uses a temp file rather than stdin because only one of the
two can be stdin; config is smaller and a more natural fit. If the
designer needs to ship a large session payload later we can reverse the
choice.

### 7.4 Runtime probe + fallback

Add to `src/ui/state.rs`:

```rust
pub enum PackBackend {
    BundleExtRender { bundle_bin: PathBuf, bundle_version: String, ext_dir: PathBuf },
    LegacyCards2Pack,
}
```

Probe at designer startup (in `ui::mod.rs::run`):

1. Resolve `greentic-bundle` binary (env `GREENTIC_BUNDLE_BIN` → PATH).
   On miss → `LegacyCards2Pack`.
2. Run `greentic-bundle ext render --help`. Exit 0 and stdout contains
   `--json` (or `--session`, as a fallback marker) → `BundleExtRender`.
3. Perform bootstrap (§6.2) and set `ext_dir`.
4. Any failure → `LegacyCards2Pack`. Log the reason once at `warn`.

Cache on `AppState.pack_backend`. Log the choice at startup:

```
pack backend: BundleExtRender (greentic-bundle 0.x.y, ext_dir=…)
```

`orchestrate::cards2pack::run()` branches on this enum:

- `BundleExtRender` → chained flow: cards2pack (into a workspace dir,
  with `.gtpack` output ignored) → session adapter → `ext render`.
- `LegacyCards2Pack` → current flow: cards2pack produces the final
  `.gtpack` directly; no trailing `ext render` step.

### 7.5 `deploy?` flag

`PackBody.bundle` (deployer → `.gtbundle`) path is untouched. It runs
after the `.gtpack` is produced regardless of which backend built it.

## 8. Data flow diagram

```
UI POST /api/pack
    │
    ▼
prepare_cards (KEEP)                         ← existing
    │   (cards_dir/<id>.json, http_entries)
    ▼
spawn greentic-cards2pack (KEEP)             ← existing, cards → workspace
    │   (workspace/flows/*.ygtc, workspace/<legacy>.gtpack ignored)
    ▼
inject_http_nodes (KEEP)                     ← existing, rewrites .ygtc
    │
    ▼
session_adapter::build_payload (NEW)
    │   (cards_dir + workspace → session_json + config_json)
    │
    ▼                                  probe = BundleExtRender
spawn greentic-bundle                           │
      --extension-dir <bootstrapped ext_dir>    │
      ext render greentic.bundle-standard standard
      --config -   (stdin)                      │
      --session <temp>.json                     │
      --out <tmp>/out.gtpack                    │
      --json                                    │
    │                                           │ stderr stream
    │                                           ▼
    │                                     PackLogLine
    ▼
stdout: {status,filename,sha256,bytesLen}
    │
    ▼
PackJob.complete(pack_path = <tmp>/out.gtpack)
    │
    ▼
if body.bundle: deployer.build_bundle()      ← unchanged
```

## 9. Error handling

| Failure                          | Location        | Designer response                  |
|----------------------------------|-----------------|------------------------------------|
| `greentic-bundle` binary missing | startup probe   | fall back to `LegacyCards2Pack`    |
| `ext render --help` fails        | startup probe   | fall back to `LegacyCards2Pack`    |
| Bootstrap unpack fails           | startup         | fall back + warn                   |
| Dispatcher error (exit ≠ 0)      | render subprocess | fail job, surface `code`+`message` |
| Stdout JSON parse error          | render subprocess | fail job, include raw tail of stderr |
| Adapter error (malformed state)  | before spawn    | fail job with adapter-specific msg |

No retries — pack builds are idempotent on re-submit.

## 10. Testing

### 10.1 greentic-bundle

Add to `tests/ext_render.rs` (feature `extensions`):

- `--json` happy path: fixture descriptor dir + fixture config + session
  files → exit 0, stdout parses as expected JSON, `.gtpack` valid.
- `--json` error path: bogus recipe id → exit ≠ 0, stdout JSON has
  `status: "error"` + `code: "recipe-not-found"`.
- Stdin config: `--config -` pipes fixture JSON → happy path unchanged.
- Stdin session: `--session -` pipes fixture JSON → happy path.
- Double stdin rejected: `--config - --session -` → parse error,
  non-zero exit, stderr mentions "stdin".

### 10.2 greentic-designer

- `session_adapter::build_payload` unit tests covering each mapping in
  §7.2 with fixture inputs.
- Probe unit test with a mock binary via `assert_cmd`:
  - mock prints `--json` marker in `ext render --help` → `BundleExtRender`.
  - mock prints unrelated help → `LegacyCards2Pack`.
  - binary absent → `LegacyCards2Pack`.
- Route integration (`routes::pack`) with `PackBackend::BundleExtRender`
  using a stub `greentic-bundle` that echoes a known JSON line and writes
  a fixture `.gtpack` to `--out`. Assert args passed, stdout parsed,
  `PackJob.pack_path` populated.
- Log streaming: stub binary emits known stderr → assert those lines
  reach `PackJob.log_lines`.

### 10.3 End-to-end (manual)

1. `cargo install --path . --features extensions` on `greentic-bundle`
   (local).
2. `cargo run --bin greentic-designer ui` with the new code.
3. Build a pack via the UI using a demo (e.g., `demo-bundle`).
4. Compare manifest contents of the produced `.gtpack` with one built by
   the legacy path for the same inputs — expect equivalent flow list,
   card assets, and `bundle.yaml`. Byte-level identity not required
   because the two paths differ in workspace layout details.

## 11. Rollout

1. Land ergonomic additions (§5) in `greentic-bundle`. Patch release
   (new subcommand flags only, no breaking change).
2. Vendor `greentic.bundle-standard-0.1.0.gtxpack` into the designer repo
   with a pin script.
3. Land designer wiring (§6, §7) + tests on a feature branch.
4. Update both `CLAUDE.md` files: `greentic-bundle` notes the new
   flags + designer usage pattern; `greentic-designer` documents the
   bootstrap + probe + fallback.
5. Optional doc page in `greentic-docs` under "GTC CLI" once stabilized.
6. Legacy `greentic-cards2pack` path marked "deprecated but retained for
   backward compatibility" in designer docs.

## 12. Risks and mitigations

- **Vendored `.gtxpack` staleness**. Pinned SHA-256 + explicit bump
  script; CI check that the embedded bytes match the pin file.
- **Bootstrap cache collision across designer versions**. Cache key
  includes SHA-256 of embedded bytes, so different versions get
  different paths.
- **`StandardConfig` field gaps**. `metadata.version` fixed at `"0.1.0"`
  in this phase; adding a designer UI field to set it is a follow-up.
  Hard-coded value is acceptable for the current dev/demo workflow.
- **Session adapter drift from `DesignerSession` shape**. Single
  canonical Rust struct mirrored in the adapter; round-trip unit test
  serializing the adapter output and deserializing as
  `DesignerSession` against a checked-in fixture that mirrors the
  `handle_standard` test fixture.
- **Probe false-negative on older binaries**. Fallback silently goes
  back to legacy subprocess; startup log surfaces the decision.

## 13. Open questions

- **Pack metadata version**. UI currently has no version field; spec
  hard-codes `"0.1.0"`. Do we want a UI-sourced value before rollout
  or after? Lean: after (YAGNI for v1 of this wiring).
- **Channels**: what qualifies as a channel in `StandardConfig.channels`
  vs `capabilities_used` in `DesignerSession`? Needs adapter unit test
  spelling out the mapping; treat any provider from a fixed
  provider-id → channel-id table, unknown providers go to
  `capabilities_used` only.

## 14. Out-of-scope follow-ups

- Mode B WASM execution in `greentic-bundle/src/ext/wasm.rs`.
- Pure-Rust `bundle-core` / `cards2pack-core` extraction.
- Additional recipes (`hosted-webchat`, `openshift`,
  `docker-compose`, `multi-channel`).
- Retiring `greentic-cards2pack`.
- Direct wasmtime invoke from the designer (once Mode B ships), instead
  of subprocess.
