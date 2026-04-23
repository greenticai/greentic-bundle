# Wave 2: Mode B WASM Execution Dispatcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `greentic-bundle/src/ext/wasm.rs` Mode B WASM execution dispatcher (currently stubbed `Err(ExtensionError::ModeBNotImplemented)`) so the host can instantiate `bundle-extension` WASM components via wasmtime, wire host imports, call `bundling.render(recipe_id, config_json, session)`, and return `RenderedArtifact`. This unblocks Wave 3 (bundle-standard 0.2.0 WASM ext).

**Architecture:**
- Add `greentic-ext-runtime` v0.3.0 as a Cargo dep — reuses canonical WASM loader (Engine, Component caching, ArcSwap hot-reload, signature verification, permission resolution).
- Vendor `greentic:extension-bundle@0.1.0` WIT files into `greentic-bundle/wit/` (copied from `greentic-bundle-extensions/wit/`). Generate host-side bindings via `wasmtime::component::bindgen!` macro for the bundle world.
- New trait `BundleWasmInvoker` (mirrors `WasmInvoker` pattern from `greentic-deployer/src/ext/wasm.rs`): production impl `WasmtimeBundleInvoker` wraps `Arc<ExtensionRuntime>`; test impl `MockBundleInvoker` returns canned bytes.
- `invoke_wasm()` in `src/ext/wasm.rs` becomes a thin shim that delegates to a process-singleton `BundleWasmInvoker` (lazy-init via `OnceLock`).
- Host imports (logging, i18n, broker, secrets, http) reuse ext-runtime's existing `HostState` impl — same wasmtime-43 `add_to_linker` pattern.

**Tech Stack:** Rust 1.91+ edition 2024, wasmtime v43 (component-model + async features), wit-bindgen 0.41, greentic-ext-runtime 0.3.0, greentic-ext-contract 0.3.0, anyhow.

**Out of scope:** bundle-standard 0.2.0 WASM rewrite (Wave 3), designer cleanup (Wave 4), builtin_bridge.rs deletion (Wave 5). The builtin path (`execution.kind="builtin"`) continues to work in parallel during Wave 2 — Mode B is additive.

**Spec reference:** `greentic-designer/docs/superpowers/specs/2026-04-23-cards2pack-removal-design.md`
**Wave 1 plan reference:** `docs/superpowers/plans/2026-04-23-wave1-pure-rust-cores-extract.md`

---

## File Structure

### New

```
greentic-bundle/
├── wit/
│   ├── extension-base.wit         vendored from greentic-bundle-extensions/wit/
│   ├── extension-bundle.wit       vendored
│   └── extension-host.wit         vendored
├── src/ext/
│   ├── wasm.rs                    REWRITTEN: thin shim + WasmInvocation/RenderedArtifact (existing types kept)
│   └── wasm/
│       ├── mod.rs                 module wiring + re-exports
│       ├── bindings.rs            wasmtime::component::bindgen! macro for bundle-extension world
│       ├── invoker.rs             BundleWasmInvoker trait + WasmtimeBundleInvoker impl
│       └── mock.rs                MockBundleInvoker for unit/integration tests
├── tests/
│   └── ext_wasm_mode_b.rs         integration test using MockBundleInvoker
└── tests/fixtures/
    └── dummy-bundle-ext/
        ├── describe.json          minimal BundleExtension descriptor
        └── extension.wasm         pre-built dummy WASM component (built by helper script in Task 9)
```

### Modified

- `Cargo.toml` — add `greentic-ext-runtime`, `greentic-ext-contract`, `wasmtime`, `anyhow` deps to top-level package + workspace.dependencies if needed.
- `src/ext/wasm.rs` — refactor: keep `WasmInvocation<'a>` + `RenderedArtifact` types; replace stubbed `invoke_wasm()` with delegating impl.
- `src/ext/mod.rs` — declare `pub mod wasm;` (already exists; verify after refactor).
- `src/ext/dispatcher.rs` — verify `execution.kind="wasm"` branch routes to `wasm::invoke_wasm()`. Likely no change needed (existing dispatcher already routes to wasm.rs::invoke_wasm; we just unstub the implementation).

### Deleted

None in Wave 2. `builtin_bridge.rs` deletion is Wave 5.

---

## Reference reading (skim before starting)

- `greentic-designer-extensions/crates/greentic-ext-runtime/src/runtime.rs` — `ExtensionRuntime` public API (lines 50-200)
- `greentic-designer-extensions/crates/greentic-ext-runtime/src/loaded.rs` — `LoadedExtension::load_from_dir` + `build_store_and_instance` (lines 30-100)
- `greentic-designer-extensions/crates/greentic-ext-runtime/src/host_state.rs` — `HostState` with logging/i18n/broker trait impls (lines 30-130)
- `greentic-designer-extensions/crates/greentic-ext-runtime/src/host_bindings.rs` — `bindgen!` macro usage example (lines 1-15)
- `greentic-deployer/src/ext/wasm.rs` — `WasmInvoker` trait + `WasmtimeInvoker` + `MockInvoker` patterns (lines 11-159) — **closest analog to what we are building**
- `greentic-bundle-extensions/wit/extension-bundle.wit` — the WIT world we are binding (lines 1-49)
- `greentic-bundle/src/ext/wasm.rs` — current 39-line stub (the file we are unstubbing)
- `greentic-bundle/src/ext/dispatcher.rs` — current dispatcher routing logic (verify integration point)

---

## PR3: Mode B WASM execution dispatcher

PR title: `feat: Mode B WASM execution dispatcher (Wave 2)`
Branch: `feat/mode-b-wasm-dispatcher`
Base: `main`

**Module convention**: each task creates files + adds appropriate `mod` declarations in `src/ext/wasm/mod.rs` so subsequent tests compile. Final wiring (replacement of `src/ext/wasm.rs` stub) happens in Task 8.

---

### Task 1: Scaffold branch + vendor WIT files

**Files:**
- Create: `wit/extension-base.wit` (copy from greentic-bundle-extensions)
- Create: `wit/extension-bundle.wit` (copy)
- Create: `wit/extension-host.wit` (copy)

- [ ] **Step 1: Create branch from latest main**

```bash
cd /home/bimbim/works/greentic/greentic-bundle
git checkout main
git pull
git checkout -b feat/mode-b-wasm-dispatcher
```

