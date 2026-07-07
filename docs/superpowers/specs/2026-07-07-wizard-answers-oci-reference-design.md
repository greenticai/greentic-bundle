# Design: `gtc wizard --answers` accepts `oci://` / `ghcr://` references

- **Date:** 2026-07-07
- **Repo:** greentic-bundle
- **Status:** approved-by-directive (implement)

## Problem

`gtc wizard --answers <ref>` only reads a local file. The loader
`load_and_normalize_answers()` in `src/wizard/mod.rs` calls
`fs::read_to_string(path)` directly, with no scheme handling.

The greentic-demo README documents the launch flow as:

```
gtc wizard --answers oci://ghcr.io/greenticai/answers/<demo>/create:latest
```

and the greentic-demo publish pipeline (`publish_demo_answers_oci.sh`) already
pushes each answer document to GHCR as a raw-JSON OCI artifact:

```
oras push --artifact-type <media_type> \
  ghcr.io/greenticai/answers/<demo>/<create|setup>:<tag> \
  <answer_file>:<media_type>
```

with media types
`application/vnd.greentic.answers.create.v1+json` /
`application/vnd.greentic.answers.setup.v1+json`.

So the artifacts exist, but nothing consumes them — passing an `oci://` value to
`--answers` makes `fs::read_to_string` try to open a local file literally named
`oci://…` and fail with *"failed to read answers file"*.

## Goal

Make `--answers` transparently accept `oci://` and `ghcr://` references,
pulling the JSON answer document from the registry and feeding it into the
**existing** parse/normalize pipeline. Local paths keep their current behaviour
byte-for-byte.

Non-goals (YAGNI): `https://` answer URLs, private-registry auth for answers,
caching of pulled answer docs. Anonymous pulls only (the demo answers are public).

## Approach

Reuse the OCI machinery greentic-bundle already depends on:
`greentic_distributor_client`. The catalog loader (`src/catalog/client.rs`)
already pulls OCI artifacts via `OciPackFetcher` inside a fresh
`tokio::runtime::Runtime`. Answer artifacts are **not** pack-shaped, so instead
of `OciPackFetcher::fetch_pack_to_cache` (which validates pack layers), use the
lower-level `RegistryClient::pull(&Reference, &accepted_manifest_types)` which
returns `PulledImage { layers: Vec<PulledLayer { media_type, data }> }`.

### New module `src/wizard/answers_source.rs`

Keeps `wizard/mod.rs` (already ~5k lines) from growing further.

```rust
pub enum AnswersSource {
    Local(PathBuf),
    Remote { reference: String, bytes: Vec<u8> },
}

/// Scheme detection. `oci://` / `ghcr://` → Remote (pull); anything else → Local.
pub fn resolve_answers_source(raw: &str) -> Result<AnswersSource>;

/// `oci://X` → `X`; `ghcr://path[:tag]` → `ghcr.io/greenticai/path[:tag]`
/// (defaults to `:latest` when no tag/digest), mirroring the catalog client's
/// `ghcr://` shortcut. Pure function — unit-tested.
fn map_answers_reference(raw: &str) -> Result<String>;

