# Plan — Component embedding for designer-built packs

**Issue:** [greenticai/greentic-bundle#102](https://github.com/greenticai/greentic-bundle/issues/102)

**Owner:** TBD
**Status:** Park / not started
**Estimate:** 2–4 working days, depending on path chosen

## TL;DR for the next agent

Designer-rendered `.gtbundle`s currently fail at runtime with
`pack execution failed: component 'component.exec' not found in pack`
because `bundle-standard`'s WASM extension produces a "minimum-viable"
manifest with `components: []`, `flows: []`, no `.wasm` blobs. The
runtime expects the **canonical** shape demonstrated by
`deep-research-demo.gtpack`: `manifest.components[]` populated,
`symbols.component_ids[]` listing real component names, the actual
`.wasm` files shipped under `components/`.

Two fix paths are open. Pick **A** unless the team is committed to
extending the bundle-standard pipeline. Both are **multi-day**.

---

## Context — what already landed (do not redo)

Eight PRs across three repos shipped during the original autoStart
investigation. They unblock everything *upstream* of this issue but
none of them resolve it:

| Repo | PR | What it landed |
|------|-----|----------------|
| greenticai/greentic-bundle | #99 | topological `detect_entry` |
| greenticai/greentic-bundle | #100 | centrality pass for established flows |
| greenticai/greentic-bundle | #101 | emit `condition:` not `when:` (YGTC schema) |
| greentic-biz/greentic-bundle-extensions | #29 / #30 / #31 | `bundle-standard` 1.2.0→1.2.3 cascading the above |
| greentic-biz/greentic-designer | #220 / #221 / #223 | modal scroll fixes |
| greentic-biz/greentic-designer | #224 | walkthrough start-node fix |
| greentic-biz/greentic-designer | #222 / #225 / #226 | bundled manifest refresh chain |
| greentic-biz/greentic-designer | #227 | host post-process — populate `manifest.cbor.flows[]` from YGTC |

**With those merged, `manifest.cbor.flows[]` parses cleanly into the
canonical `Flow` struct via `greentic_flow::compile_ygtc_str`.**
The remaining gap is *components*.

## Reproduction

Need a designer-built `.gtbundle`. The one Bima used for the original
investigation lives in his Downloads folder; reproduce by:

1. Open the designer (`make dev-watch-all` from
   `greentic-biz/greentic-designer`).
2. Load any flow with at least one Adaptive Card and one HTTP node
   (e.g. the support-ticket-router fixture from `greentic-bundle`'s
   `tests/fixtures/support_ticket_router/cards.json`).
3. Click "Deploy" → "Local runtime" or use the export flow to write
   the `.gtbundle` to disk.
4. Run `gtc start <bundle>.gtbundle` — observe the
   `pack execution failed: component 'component.exec' not found in pack`
   error in the log right after `[ws pump] entering live loop`.

To inspect the manifest:

```bash
unsquashfs -d /tmp/ext "<bundle>.gtbundle"
unzip -q /tmp/ext/packs/*.gtpack -d /tmp/pack
python3 -c "import cbor2; d=cbor2.load(open('/tmp/pack/manifest.cbor','rb')); \
  print('components:', d['components']); \
  print('symbols.component_ids:', d['symbols']['component_ids'])"
```

A working pack (compare with `greentic-demo/demos/deep-research-demo-bundle.gtbundle`)
shows real component names; a designer-built pack shows
`["component.exec"]` and `components: []`.

## Path A — shell out to `packc build` (recommended)

### Why this is cleaner

`packc` (binary in `greentic-biz/greentic-pack`'s
`crates/packc/src/bin/`) is the canonical pack builder. It already
implements every piece this gap needs:
component fetching (`crates/packc/src/build.rs::resolve_component_artifacts`),
manifest assembly, signing, lockfile generation. Reimplementing this
in the designer would duplicate ~2k LOC. The demo pipeline already
uses it via `greentic-demo/scripts/package_demos.sh`.

### Implementation outline

#### Step 1 — make `packc` reachable from the designer host

Two sub-options:

- **Vendor the binary**: ship a `packc` binary alongside the
  designer's `gtc` distribution and locate it via `PATH` or a known
  path (`~/.greentic/bin/packc`).
- **`cargo install greentic-pack` once at startup**: the designer
  detects `packc` via `which packc`; if missing, runs
  `cargo install --locked greentic-pack` and caches the binary path.

The team's existing pattern (see `greentic-biz/greentic-bundle-extensions`
release pipeline using `cargo binstall`) suggests the binary is
already installable that way. Ship documentation that mentions the
prerequisite.