- [ ] **Step 2: Vendor WIT files**

```bash
mkdir -p wit
cp ../greentic-bundle-extensions/wit/extension-base.wit wit/
cp ../greentic-bundle-extensions/wit/extension-bundle.wit wit/
cp ../greentic-bundle-extensions/wit/extension-host.wit wit/
```

Verify:
```bash
ls wit/
# Expected: extension-base.wit  extension-bundle.wit  extension-host.wit
head -3 wit/extension-bundle.wit
# Expected: starts with `// Vendored from greentic-biz/greentic-designer-extensions`
```

- [ ] **Step 3: Add a brief WIT vendor README**

`wit/README.md`:

```markdown
# WIT vendor

These `.wit` files describe the `greentic:extension-bundle@0.1.0` world that
bundle extensions implement. Vendored from `greentic-bundle-extensions/wit/`
which itself vendors from `greentic-biz/greentic-designer-extensions`.

Refresh procedure:

```bash
cp ../greentic-bundle-extensions/wit/*.wit wit/
git diff wit/  # review for breaking changes
```

DO NOT edit these files directly — push schema changes upstream first.
```

- [ ] **Step 4: Commit**

```bash
git add wit/
git commit -m "feat(ext/wasm): vendor extension-bundle WIT files"
```

---

### Task 2: Add Cargo dependencies

**Files:**
- Modify: `Cargo.toml`

The `greentic-ext-runtime` and `greentic-ext-contract` crates live at relative path `../greentic-designer-extensions/crates/<name>`. Use path deps (workspace pattern matches `bundle-standard-core` from Wave 1).

- [ ] **Step 1: Add path dep entries to `[workspace.dependencies]`**

In `Cargo.toml`, append to the `[workspace.dependencies]` block:

```toml
greentic-ext-contract = { path = "../greentic-designer-extensions/crates/greentic-ext-contract" }
greentic-ext-runtime = { path = "../greentic-designer-extensions/crates/greentic-ext-runtime" }
wasmtime = { version = "43", default-features = false, features = ["component-model", "runtime", "cranelift", "cache"] }
```

Add under `[workspace.dependencies]` if not already present:
```toml
anyhow = "1"   # already exists per Wave 1, verify
```

- [ ] **Step 2: Add to top-level `[dependencies]` block**

```toml
greentic-ext-contract.workspace = true
greentic-ext-runtime.workspace = true
wasmtime.workspace = true
anyhow.workspace = true
```

- [ ] **Step 3: Verify compile (deps resolve)**

```bash
cargo check --no-default-features 2>&1 | tail -10
cargo check 2>&1 | tail -10
```

Expected: succeeds with no errors. Many warnings about wasmtime features acceptable.

If compile fails because `greentic-ext-runtime` isn't found, verify path: `ls ../greentic-designer-extensions/crates/greentic-ext-runtime/Cargo.toml`. If missing, STOP and report BLOCKED with directory listing.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat(ext/wasm): add greentic-ext-runtime + wasmtime 43 deps"
```

---

### Task 3: Create wasm module skeleton

**Files:**
- Create: `src/ext/wasm/mod.rs`
- Create: `src/ext/wasm/invoker.rs` (stub)
- Create: `src/ext/wasm/bindings.rs` (stub)
- Create: `src/ext/wasm/mock.rs` (stub)
- Modify: `src/ext/wasm.rs` (will be REPLACED by mod path; for now keep as-is, change in Task 8)

**Note**: Rust supports both `wasm.rs` AND `wasm/mod.rs` simultaneously? No — they conflict. We'll create the new `wasm/` directory with mod files first as orphan modules, NOT yet referenced from `src/ext/mod.rs`. In Task 8 we delete the old `wasm.rs` and rename the directory's `mod.rs` to take over.

To avoid this conflict during Wave 2 development, create the new files under a TEMPORARY namespace `src/ext/wasm_b/` first, then in Task 8 we rename `wasm_b/` → `wasm/` after deleting old `wasm.rs`.

- [ ] **Step 1: Create `src/ext/wasm_b/` directory + stub files**

```bash
mkdir -p src/ext/wasm_b
```

`src/ext/wasm_b/mod.rs`:

```rust
//! Mode B WASM execution dispatcher for bundle extensions.
//!
//! Wraps `greentic-ext-runtime` for loading + invocation of WASM bundle
//! extensions. Implements the `BundleWasmInvoker` trait with two impls:
//! `WasmtimeBundleInvoker` (production, backed by ext-runtime) and
//! `MockBundleInvoker` (tests).

mod bindings;
mod invoker;
mod mock;

pub use invoker::{BundleWasmInvoker, WasmtimeBundleInvoker};
pub use mock::MockBundleInvoker;
```

`src/ext/wasm_b/bindings.rs`:

```rust
//! wasmtime::component::bindgen! generated host bindings for the
//! `greentic:extension-bundle/bundle-extension` WIT world.
//!
//! Filled in Task 4.
```

`src/ext/wasm_b/invoker.rs`:

```rust
//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.
//!
//! Filled in Tasks 5 + 7.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};

/// Trait abstracting WASM invocation — enables test injection via MockBundleInvoker.
pub trait BundleWasmInvoker: Send + Sync {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError>;
}

/// Production impl placeholder. Implementation arrives in Task 7.
pub struct WasmtimeBundleInvoker;

impl WasmtimeBundleInvoker {
    pub fn new() -> Result<Self, ExtensionError> {
        Ok(Self)
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented) // unstubbed in Task 7
    }
}
```

`src/ext/wasm_b/mock.rs`:

```rust
//! MockBundleInvoker for unit/integration tests.
//!
//! Filled in Task 6.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::BundleWasmInvoker;

/// Test invoker placeholder; populated in Task 6.
#[derive(Default)]
pub struct MockBundleInvoker;

impl BundleWasmInvoker for MockBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented)
    }
}
```

- [ ] **Step 2: Wire into `src/ext/mod.rs`**

Read `src/ext/mod.rs`. Add:

```rust
pub mod wasm_b;
```

(Keep existing `pub mod wasm;` — both coexist during Wave 2; merged in Task 8.)

- [ ] **Step 3: Verify compile**

```bash
cargo check 2>&1 | tail -5
```

Expected: succeeds (everything is stubs returning `ModeBNotImplemented`).