/// Build a `Reference`, pull anonymously, return the JSON layer bytes.
/// Runs the async pull inside a fresh `tokio::runtime::Runtime`
/// (same pattern as `DistributorCatalogClient::fetch_catalog`).
fn pull_oci_answers<C: RegistryClient>(client: &C, oci_ref: &str) -> Result<Vec<u8>>;
```

**Layer selection** (`pull_oci_answers`): from `PulledImage.layers`, prefer a
layer whose `media_type` starts with `application/vnd.greentic.answers.` and ends
with `+json`; otherwise, if there is exactly one layer, use it; otherwise error
with the observed media types listed.

**Offline guard:** if `crate::runtime::offline()` is true, return a clear error
("cannot pull answers from `<ref>`: offline mode is enabled").

**Accepted manifest types:** OCI image manifest + OCI artifact manifest + docker
v2 manifest (ORAS pushes an OCI image manifest with `artifactType` set).

### `load_and_normalize_answers` refactor (`src/wizard/mod.rs`)

Signature stays the same (`path: &Path, …`). Internally:

1. `let source = resolve_answers_source(&path.to_string_lossy())?;`
2. Obtain `(text, base_dir)`:
   - `Local(p)` → `(fs::read_to_string(p)?, Some(answer_reference_base_dir(p)?))` — unchanged path.
   - `Remote { bytes, reference }` → `(String::from_utf8(bytes)?, None)`; remember `reference` for error messages.
3. Parse via the existing `parse_answer_document` → `normalized_request_from_document`.
4. Set `local_reference_base_dir = base_dir`.
5. **Remote relative-reference guard:** when `base_dir` is `None`, scan the
   request's references (`app_packs`, `app_pack_entries[].reference`,
   `extension_providers`) and bail if any is a local/relative kind
   (`local_file` / `local_dir` / `file_uri` / `unknown` per
   `detected_reference_kind`). Message:
   *"answers loaded from `<ref>` must use absolute references
   (https/oci/repo/store); found local reference `<x>`"*. The current published
   demo answers already use absolute `https://…/releases/latest/download/…`
   references, so this only rejects genuinely unusable input.

The CLI arg type (`Option<PathBuf>` / `PathBuf`) is unchanged — a `PathBuf`
holds the `oci://…` string fine; only the loader interprets it.

## Data flow

```
--answers "oci://ghcr.io/greenticai/answers/quickstart/create:latest"
      │
      ▼  resolve_answers_source()  ── starts_with oci://|ghcr:// ?
      │                                    │ no → Local(path) → fs::read_to_string (unchanged)
      │ yes
      ▼  map_answers_reference()  → "ghcr.io/greenticai/answers/quickstart/create:latest"
      ▼  RegistryClient::pull()   → PulledImage { layers }
      ▼  pick answers +json layer → Vec<u8> (the JSON doc)
      ▼  String::from_utf8 → parse_answer_document → NormalizedRequest (base_dir = None)
      ▼  relative-ref guard → execute_request  (existing path)
```

## Error handling

| Condition | Behaviour |
|-----------|-----------|
| Offline + remote ref | Clear error naming the ref, before any network call |
| Invalid reference syntax | `InvalidReference`-style error naming the ref |
| Registry pull fails (404 / network) | Propagated with the ref in context |
| No JSON layer / ambiguous layers | Error listing the observed media types |
| Non-UTF8 / invalid JSON blob | Reuses existing `errors.answer_document.invalid_json` |
| Remote doc with relative pack ref | Clear "must use absolute references" error |
| Local path (no scheme) | Unchanged — `fs::read_to_string` |

## Testing

- **Unit (pure):** `map_answers_reference` — `oci://` passthrough, `ghcr://`
  shortcut, default `:latest`, digest preserved, invalid input.
- **Unit (mock client):** a fake `RegistryClient` returning a crafted
  `PulledImage`; assert `pull_oci_answers` selects the JSON layer, handles
  single-layer, errors on zero/ambiguous layers.
- **Unit:** `resolve_answers_source` classifies `oci://` / `ghcr://` as Remote,
  local paths as Local.
- **Unit:** relative-reference guard rejects a remote doc carrying a bare
  `./x.gtpack` reference; accepts one with `https://…`.
- **Manual smoke** (documented, not in CI — needs network):
  `gtc wizard --answers oci://ghcr.io/greenticai/answers/quickstart/create:latest --dry-run`
  resolves and prints the plan.

## Follow-ups (out of scope)

- Same `oci://` support for `gtc setup --answers` (setup answers are also
  published to OCI) — mirror this once the wizard path is proven.
- Private-registry auth for answers (reuse `DefaultRegistryClient::with_basic_auth`).