#### Step 2 — write a minimal pack source tree at render time

In `greentic-biz/greentic-designer/src/orchestrate/`, add a new module
(e.g. `pack_via_packc.rs`) that, given the designer session
(`AdapterInputs` from `session_adapter.rs`), writes a temp directory
matching `packc`'s expected pack layout:

```
$tmp/pack-source/
  pack.yaml            ← top-level manifest (name, version, components[], flows[])
  flows/
    main.ygtc          ← from cards2pack-core::convert (already there today)
  assets/
    cards/<id>.json    ← from session
  components/          ← (left empty; packc resolves via OCI/Store)
```

Generate `pack.yaml` declaring:

- `[package]` block — name, version, kind (`application`).
- `[[components]]` blocks — one per component the flow references.
  Determine these by walking the YGTC's nodes:
  - card nodes (`card.call.op: render`) → component
    `ai.greentic.component-adaptive-card` at the version pinned by
    the designer's bundled extensions manifest (`bundled/manifest.json`).
  - HTTP nodes (`component.exec.source`) → look up the OCI source
    from the YGTC's `bindings.url` style block, declare the
    component reference matching that source.
- `[[flows]]` — single entry referencing `flows/main.ygtc`.

Mirror the structure of `greentic-demo/crates/deep-research-demo/`'s
pack.yaml (it's checked in) for shape.

#### Step 3 — replace the `bundle-standard` render path

In `src/orchestrate/ext_render.rs::render`, before falling through to
`runtime.render_bundle(...)`:

- If the recipe is `standard` and the pack's components are
  resolvable via OCI (the common card-only case), short-circuit to
  the new `pack_via_packc::render(&inputs)` path.
- Run `packc build $tmp/pack-source --output $out_dir/pack.gtpack`
  via `tokio::process::Command`, with the existing
  `wait-timeout`-style guard to bound the subprocess.
- Pipe `packc`'s stderr into the existing `StderrSink` channel so
  the wizard surfaces packc's progress messages unchanged.

The host post-process step we already have
(`cbor_flow_post::populate_manifest_flows`) becomes a no-op when
`packc` is in charge — it can stay as a defensive fallback for any
codepath that still goes through `bundle-standard`.

#### Step 4 — wire the wizard / pack endpoints

`src/ui/routes/pack.rs` and `src/ui/routes/wizard_pipeline.rs`
already invoke `ext_render::render`. No changes there beyond the
new code path being chosen automatically.

#### Step 5 — test plan

- **Unit**: a `tests/pack_via_packc_smoke.rs` integration test that
  runs the designer fixture cards + YGTC through the new path and
  asserts the produced pack has non-empty `manifest.components[]`
  and the canonical Flow representation (integer ComponentId, not
  the `component.exec` shim).
- **Manual**: regenerate `support-ticket-router.gtbundle`, run
  `gtc start`, confirm the welcome card appears on first WebChat
  connect with no chat message needed.

### Risks

- `packc` evolves separately from designer; pinning the binary
  version is essential. Add a `--version` check at designer startup.
- `packc` needs network access to fetch components from OCI / the
  Greentic Store on first run. Confirm that fits the
  designer-deployment model (offline mode users may need to
  pre-populate the cache).

## Path B — extend the host post-process to fetch components

### When to pick this

Only if the team explicitly wants `bundle-standard` to remain the
canonical pack builder for card-only packs and is willing to
re-implement `packc::resolve_component_artifacts` host-side.

### Implementation outline

In `greentic-biz/greentic-designer/src/orchestrate/cbor_flow_post.rs`
(already added in PR #227 with the YGTC→CBOR flows[] step), extend
`populate_manifest_flows` to:

1. After compiling the YGTC into `Flow`, walk every node and
   collect the set of distinct `(operation, source)` pairs:
   - card nodes: `("card", None)` → resolves to the AC component.
   - HTTP nodes: `("component", Some("oci://..."))` → resolves to
     the OCI-referenced component.
2. For each pair, fetch the matching `.wasm` blob and
   `component.manifest.json`. Two source channels exist:
   - **Greentic Store** (preferred for `ai.greentic.*` components):
     the designer already has the URL in `bundled/manifest.json`'s
     `store_base_url`. Add a small fetcher that hits
     `<store>/<component>/<version>/component.wasm` etc.
   - **OCI registry** (`oci://ghcr.io/...`): use `oci-distribution`
     or shell out to `oras pull` (heavier but well-tested).
3. Embed each `.wasm` blob in the zip under `components/<id>.wasm`,
   the manifest under `components/<id>.manifest.cbor`.
4. Append `ComponentManifest` entries to the in-memory
   `PackManifest.components` and update
   `manifest.symbols.component_ids` to list the real names instead
   of `component.exec`.
5. Walk `Flow.nodes` and rewrite each `node.component.id` from the
   `component.exec` shim to the proper integer index referencing
   `symbols.component_ids[]`. Clear `node.component.operation` if
   the canonical encoding stores the operation elsewhere.
6. Re-encode `manifest.cbor` and rewrite the zip (the existing
   plumbing in `cbor_flow_post.rs` already handles this).

### Risks

- Replicates `packc`'s logic; drifts from `packc`'s guarantees over
  time.
- Component fetching needs an offline-friendly cache; OCI auth (for
  private registries) must be handled.
- Larger surface area — more code to maintain, more tests.

### Files to touch

- `src/orchestrate/cbor_flow_post.rs` (extend in-place)
- `src/orchestrate/component_fetcher.rs` (new — Store + OCI fetchers)
- `Cargo.toml` (add `oci-distribution` if going the in-process route)
- `tests/cbor_flow_post_smoke.rs` (extend to assert component
  embedding)

## Definition of done

A designer session containing at least one Adaptive Card and one
HTTP node renders to a `.gtbundle` that:

1. Has `manifest.cbor.components[]` non-empty with real
   `ComponentManifest` entries (not the `component.exec` shim).
2. Ships the corresponding `.wasm` blobs under `components/`.
3. Runs end-to-end via `gtc start`: WebChat session opens,
   autoStart fires the start node, the welcome card reaches the
   client without the user needing to send a chat message first.
4. The 8 already-merged fixes (PR #99 / #100 / #101 in
   greentic-bundle, the cascade in bundle-extensions, and the
   designer chain) remain unchanged.

## Out of scope

- Replacing `bundle-standard` for non-card packs (provider /
  deployer / library packs). The minimum-viable manifest those use
  may stay as-is until proven inadequate.
- Reorganising `cards2pack-core`'s public surface; it stays a
  YGTC emitter.
- LLM-based component inference (e.g. picking which AC component
  variant to ship based on the card content); use the version
  already pinned in `bundled/manifest.json`.

## Reading list before starting

- `greentic-bundle/crates/bundle-standard-core/src/workspace.rs:15-21`
  — the explicit "minimum-viable" comment that documents the
  current gap.
- `greentic-pack/crates/packc/src/build.rs::resolve_component_artifacts`
  — canonical component embedding logic.
- `greentic-pack/crates/packc/src/cli/mod.rs::BuildArgs` — CLI
  contract for the subprocess in path A.
- `greentic-demo/crates/deep-research-demo/` — full example of a
  pack source tree that produces a runnable `.gtpack`.
- `greentic-flow/src/lib.rs:154` — the `component.exec` shim
  emission rule that explains why designer-built packs have the
  current shape.
- `greentic-biz/greentic-designer/src/orchestrate/cbor_flow_post.rs`
  — already-landed CBOR flows[] post-process (the natural extension
  point for path B).