- [ ] **Step 4: Commit**

```bash
git add src/ext/wasm_b/ src/ext/mod.rs
git commit -m "feat(ext/wasm): scaffold wasm_b module (BundleWasmInvoker trait + stubs)"
```

---

### Task 4: bindgen! macro for bundle-extension world

**Files:**
- Modify: `src/ext/wasm_b/bindings.rs`

Use `wasmtime::component::bindgen!` to generate host-side bindings for the `bundle-extension` WIT world. Configure async + interface paths.

- [ ] **Step 1: Write bindings module**

`src/ext/wasm_b/bindings.rs`:

```rust
//! wasmtime::component::bindgen! generated host bindings for the
//! `greentic:extension-bundle/bundle-extension` WIT world.
//!
//! The macro generates:
//! - `BundleExtension` struct: instantiation entry point with typed export accessors
//! - `add_to_linker` helpers for each imported interface
//!
//! Imports we wire (delegated to greentic-ext-runtime's HostState impl):
//! - greentic:extension-base/types
//! - greentic:extension-host/logging
//! - greentic:extension-host/i18n
//! - greentic:extension-host/broker

#![allow(clippy::too_many_arguments)]

wasmtime::component::bindgen!({
    path: "wit",
    world: "greentic:extension-bundle/bundle-extension",
    async: false,
    trappable_imports: true,
});
```

- [ ] **Step 2: Verify bindgen succeeds**

```bash
cargo check 2>&1 | tail -10
```

Expected: succeeds. The macro reads `wit/` directory, finds the world, generates code at compile time. If wit-bindgen complains about missing dependencies, verify `wit/` has all 3 vendored files (`extension-base.wit`, `extension-bundle.wit`, `extension-host.wit`).

If error mentions WIT dep paths, the WIT files may need to be in subdirs (`wit/deps/<name>/<name>.wit`). Try this layout:

```bash
mkdir -p wit/deps/extension-base wit/deps/extension-host
mv wit/extension-base.wit wit/deps/extension-base/extension-base.wit
mv wit/extension-host.wit wit/deps/extension-host/extension-host.wit
# Keep wit/extension-bundle.wit at top level (the world definition)
cargo check
```

This mirrors the `cargo-component` convention and is also used in `greentic-bundle-extensions/wit/`.

- [ ] **Step 3: Commit**

```bash
git add src/ext/wasm_b/bindings.rs wit/
git commit -m "feat(ext/wasm): bindgen! for bundle-extension WIT world"
```

---

### Task 5: BundleWasmInvoker trait + RuntimeContext

The trait is defined in Task 3. This task expands `WasmtimeBundleInvoker` with the `Arc<ExtensionRuntime>` field + constructor that registers extension dirs.

**Files:**
- Modify: `src/ext/wasm_b/invoker.rs`

- [ ] **Step 1: Replace invoker.rs with the constructor + trait impl scaffolding**

`src/ext/wasm_b/invoker.rs`:

```rust
//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use std::path::PathBuf;
use std::sync::Arc;

/// Trait abstracting WASM invocation — enables test injection via MockBundleInvoker.
pub trait BundleWasmInvoker: Send + Sync {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError>;
}

/// Production impl: wraps greentic-ext-runtime's ExtensionRuntime.
pub struct WasmtimeBundleInvoker {
    runtime: Arc<greentic_ext_runtime::ExtensionRuntime>,
}

impl WasmtimeBundleInvoker {
    /// Construct from an iterator of bundle-extension directories.
    /// Each dir must contain `describe.json` + the WASM component file.
    pub fn from_ext_dirs(ext_dirs: &[PathBuf]) -> Result<Self, ExtensionError> {
        let user_path = ext_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let paths = greentic_ext_runtime::DiscoveryPaths::new(user_path);
        let config = greentic_ext_runtime::RuntimeConfig::from_paths(paths);
        let mut runtime = greentic_ext_runtime::ExtensionRuntime::new(config)
            .map_err(|e| ExtensionError::Internal(format!("ext-runtime init: {e}")))?;

        for d in ext_dirs {
            runtime
                .register_loaded_from_dir(d)
                .map_err(|e| ExtensionError::Internal(format!("register {d:?}: {e}")))?;
        }

        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Internal accessor for invoke() impl in Task 7.
    pub(crate) fn runtime(&self) -> &Arc<greentic_ext_runtime::ExtensionRuntime> {
        &self.runtime
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        // Real impl arrives in Task 7. For now still stubbed.
        Err(ExtensionError::ModeBNotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ext_dirs_handles_empty() {
        // Empty dirs → still constructs (just zero loaded extensions).
        let invoker = WasmtimeBundleInvoker::from_ext_dirs(&[]);
        assert!(invoker.is_ok(), "empty dirs should construct: {invoker:?}");
        let _ = invoker;
    }
}
```

- [ ] **Step 2: Run test**

```bash
cargo test -p greentic-bundle ext::wasm_b::invoker 2>&1 | tail -10
```

Expected: 1 PASS.

If `RuntimeConfig::from_paths` doesn't exist on the actual ext-runtime version, look up the real constructor signature in `greentic-designer-extensions/crates/greentic-ext-runtime/src/runtime.rs` and adapt.

- [ ] **Step 3: Commit**

```bash
git add src/ext/wasm_b/invoker.rs
git commit -m "feat(ext/wasm): WasmtimeBundleInvoker constructor + ExtensionRuntime wiring"
```

---

### Task 6: MockBundleInvoker for tests

**Files:**
- Modify: `src/ext/wasm_b/mock.rs`

Mock returns canned `RenderedArtifact` keyed by `(extension_id, recipe_id)` pair. Lets tests verify dispatcher → invoker routing without real WASM.

- [ ] **Step 1: Write mock impl**

`src/ext/wasm_b/mock.rs`:

```rust
//! MockBundleInvoker for unit/integration tests.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::BundleWasmInvoker;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock that returns pre-populated artifacts (or errors) keyed by (extension_id, recipe_id).
#[derive(Default)]
pub struct MockBundleInvoker {
    responses: Mutex<HashMap<(String, String), Result<RenderedArtifact, ExtensionError>>>,
    /// Calls captured for assertion in tests.
    pub call_log: Mutex<Vec<(String, String, String)>>, // (ext_id, recipe_id, config_json)
}

impl MockBundleInvoker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect_render(
        &self,
        extension_id: &str,
        recipe_id: &str,
        result: Result<RenderedArtifact, ExtensionError>,
    ) {
        let mut responses = self.responses.lock().unwrap();
        responses.insert((extension_id.to_owned(), recipe_id.to_owned()), result);
    }
}

impl BundleWasmInvoker for MockBundleInvoker {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        let key = (call.extension_id.to_owned(), call.recipe_id.to_owned());
        {
            let mut log = self.call_log.lock().unwrap();
            log.push((
                call.extension_id.to_owned(),
                call.recipe_id.to_owned(),
                call.config_json.to_owned(),
            ));
        }
        let mut responses = self.responses.lock().unwrap();
        match responses.remove(&key) {
            Some(r) => r,
            None => Err(ExtensionError::Internal(format!(
                "MockBundleInvoker has no response for ({}, {})",
                key.0, key.1
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_artifact() -> RenderedArtifact {
        RenderedArtifact {
            filename: "demo.gtpack".into(),
            bytes: b"PK\x03\x04".to_vec(), // ZIP magic
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn returns_canned_response() {
        let mock = MockBundleInvoker::new();
        mock.expect_render("ext.x", "standard", Ok(dummy_artifact()));
        let r = mock
            .invoke(WasmInvocation {
                extension_id: "ext.x",
                recipe_id: "standard",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap();
        assert_eq!(r.filename, "demo.gtpack");
    }

    #[test]
    fn captures_call_log() {
        let mock = MockBundleInvoker::new();
        mock.expect_render("ext.x", "standard", Ok(dummy_artifact()));
        mock.invoke(WasmInvocation {
            extension_id: "ext.x",
            recipe_id: "standard",
            config_json: "{\"foo\":1}",
            session_json: "{}",
        })
        .unwrap();
        let log = mock.call_log.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].2, "{\"foo\":1}");
    }

    #[test]
    fn errors_on_unknown_key() {
        let mock = MockBundleInvoker::new();
        let err = mock
            .invoke(WasmInvocation {
                extension_id: "missing",
                recipe_id: "x",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap_err();
        assert!(matches!(err, ExtensionError::Internal(_)));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p greentic-bundle ext::wasm_b::mock 2>&1 | tail -10
```

Expected: 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add src/ext/wasm_b/mock.rs
git commit -m "feat(ext/wasm): MockBundleInvoker with expect_render + call_log"
```

---

### Task 7: WasmtimeBundleInvoker::invoke() — real implementation

This is the biggest task. Calls into ext-runtime's `LoadedExtension::build_store_and_instance`, then uses our `bindgen!`-generated `BundleExtension` interface to call `bundling.render`.

**Files:**
- Modify: `src/ext/wasm_b/invoker.rs`

- [ ] **Step 1: Read ext-runtime's LoadedExtension to confirm API**

```bash
grep -n "build_store_and_instance\|pub fn\|pub struct" ../greentic-designer-extensions/crates/greentic-ext-runtime/src/loaded.rs | head -30
```

Confirm `build_store_and_instance` signature. Likely `pub fn build_store_and_instance(&self, engine: &Engine) -> Result<(Store<HostState>, Instance), Error>` — if signature differs, adapt the code below.

- [ ] **Step 2: Replace invoker.rs with full impl**

`src/ext/wasm_b/invoker.rs`:

```rust
//! BundleWasmInvoker trait + WasmtimeBundleInvoker production impl.

use crate::ext::errors::ExtensionError;
use crate::ext::wasm::{RenderedArtifact, WasmInvocation};
use crate::ext::wasm_b::bindings::greentic::extension_bundle::bundling::DesignerSession;
use crate::ext::wasm_b::bindings::BundleExtension;
use std::path::PathBuf;
use std::sync::Arc;

/// Trait abstracting WASM invocation — enables test injection via MockBundleInvoker.
pub trait BundleWasmInvoker: Send + Sync {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError>;
}

/// Production impl: wraps greentic-ext-runtime's ExtensionRuntime.
pub struct WasmtimeBundleInvoker {
    runtime: Arc<greentic_ext_runtime::ExtensionRuntime>,
}

impl WasmtimeBundleInvoker {
    pub fn from_ext_dirs(ext_dirs: &[PathBuf]) -> Result<Self, ExtensionError> {
        let user_path = ext_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let paths = greentic_ext_runtime::DiscoveryPaths::new(user_path);
        let config = greentic_ext_runtime::RuntimeConfig::from_paths(paths);
        let mut runtime = greentic_ext_runtime::ExtensionRuntime::new(config)
            .map_err(|e| ExtensionError::Internal(format!("ext-runtime init: {e}")))?;

        for d in ext_dirs {
            runtime
                .register_loaded_from_dir(d)
                .map_err(|e| ExtensionError::Internal(format!("register {d:?}: {e}")))?;
        }

        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    pub(crate) fn runtime(&self) -> &Arc<greentic_ext_runtime::ExtensionRuntime> {
        &self.runtime
    }
}

impl BundleWasmInvoker for WasmtimeBundleInvoker {
    fn invoke(&self, call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        // 1. Look up loaded extension by id.
        let loaded_map = self.runtime.loaded();
        let ext_id = greentic_ext_contract::ExtensionId::from(call.extension_id.to_string());
        let loaded = loaded_map
            .get(&ext_id)
            .ok_or_else(|| ExtensionError::NotFound(format!("extension {}", call.extension_id)))?;

        // 2. Build a fresh store + instance via ext-runtime.
        let (mut store, instance) = loaded
            .build_store_and_instance(self.runtime.engine())
            .map_err(|e| ExtensionError::Internal(format!("instantiate: {e}")))?;

        // 3. Get our BundleExtension typed bindings (separate from ext-runtime's design-side).
        // Note: bindgen! generated `BundleExtension::new(&mut store, &instance)` lifts the raw
        // wasmtime Instance into our typed view.
        let bindings = BundleExtension::new(&mut store, &instance)
            .map_err(|e| ExtensionError::Internal(format!("bindings: {e}")))?;

        // 4. Convert WasmInvocation → DesignerSession (parse session_json).
        let session: DesignerSession = parse_designer_session(call.session_json)
            .map_err(|e| ExtensionError::InvalidConfig(format!("session_json: {e}")))?;

        // 5. Call bundling.render(recipe_id, config_json, session).
        let bundling = bindings.greentic_extension_bundle_bundling();
        let result = bundling
            .call_render(
                &mut store,
                call.recipe_id,
                call.config_json,
                &session,
            )
            .map_err(|e| ExtensionError::Internal(format!("call_render trap: {e}")))?;

        // 6. Map WIT Result<bundle-artifact, extension-error> → ExtensionError.
        match result {
            Ok(artifact) => Ok(RenderedArtifact {
                filename: artifact.filename,
                bytes: artifact.bytes,
                sha256: artifact.sha256,
            }),
            Err(ext_err) => Err(map_extension_error(ext_err)),
        }
    }
}

fn parse_designer_session(json: &str) -> Result<DesignerSession, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        flows_json: String,
        #[serde(default)]
        contents_json: String,
        #[serde(default)]
        assets: Vec<(String, Vec<u8>)>,
        #[serde(default)]
        capabilities_used: Vec<String>,
    }
    let raw: Raw = serde_json::from_str(json)?;
    Ok(DesignerSession {
        flows_json: raw.flows_json,
        contents_json: raw.contents_json,
        assets: raw.assets,
        capabilities_used: raw.capabilities_used,
    })
}

fn map_extension_error(
    ext_err: crate::ext::wasm_b::bindings::greentic::extension_base::types::ExtensionError,
) -> ExtensionError {
    use crate::ext::wasm_b::bindings::greentic::extension_base::types::ExtensionError as Wit;
    match ext_err {
        Wit::InvalidInput(msg) => ExtensionError::InvalidConfig(msg),
        Wit::NotFound(msg) => ExtensionError::NotFound(msg),
        Wit::Unauthorized(msg) => ExtensionError::Internal(format!("unauthorized: {msg}")),
        Wit::Unavailable(msg) => ExtensionError::Internal(format!("unavailable: {msg}")),
        Wit::Internal(msg) => ExtensionError::Internal(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ext_dirs_handles_empty() {
        let invoker = WasmtimeBundleInvoker::from_ext_dirs(&[]);
        assert!(invoker.is_ok(), "empty dirs should construct");
    }

    #[test]
    fn invoke_unknown_extension_returns_not_found() {
        let invoker = WasmtimeBundleInvoker::from_ext_dirs(&[]).unwrap();
        let err = invoker
            .invoke(WasmInvocation {
                extension_id: "ghost",
                recipe_id: "standard",
                config_json: "{}",
                session_json: "{}",
            })
            .unwrap_err();
        assert!(matches!(err, ExtensionError::NotFound(_)));
    }
}
```

**Important note**: the actual struct/method names from `bindgen!` depend on the WIT world. The names above (`BundleExtension::new`, `greentic_extension_bundle_bundling`, `call_render`) are predictions. After Step 2, run `cargo check` and adjust naming based on actual generated code. To inspect generated bindings:

```bash
cargo expand -p greentic-bundle ext::wasm_b::bindings 2>&1 | head -200
```

(Install `cargo-expand` first: `cargo install cargo-expand --locked`.)

If `ExtensionError` enum variants differ (e.g. `NotFound` doesn't exist in our local enum), check `src/ext/errors.rs` for the actual variants and adapt the mapping. Add new variants if needed (not in scope for Wave 2 — extend conservatively).

- [ ] **Step 3: Verify compile + tests**

```bash
cargo build -p greentic-bundle 2>&1 | tail -10
cargo test -p greentic-bundle ext::wasm_b 2>&1 | tail -10
```

Expected: build clean, 2 tests PASS (`from_ext_dirs_handles_empty`, `invoke_unknown_extension_returns_not_found`).

If `NotFound` variant doesn't exist on `ExtensionError`, check existing variants in `src/ext/errors.rs` and either:
- Use `Internal(...)` as fallback (simpler — slightly less precise)
- Add `NotFound(String)` variant to the enum (1-line change in errors.rs + match arm in errors.rs `code()` if present)

Choose the simpler option (use Internal) unless a separate test asserts the NotFound semantic.

- [ ] **Step 4: Commit**

```bash
git add src/ext/wasm_b/invoker.rs src/ext/errors.rs
git commit -m "feat(ext/wasm): WasmtimeBundleInvoker.invoke() full impl via ext-runtime"
```

---

### Task 8: Replace src/ext/wasm.rs stub + integrate dispatcher

Now consolidate: delete the old `src/ext/wasm.rs` stub file, rename `src/ext/wasm_b/` → `src/ext/wasm/`, update `src/ext/mod.rs`, ensure `dispatcher.rs` routes wasm invocations through the new path.

**Files:**
- Delete: `src/ext/wasm.rs` (the old stub)
- Rename: `src/ext/wasm_b/` → `src/ext/wasm/`
- Modify: `src/ext/mod.rs`
- Modify: `src/ext/dispatcher.rs` (verify only; likely no change)

- [ ] **Step 1: Read current `src/ext/wasm.rs` to extract WasmInvocation + RenderedArtifact types**

```bash
cat src/ext/wasm.rs
```

The struct definitions (`WasmInvocation<'a>`, `RenderedArtifact`) need to live SOMEWHERE. Options:
- (A) Move them into `src/ext/wasm/mod.rs` after rename
- (B) Move them into `src/ext/wasm/types.rs` (new file)

Choose (A) for simplicity (small types, single file fine).

- [ ] **Step 2: Delete old wasm.rs**

```bash
git rm src/ext/wasm.rs
```

- [ ] **Step 3: Rename wasm_b → wasm**

```bash
git mv src/ext/wasm_b src/ext/wasm
```

Verify the rename:
```bash
ls src/ext/wasm/
# Expected: bindings.rs  invoker.rs  mock.rs  mod.rs
```

- [ ] **Step 4: Update `src/ext/wasm/mod.rs` with type definitions + invoke_wasm() shim**

`src/ext/wasm/mod.rs`:

```rust
//! Mode B WASM execution dispatcher for bundle extensions.
//!
//! Wraps `greentic-ext-runtime` for loading + invocation of WASM bundle
//! extensions. Implements the `BundleWasmInvoker` trait with two impls:
//! `WasmtimeBundleInvoker` (production, backed by ext-runtime) and
//! `MockBundleInvoker` (tests).

mod bindings;
mod invoker;
mod mock;

pub use invoker::{BundleWasmInvoker, WasmtimeBundleInvoker};
pub use mock::MockBundleInvoker;

use crate::ext::errors::ExtensionError;

/// Invocation parameters passed across the host-WASM boundary.
pub struct WasmInvocation<'a> {
    pub extension_id: &'a str,
    pub recipe_id: &'a str,
    pub config_json: &'a str,
    pub session_json: &'a str,
}

/// The artifact returned by `bundling.render`.
#[derive(Debug, Clone)]
pub struct RenderedArtifact {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

/// Process-wide singleton invoker. Initialized lazily on first call.
///
/// In production, `init_invoker_from_env()` should be called early (e.g. from
/// CLI startup) so first invocation doesn't pay init cost. Tests can inject a
/// `MockBundleInvoker` via `set_invoker_for_test()`.
static INVOKER: std::sync::OnceLock<Box<dyn BundleWasmInvoker>> = std::sync::OnceLock::new();

/// Install a custom invoker (test injection point).
/// Returns Err if invoker was already set.
pub fn set_invoker(invoker: Box<dyn BundleWasmInvoker>) -> Result<(), &'static str> {
    INVOKER
        .set(invoker)
        .map_err(|_| "invoker already set; tests must run sequentially")
}

/// Default invoker constructor: read `GREENTIC_BUNDLE_EXT_DIR` env (or default
/// `~/.greentic/extensions/bundle/`), enumerate ext dirs, build WasmtimeBundleInvoker.
fn default_invoker() -> Box<dyn BundleWasmInvoker> {
    let ext_root = std::env::var("GREENTIC_BUNDLE_EXT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = dirs::home_dir().unwrap_or_default();
            p.push(".greentic/extensions/bundle");
            p
        });

    let ext_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&ext_root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("describe.json").exists())
                .collect()
        })
        .unwrap_or_default();

    match WasmtimeBundleInvoker::from_ext_dirs(&ext_dirs) {
        Ok(inv) => Box::new(inv),
        Err(e) => {
            tracing::error!(error = %e, "failed to init WasmtimeBundleInvoker; using stub");
            Box::new(StubInvoker)
        }
    }
}

/// Fallback invoker that returns ModeBNotImplemented. Used only if
/// WasmtimeBundleInvoker init fails.
struct StubInvoker;
impl BundleWasmInvoker for StubInvoker {
    fn invoke(&self, _call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
        Err(ExtensionError::ModeBNotImplemented)
    }
}

/// Public dispatcher entry point: routes Mode B invocations to the configured invoker.
pub fn invoke_wasm(call: WasmInvocation<'_>) -> Result<RenderedArtifact, ExtensionError> {
    let invoker = INVOKER.get_or_init(default_invoker);
    invoker.invoke(call)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: tests can't safely set the OnceLock invoker concurrently. Each
    // integration test that needs invoker injection runs in its own process
    // (separate test binary file). See tests/ext_wasm_mode_b.rs for the
    // pattern.

    #[test]
    fn types_construct() {
        let _ = WasmInvocation {
            extension_id: "x",
            recipe_id: "y",
            config_json: "{}",
            session_json: "{}",
        };
        let _ = RenderedArtifact {
            filename: "x.gtpack".into(),
            bytes: vec![],
            sha256: "0".repeat(64),
        };
    }
}
```

Add `dirs = "6"` to top-level `[dependencies]` in `Cargo.toml` if not present.

- [ ] **Step 5: Update `src/ext/mod.rs`**

`src/ext/mod.rs` becomes (remove the temporary `wasm_b` reference):

```rust
// existing modules untouched
pub mod builtin_bridge;
pub mod describe;
pub mod dispatcher;
pub mod errors;
pub mod loader;
pub mod registry;
pub mod wasm;
```

(Verify by reading current file first; structure should already be like this minus the `wasm_b` line we added in Task 3 — remove that line.)

- [ ] **Step 6: Verify dispatcher routes correctly**

```bash
grep -A 10 "execution.kind\|Wasm\|invoke_wasm" src/ext/dispatcher.rs | head -30
```

The dispatcher should already call `wasm::invoke_wasm()` when execution.kind=wasm. If it does, NO CHANGE NEEDED. If it short-circuits to error, update to call invoke_wasm.

- [ ] **Step 7: Verify build + all tests pass**

```bash
cargo build -p greentic-bundle 2>&1 | tail -5
cargo test -p greentic-bundle ext::wasm 2>&1 | tail -10
cargo test -p greentic-bundle 2>&1 | tail -10  # full suite, no regression
```

Expected: build clean, ext::wasm tests PASS (5 from invoker + 3 from mock + 1 from mod tests = 9), no other regression.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(ext/wasm): consolidate wasm_b into wasm; expose invoke_wasm shim"
```

---

### Task 9: Dummy WASM bundle-extension fixture

For Task 10's integration test, we need a real WASM component that implements the bundle-extension WIT world. Build a minimal one in `tests/fixtures/dummy-bundle-ext/`.

**Files:**
- Create: `tests/fixtures/dummy-bundle-ext/Cargo.toml`
- Create: `tests/fixtures/dummy-bundle-ext/src/lib.rs`
- Create: `tests/fixtures/dummy-bundle-ext/build.sh`
- Create: `tests/fixtures/dummy-bundle-ext/describe.json` (after build)
- Create: `tests/fixtures/dummy-bundle-ext/extension.wasm` (build artifact)

The fixture is a separate cargo crate (NOT in workspace.members) that builds to `wasm32-wasip2` via `cargo component build`.

- [ ] **Step 1: Scaffold dummy ext crate**

```bash
mkdir -p tests/fixtures/dummy-bundle-ext/src
```

`tests/fixtures/dummy-bundle-ext/Cargo.toml`:

```toml
[package]
name = "dummy-bundle-ext"
version = "0.0.1"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen-rt = { version = "0.41", features = ["bitflags"] }

[package.metadata.component]
package = "greentic:dummy-bundle-ext"

[package.metadata.component.target]
path = "../../../wit"
world = "greentic:extension-bundle/bundle-extension"
```

- [ ] **Step 2: Write minimal lib.rs that implements bundle-extension exports**

`tests/fixtures/dummy-bundle-ext/src/lib.rs`:

```rust
//! Minimal dummy bundle extension for greentic-bundle integration tests.
//! Returns canned bytes for any render() call.

#[allow(warnings)]
mod bindings;

use bindings::exports::greentic::extension_base::lifecycle;
use bindings::exports::greentic::extension_base::manifest;
use bindings::exports::greentic::extension_bundle::bundling;
use bindings::exports::greentic::extension_bundle::recipes;
use bindings::greentic::extension_base::types;

const DUMMY_BYTES: &[u8] = b"PK\x03\x04dummy-pack-bytes";
const DUMMY_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

struct Component;

impl manifest::Guest for Component {
    fn get_identity() -> types::ExtensionIdentity {
        types::ExtensionIdentity {
            id: "greentic.dummy-bundle-ext".into(),
            version: "0.0.1".into(),
            kind: types::Kind::Bundle,
        }
    }
    fn get_offered() -> Vec<types::CapabilityRef> { Vec::new() }
    fn get_required() -> Vec<types::CapabilityRef> { Vec::new() }
}

impl lifecycle::Guest for Component {
    fn init(_config_json: String) -> Result<(), types::ExtensionError> { Ok(()) }
    fn shutdown() {}
}

impl recipes::Guest for Component {
    fn list_recipes() -> Vec<recipes::RecipeSummary> {
        vec![recipes::RecipeSummary {
            id: "dummy".into(),
            display_name: "Dummy".into(),
            description: "Test fixture recipe".into(),
            icon_path: None,
        }]
    }
    fn recipe_config_schema(_recipe_id: String) -> Result<String, types::ExtensionError> {
        Ok("{}".into())
    }
    fn supported_capabilities(_recipe_id: String) -> Result<Vec<String>, types::ExtensionError> {
        Ok(Vec::new())
    }
}

impl bundling::Guest for Component {
    fn validate_config(_recipe_id: String, _config_json: String) -> Vec<types::Diagnostic> {
        Vec::new()
    }
    fn render(
        _recipe_id: String,
        _config_json: String,
        _session: bundling::DesignerSession,
    ) -> Result<bundling::BundleArtifact, types::ExtensionError> {
        Ok(bundling::BundleArtifact {
            filename: "dummy-0.0.1.gtpack".into(),
            bytes: DUMMY_BYTES.to_vec(),
            sha256: DUMMY_SHA.into(),
        })
    }
}

bindings::export!(Component with_types_in bindings);
```

- [ ] **Step 3: Write build script**

`tests/fixtures/dummy-bundle-ext/build.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo-component >/dev/null || cargo install cargo-component --locked
cargo component build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/dummy_bundle_ext.wasm extension.wasm
echo "Built: $(pwd)/extension.wasm ($(wc -c < extension.wasm) bytes)"
```

```bash
chmod +x tests/fixtures/dummy-bundle-ext/build.sh
```

- [ ] **Step 4: Write describe.json**

`tests/fixtures/dummy-bundle-ext/describe.json`:

```json
{
  "id": "greentic.dummy-bundle-ext",
  "version": "0.0.1",
  "kind": "BundleExtension",
  "execution": { "kind": "wasm" },
  "runtime": {
    "component": "extension.wasm",
    "wit_world": "greentic:extension-bundle/bundle-extension"
  },
  "permissions": {
    "network": [],
    "filesystem": [],
    "secrets": [],
    "call_extension_kinds": []
  }
}
```

- [ ] **Step 5: Build the fixture once + commit binary**

```bash
bash tests/fixtures/dummy-bundle-ext/build.sh
ls -la tests/fixtures/dummy-bundle-ext/extension.wasm
```

Expected: file exists, size > 0.

- [ ] **Step 6: Add `.gitignore` for build artifacts (NOT extension.wasm)**

`tests/fixtures/dummy-bundle-ext/.gitignore`:

```
target/
Cargo.lock
src/bindings.rs
```

- [ ] **Step 7: Commit fixture sources + binary**

```bash
git add tests/fixtures/dummy-bundle-ext/Cargo.toml \
        tests/fixtures/dummy-bundle-ext/build.sh \
        tests/fixtures/dummy-bundle-ext/describe.json \
        tests/fixtures/dummy-bundle-ext/src/lib.rs \
        tests/fixtures/dummy-bundle-ext/extension.wasm \
        tests/fixtures/dummy-bundle-ext/.gitignore
git commit -m "test(ext/wasm): dummy bundle-extension WASM fixture"
```

---

### Task 10: Integration test — WasmtimeBundleInvoker against dummy fixture

**Files:**
- Create: `tests/ext_wasm_mode_b.rs`

This test wires up `WasmtimeBundleInvoker` against the real dummy WASM fixture and asserts the round-trip works.

- [ ] **Step 1: Write integration test**

`tests/ext_wasm_mode_b.rs`:

```rust
//! Integration test for Mode B WASM execution dispatcher.
//!
//! Loads the dummy bundle extension at `tests/fixtures/dummy-bundle-ext/`,
//! invokes `bundling.render()`, asserts canned bytes returned.

use std::path::PathBuf;

#[test]
fn dummy_ext_render_round_trip() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dummy-bundle-ext");
    assert!(
        fixture_dir.join("extension.wasm").exists(),
        "extension.wasm missing — run tests/fixtures/dummy-bundle-ext/build.sh"
    );

    let invoker = greentic_bundle::ext::wasm::WasmtimeBundleInvoker::from_ext_dirs(&[fixture_dir])
        .expect("invoker init");

    let session_json = r#"{"flows_json":"[]","contents_json":"[]","assets":[],"capabilities_used":[]}"#;
    let result = invoker
        .invoke(greentic_bundle::ext::wasm::WasmInvocation {
            extension_id: "greentic.dummy-bundle-ext",
            recipe_id: "dummy",
            config_json: "{}",
            session_json,
        })
        .expect("invoke");

    assert_eq!(result.filename, "dummy-0.0.1.gtpack");
    assert_eq!(result.bytes, b"PK\x03\x04dummy-pack-bytes");
    assert_eq!(result.sha256.len(), 64);
}

#[test]
fn invoke_unknown_extension_id() {
    let invoker = greentic_bundle::ext::wasm::WasmtimeBundleInvoker::from_ext_dirs(&[])
        .expect("invoker init");

    let err = invoker
        .invoke(greentic_bundle::ext::wasm::WasmInvocation {
            extension_id: "missing.ext",
            recipe_id: "x",
            config_json: "{}",
            session_json: "{}",
        })
        .unwrap_err();

    // Should be NotFound or Internal — not ModeBNotImplemented (we DID implement it).
    assert!(
        !matches!(err, greentic_bundle::ext::errors::ExtensionError::ModeBNotImplemented),
        "got ModeBNotImplemented — implementation regressed"
    );
}
```

**Note**: `greentic_bundle` must expose `pub mod ext;` and within `ext/mod.rs`, `pub mod wasm;` etc. If the existing crate uses `pub(crate)` for these, you may need to expose them publicly OR move tests into `src/ext/wasm/tests.rs` as `mod tests` block.

- [ ] **Step 2: Verify lib.rs exports `ext` publicly**

```bash
grep -n "pub mod ext\|mod ext" src/lib.rs
```

If `mod ext` is private, change to `pub mod ext;`. Or, if the project convention disallows public ext module, move integration test inside `src/ext/wasm/mod.rs` as `#[cfg(test)] mod integration_tests` block (and load fixture path via `env!("CARGO_MANIFEST_DIR")` same way).

- [ ] **Step 3: Run integration test**

```bash
cargo test -p greentic-bundle --test ext_wasm_mode_b 2>&1 | tail -15
```

Expected: 2 PASS.

If `dummy_ext_render_round_trip` fails with WIT/wasmtime errors (e.g. "no matching import"), check:
1. The `bindgen!` macro world matches between greentic-bundle and dummy-ext
2. ext-runtime's `LoadedExtension::build_store_and_instance` wires same imports as our bindings expect

- [ ] **Step 4: Commit**

```bash
git add tests/ext_wasm_mode_b.rs src/lib.rs
git commit -m "test(ext/wasm): integration test against dummy bundle-extension fixture"
```

---

### Task 11: PR3 final verification + push + open PR

- [ ] **Step 1: Run full local check**

```bash
bash ci/local_check.sh
```

Expected: PASS — fmt, clippy with -D warnings, all tests, packaging dry-runs.

If clippy warnings, fix with `cargo clippy --fix --allow-staged --all-targets`.

- [ ] **Step 2: Verify Mode B path active**

Quick smoke test:

```bash
cargo test -p greentic-bundle ext::wasm 2>&1 | tail -10
cargo test -p greentic-bundle --test ext_wasm_mode_b 2>&1 | tail -10
```

- [ ] **Step 3: Push branch**

```bash
git push -u origin feat/mode-b-wasm-dispatcher
```

- [ ] **Step 4: Open PR**

```bash
gh pr create --base main \
  --title "feat: Mode B WASM execution dispatcher (Wave 2)" \
  --body "$(cat <<'EOF'
## Summary

- Implements `greentic-bundle/src/ext/wasm.rs` Mode B WASM execution dispatcher (was stubbed `Err(ModeBNotImplemented)`)
- New `BundleWasmInvoker` trait with `WasmtimeBundleInvoker` (production) + `MockBundleInvoker` (tests) impls
- Reuses `greentic-ext-runtime` v0.3.0 for Engine/Component/InstancePool foundation
- Adds bundle-side `bindgen!` for `greentic:extension-bundle@0.1.0` WIT world (vendored into `wit/`)
- Process-singleton invoker via `OnceLock`; default loads from `~/.greentic/extensions/bundle/` (override via `GREENTIC_BUNDLE_EXT_DIR`)
- Dummy WASM fixture at `tests/fixtures/dummy-bundle-ext/` for integration tests

## What it unblocks

Wave 3 (`bundle-standard 0.2.0` WASM rewrite) — the recipe extension can now flip `execution.kind` from `builtin` to `wasm` and have the host dispatch to it via this Mode B path.

## Test plan

- [x] `cargo test -p greentic-bundle ext::wasm` — unit tests pass (~9 tests across invoker + mock + types)
- [x] `cargo test -p greentic-bundle --test ext_wasm_mode_b` — integration test against dummy fixture passes
- [x] `bash ci/local_check.sh` green
- [x] Builtin bridge (`execution.kind=builtin`) path unaffected — `cargo test -p greentic-bundle ext::builtin_bridge` still passes 3 tests

Part of Wave 2 of cards2pack removal migration. Spec at `greentic-designer/docs/superpowers/specs/2026-04-23-cards2pack-removal-design.md`. Wave 1 PRs: #57, #58.
EOF
)"
```

Capture the PR URL.

- [ ] **Step 5: Verify CI green** before declaring Wave 2 complete.

---

## Wave 2 completion checklist

After PR merged to main:

- [ ] `cargo test -p greentic-bundle ext::wasm` green
- [ ] `cargo test -p greentic-bundle --test ext_wasm_mode_b` green
- [ ] `cargo test -p greentic-bundle ext::builtin_bridge` green (no regression)
- [ ] `bash ci/local_check.sh` green
- [ ] Memory entry updated: append Wave 2 progress to `cards2pack-bundle-pipeline-2026-04-23.md`
- [ ] Wave 3 plan can be written: `bundle-standard 0.2.0 WASM` (flip execution.kind=builtin→wasm, render() now real impl using cards2pack-core + bundle-standard-core libs from Wave 1)

## Risks + mitigations

| Risk | Mitigation |
|------|-----------|
| `bindgen!` macro generates names that don't match plan code | Run `cargo expand` to inspect actual generated names; adapt code to match |
| ext-runtime's `LoadedExtension::build_store_and_instance` signature differs from prediction | Read actual source in Step 1 of Task 7; adapt accordingly |
| WIT world version mismatch (`@0.1.0` vs newer) | Vendored WIT pinned in `wit/`; refresh procedure documented in `wit/README.md` |
| `greentic-ext-runtime` not on crates.io → packaging dry-run fails | Wave 1 already saw this; ci/local_check.sh skips package dry-run when workspace deps unpublished. If blocking, switch to `git` dep with commit pin. |
| Engine cache TLB pressure (multiple Engine instances if singleton not respected) | `INVOKER` OnceLock ensures single ExtensionRuntime → single Engine per process |
| Test cross-contamination via OnceLock | Integration tests in `tests/*.rs` get separate process per file. Unit tests inside `src/ext/wasm/mod.rs` test types only, not the singleton. |
