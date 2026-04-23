# Wave 1: Pure-Rust Core Libs Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract two pure-Rust crates (`cards2pack-core`, `bundle-standard-core`) into `greentic-bundle/crates/` as the foundation for Mode B WASM `bundle-standard` extension. Bake in the four observed flow generation bug fixes during extraction.

**Architecture:**
- Two new workspace member crates in `greentic-bundle/crates/`. Each is pure Rust (no `tokio`, no subprocess, no `tempfile`, no `walkdir`) and cross-compiles cleanly to `wasm32-wasip2`.
- `cards2pack-core` converts a card array → flow YGTc YAML string. Owns entry detection, routing graph, HTTP node injection, and clean YAML emission.
- `bundle-standard-core` accepts flow YAML + cards + config and produces `.gtpack` bytes (ZIP-of-workspace, in-memory).
- `greentic-bundle/src/ext/builtin_bridge.rs::handle_standard()` becomes a thin wrapper that calls `bundle_standard_core::build_pack()`. This preserves Phase A behavior while the new core is the single source of truth.

**Tech Stack:** Rust 1.91+ edition 2024, `serde`, `serde_json`, `serde_yaml_bw` (re-exported as `serde_yaml_gtc`), `sha2`, `zip`, `thiserror`, `insta` (snapshot tests).

**Out of scope:** Mode B WASM dispatcher (Wave 2), bundle-standard 0.2.0 WASM impl (Wave 3), designer cleanup (Wave 4), repo archival (Wave 5).

**Spec reference:** `greentic-designer/docs/superpowers/specs/2026-04-23-cards2pack-removal-design.md`

---

## File Structure

### New: `crates/cards2pack-core/`

```
crates/cards2pack-core/
├── Cargo.toml
├── src/
│   ├── lib.rs              public API + module wiring
│   ├── types.rs            CardEntry, CardKind, ConvertOptions, ConvertResult, Diagnostic
│   ├── errors.rs           ConvertError + code() method
│   ├── parse.rs            parse_cards: JSON string → Vec<CardEntry>
│   ├── entry.rs            detect_entry: heuristic for entry node selection
│   ├── routing.rs          build_routing: BFS DAG from card actions
│   ├── http_inject.rs      http_to_node: HTTP entry → component.exec flow node
│   ├── emit.rs             emit_ygtc: serialize Flow IR → YAML string
│   └── convert.rs          convert(): public orchestrator
├── tests/
│   ├── golden.rs           insta snapshot tests for 4 fixtures
│   └── fixtures/
│       ├── noc_alert/      copy of /tmp/inspect-app/assets/cards/*.json
│       │   ├── cards.json  consolidated input shape
│       │   └── expected.ygtc
│       ├── chatbot_loop/
│       ├── http_form/
│       └── multi_form/
```

### New: `crates/bundle-standard-core/`

```
crates/bundle-standard-core/
├── Cargo.toml
├── src/
│   ├── lib.rs              public API + module wiring
│   ├── types.rs            PackInputs, PackOutput, FlowEntry, CardContentEntry, StandardConfig, StandardMetadata, I18nConfig
│   ├── errors.rs           PackError + code()
│   ├── workspace.rs        synthesize_workspace: in-memory Vec<(path, bytes)>
│   ├── zip_writer.rs       zip_entries: ZIP a sorted Vec into Vec<u8>
│   └── build.rs            build_pack(): public orchestrator + sha256 + filename
├── tests/
│   └── round_trip.rs       build → unzip → assert structure + sha256 stability
```

### Modified

- `Cargo.toml` (workspace root) — add `crates/cards2pack-core` and `crates/bundle-standard-core` to `workspace.members`. Add new shared deps under `[workspace.dependencies]`.
- `src/ext/builtin_bridge.rs` — refactor `handle_standard` to delegate to `bundle_standard_core::build_pack`; keep public signature identical so `dispatcher.rs` and existing tests are untouched.
- `src/ext/mod.rs` — no functional change; add re-export only if needed.
- `Cargo.toml` (top-level package) — add `bundle-standard-core` as path dep for `greentic-bundle` package.

---

## Reference reading (skim before starting)

Engineer should glance at these files for context but **NOT verbatim port**:

- `greentic-cards2pack/src/graph.rs` — existing FlowGraph IR (BTreeMap-based; we will rewrite using ordering-preserving structures)
- `greentic-cards2pack/src/emit_flow.rs` — existing YGTc emission via subprocess to `greentic-flow new`. Subprocess approach IS NOT allowed in this lib — emit YAML directly via `serde_yaml_bw`.
- `greentic-cards2pack/src/ir.rs` — existing card IR with `RouteTarget` enum
- `greentic-cards2pack/src/scan.rs` — existing card scanner (filesystem-based; we accept JSON arrays directly, no scan needed)
- `greentic-bundle/src/ext/builtin_bridge.rs` — current builtin handler (full flow we are extracting `bundle-standard-core` from)
- `greentic-designer/src/orchestrate/cards2pack.rs::prepare_cards` — observe current ID rename (welcome) and back-edge stripping (which we will NOT carry over)
- `greentic-designer/src/orchestrate/http_inject.rs` — HTTP entry shape (`{type: "http", config: {url, method, headers, body_mapping}}`)

NOC fixture data already on disk: `/tmp/inspect-app/assets/cards/*.json` — 13 card files extracted during the 2026-04-23 inspection. Use these verbatim as the NOC fixture.

---

## PR1: cards2pack-core extract

PR title: `feat: extract cards2pack-core pure-Rust crate (Wave 1.1)`
Branch: `feat/cards2pack-core-extract`
Base: `main`

**Module-declaration convention for PR1**: as each new module file is created in a task, append a `mod <name>;` line to `crates/cards2pack-core/src/lib.rs` so the test step compiles. Final re-export block (`pub use ...`) lands in Task 10. Same convention applies to PR2 below — Task 19 finalizes the re-export block for `bundle-standard-core`.

### Task 1: Scaffold crate

**Files:**
- Create: `crates/cards2pack-core/Cargo.toml`
- Create: `crates/cards2pack-core/src/lib.rs`
- Modify: `Cargo.toml` (workspace root) — add member + workspace deps

- [ ] **Step 1: Create branch and scaffold workspace member**

```bash
git checkout main
git pull
git checkout -b feat/cards2pack-core-extract
mkdir -p crates/cards2pack-core/src crates/cards2pack-core/tests/fixtures
```

- [ ] **Step 2: Write Cargo.toml**

`crates/cards2pack-core/Cargo.toml`:

```toml
[package]
name = "cards2pack-core"
version.workspace = true
edition = "2024"
rust-version = "1.91"
license = "MIT"
description = "Pure-Rust cards-to-flow YGTc converter for Greentic bundle extensions."
repository = "https://github.com/greenticai/greentic-bundle"

[lib]
path = "src/lib.rs"

[dependencies]
serde.workspace = true
serde_json.workspace = true
serde_yaml_bw.workspace = true
thiserror = "2"

[dev-dependencies]
insta = { version = "1", features = ["yaml", "json"] }
```

- [ ] **Step 3: Add to workspace members + deps**

In root `Cargo.toml`, modify `[workspace]` section:

```toml
[workspace]
members = [
    "crates/greentic-bundle-reader",
    "crates/cards2pack-core",
]
```

Add to `[workspace.dependencies]` if missing:

```toml
thiserror = "2"
```

- [ ] **Step 4: Initial lib.rs (modules added incrementally per task)**

`crates/cards2pack-core/src/lib.rs`:

```rust
//! Pure-Rust cards-to-flow YGTc converter.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tokio`, NO subprocess,
//! NO native filesystem mutation. Inputs are deserialized JSON; outputs are strings
//! and structured warnings.

#![forbid(unsafe_code)]
#![deny(rust_2024_idioms)]
```

(No `mod` declarations yet. Each subsequent task adds its own `mod foo;` line as the file is created. Final re-export block lands in Task 10.)

- [ ] **Step 5: Verify cargo check compiles workspace**

```bash
cargo check -p cards2pack-core
```

Expected: succeeds (only stub modules, but workspace wiring valid).

- [ ] **Step 6: Commit**

```bash
git add crates/cards2pack-core/Cargo.toml crates/cards2pack-core/src/lib.rs Cargo.toml
git commit -m "feat(cards2pack-core): scaffold pure-Rust crate"
```

---

### Task 2: Public types

**Files:**
- Create: `crates/cards2pack-core/src/types.rs`

- [ ] **Step 1: Write failing test**

`crates/cards2pack-core/src/types.rs`:

```rust
//! Public input + output types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct CardEntry {
    pub id: String,
    pub kind: CardKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CardKind {
    AdaptiveCard(serde_json::Value),
    Http(HttpConfig),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HttpConfig {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: serde_json::Value,
    #[serde(default)]
    pub body_mapping: serde_json::Value,
    #[serde(default)]
    pub next_entry_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub flow_name: String,
    pub strict: bool,
}

#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub flow_yaml: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticKind {
    UnreachableCard,
    DanglingRoute,
    DuplicateRouteKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_config_roundtrips_json() {
        let raw = r#"{"url":"https://x","method":"POST","next_entry_id":"next"}"#;
        let cfg: HttpConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.url, "https://x");
        assert_eq!(cfg.method, "POST");
        assert_eq!(cfg.next_entry_id.as_deref(), Some("next"));
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

```bash
cargo test -p cards2pack-core types
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/types.rs
git commit -m "feat(cards2pack-core): public types (CardEntry, ConvertOptions, etc.)"
```

---

### Task 3: Errors with stable codes

**Files:**
- Create: `crates/cards2pack-core/src/errors.rs`

- [ ] **Step 1: Write the file**

`crates/cards2pack-core/src/errors.rs`:

```rust
//! Typed errors with stable string codes for cross-boundary identification.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("E_NO_CARDS: no cards provided")]
    NoCards,
    #[error("E_NO_ENTRY: cannot detect entry card from {count} cards")]
    NoEntryCard { count: usize },
    #[error("E_DANGLING_ROUTE: card '{from}' routes to unknown card '{to}'")]
    DanglingRoute { from: String, to: String },
    #[error("E_INVALID_CARD: card '{id}' is not a valid AdaptiveCard JSON: {msg}")]
    InvalidCard { id: String, msg: String },
    #[error("E_INVALID_HTTP: card '{id}' has invalid HTTP config: {msg}")]
    InvalidHttp { id: String, msg: String },
    #[error("E_PARSE: cannot parse cards JSON: {0}")]
    Parse(String),
    #[error("E_EMIT: cannot serialize flow YAML: {0}")]
    Emit(String),
}

impl ConvertError {
    pub fn code(&self) -> &'static str {
        match self {
            ConvertError::NoCards => "E_NO_CARDS",
            ConvertError::NoEntryCard { .. } => "E_NO_ENTRY",
            ConvertError::DanglingRoute { .. } => "E_DANGLING_ROUTE",
            ConvertError::InvalidCard { .. } => "E_INVALID_CARD",
            ConvertError::InvalidHttp { .. } => "E_INVALID_HTTP",
            ConvertError::Parse(_) => "E_PARSE",
            ConvertError::Emit(_) => "E_EMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_round_trip() {
        assert_eq!(ConvertError::NoCards.code(), "E_NO_CARDS");
        assert_eq!(
            ConvertError::NoEntryCard { count: 0 }.code(),
            "E_NO_ENTRY"
        );
        assert_eq!(
            ConvertError::DanglingRoute { from: "a".into(), to: "b".into() }.code(),
            "E_DANGLING_ROUTE"
        );
    }
}
```

- [ ] **Step 2: Run test**

```bash
cargo test -p cards2pack-core errors
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/errors.rs
git commit -m "feat(cards2pack-core): typed errors with stable code() identifiers"
```

---

### Task 4: parse_cards (JSON → Vec<CardEntry>)

**Files:**
- Create: `crates/cards2pack-core/src/parse.rs`

Input shape (matches `designer-session.contents_json` from WIT contract):

```json
[
    { "id": "welcome", "json": { "type": "AdaptiveCard", ... } },
    { "id": "api_step", "json": { "type": "http", "config": { ... } } }
]
```

When `json.type == "http"`, parse `json.config` as `HttpConfig` → `CardKind::Http`. Otherwise → `CardKind::AdaptiveCard(json)` verbatim.

- [ ] **Step 1: Write failing test**

`crates/cards2pack-core/src/parse.rs`:

```rust
//! Parse `contents_json` payload into `Vec<CardEntry>`.

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind, HttpConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawEntry {
    id: String,
    json: serde_json::Value,
}

pub fn parse_cards(contents_json: &str) -> Result<Vec<CardEntry>, ConvertError> {
    let raw: Vec<RawEntry> = serde_json::from_str(contents_json)
        .map_err(|e| ConvertError::Parse(e.to_string()))?;

    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let kind = classify(&entry)?;
        out.push(CardEntry { id: entry.id, kind });
    }
    Ok(out)
}

fn classify(entry: &RawEntry) -> Result<CardKind, ConvertError> {
    let ty = entry.json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ty == "http" {
        let config_value = entry.json.get("config").cloned().unwrap_or(serde_json::json!({}));
        let cfg: HttpConfig = serde_json::from_value(config_value)
            .map_err(|e| ConvertError::InvalidHttp { id: entry.id.clone(), msg: e.to_string() })?;
        Ok(CardKind::Http(cfg))
    } else {
        Ok(CardKind::AdaptiveCard(entry.json.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_card_and_http() {
        let raw = r#"[
            {"id":"welcome","json":{"type":"AdaptiveCard","version":"1.5"}},
            {"id":"api_x","json":{"type":"http","config":{"url":"http://x","method":"GET","next_entry_id":"after"}}}
        ]"#;
        let cards = parse_cards(raw).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "welcome");
        assert!(matches!(cards[0].kind, CardKind::AdaptiveCard(_)));
        assert!(matches!(cards[1].kind, CardKind::Http(_)));
    }

    #[test]
    fn rejects_invalid_http_config() {
        let raw = r#"[{"id":"bad","json":{"type":"http","config":{"url":42}}}]"#;
        let err = parse_cards(raw).unwrap_err();
        assert_eq!(err.code(), "E_INVALID_HTTP");
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_cards("not json").unwrap_err();
        assert_eq!(err.code(), "E_PARSE");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core parse
```

Expected: 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/parse.rs
git commit -m "feat(cards2pack-core): parse_cards from contents_json payload"
```

---

### Task 5: Entry detection heuristic

Heuristic (in priority order):
1. First card with ≥2 `Action.Submit` actions where each action's `data.routeToCardId` is non-empty (a "menu card").
2. Otherwise: first card by array order whose kind is `CardKind::AdaptiveCard` (skip HTTP entries).
3. Otherwise: error `NoEntryCard`.

**Files:**
- Create: `crates/cards2pack-core/src/entry.rs`

- [ ] **Step 1: Write failing tests**

`crates/cards2pack-core/src/entry.rs`:

```rust
//! Entry node detection.

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind};

/// Returns the id of the card that should be the flow's `start` node.
pub fn detect_entry(cards: &[CardEntry]) -> Result<String, ConvertError> {
    if cards.is_empty() {
        return Err(ConvertError::NoCards);
    }

    if let Some(menu) = cards.iter().find(|c| is_menu_card(c)) {
        return Ok(menu.id.clone());
    }

    if let Some(first) = cards.iter().find(|c| matches!(c.kind, CardKind::AdaptiveCard(_))) {
        return Ok(first.id.clone());
    }

    Err(ConvertError::NoEntryCard { count: cards.len() })
}

fn is_menu_card(card: &CardEntry) -> bool {
    let CardKind::AdaptiveCard(json) = &card.kind else {
        return false;
    };
    let actions = match json.get("actions").and_then(|a| a.as_array()) {
        Some(arr) => arr,
        None => return false,
    };
    let route_count = actions
        .iter()
        .filter(|a| {
            a.get("type").and_then(|v| v.as_str()) == Some("Action.Submit")
                && a.get("data")
                    .and_then(|d| d.get("routeToCardId"))
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
        })
        .count();
    route_count >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(id: &str, json: serde_json::Value) -> CardEntry {
        CardEntry { id: id.into(), kind: CardKind::AdaptiveCard(json) }
    }

    #[test]
    fn picks_menu_card_over_first() {
        let cards = vec![
            card("intro", json!({"type":"AdaptiveCard"})),
            card("welcome", json!({
                "type":"AdaptiveCard",
                "actions":[
                    {"type":"Action.Submit","data":{"routeToCardId":"a"}},
                    {"type":"Action.Submit","data":{"routeToCardId":"b"}}
                ]
            })),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "welcome");
    }

    #[test]
    fn fallback_to_first_card_when_no_menu() {
        let cards = vec![
            card("a", json!({"type":"AdaptiveCard"})),
            card("b", json!({"type":"AdaptiveCard"})),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "a");
    }

    #[test]
    fn ignores_single_action_cards() {
        let cards = vec![
            card("greeter", json!({
                "type":"AdaptiveCard",
                "actions":[{"type":"Action.Submit","data":{"routeToCardId":"next"}}]
            })),
            card("after", json!({"type":"AdaptiveCard"})),
        ];
        // Single-action card is NOT a menu card, fallback to first.
        assert_eq!(detect_entry(&cards).unwrap(), "greeter");
    }

    #[test]
    fn errors_when_only_http_entries() {
        let cards = vec![CardEntry {
            id: "x".into(),
            kind: CardKind::Http(crate::types::HttpConfig::default()),
        }];
        let err = detect_entry(&cards).unwrap_err();
        assert_eq!(err.code(), "E_NO_ENTRY");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core entry
```

Expected: 4 PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/entry.rs
git commit -m "feat(cards2pack-core): entry detection (menu-card heuristic + fallback)"
```

---

### Task 6: Routing graph builder

Walk every card's `actions[].data.routeToCardId` and build edges. Back-edges (target appears earlier in array order) are PRESERVED — runtime handles navigation.

`strict=true` → unknown route target raises `DanglingRoute`. `strict=false` → emits `Diagnostic` and skips the edge.

Routes are stored per source node, in declaration order, with key derived from `data.action_id` (fallback to `goto_<target>` if absent).

**Files:**
- Create: `crates/cards2pack-core/src/routing.rs`

- [ ] **Step 1: Write the file**

`crates/cards2pack-core/src/routing.rs`:

```rust
//! Build routing graph from card actions.

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind, Diagnostic, DiagnosticKind};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct RouteEdge {
    pub action_id: String,
    pub target: String,
}

#[derive(Debug, Default)]
pub struct RoutingGraph {
    /// node_id → outbound edges in declaration order.
    pub edges: HashMap<String, Vec<RouteEdge>>,
}

pub fn build_routing(
    cards: &[CardEntry],
    strict: bool,
) -> Result<(RoutingGraph, Vec<Diagnostic>), ConvertError> {
    let known: std::collections::HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();
    let mut graph = RoutingGraph::default();
    let mut diagnostics = Vec::new();

    for card in cards {
        let mut edges_for_card: Vec<RouteEdge> = Vec::new();
        let CardKind::AdaptiveCard(json) = &card.kind else {
            // HTTP entries get their routing emitted in http_inject; skip here.
            continue;
        };
        let actions = json.get("actions").and_then(|a| a.as_array());
        let Some(actions) = actions else { continue };

        for action in actions {
            let target = action
                .get("data")
                .and_then(|d| d.get("routeToCardId"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());

            let Some(target) = target else { continue };

            if !known.contains(target) {
                if strict {
                    return Err(ConvertError::DanglingRoute {
                        from: card.id.clone(),
                        to: target.into(),
                    });
                }
                diagnostics.push(Diagnostic {
                    kind: DiagnosticKind::DanglingRoute,
                    message: format!("card '{}' routes to unknown '{}'", card.id, target),
                });
                continue;
            }

            let action_id = action
                .get("data")
                .and_then(|d| d.get("action_id"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("goto_{target}"));

            edges_for_card.push(RouteEdge {
                action_id,
                target: target.to_owned(),
            });
        }

        if !edges_for_card.is_empty() {
            graph.edges.insert(card.id.clone(), edges_for_card);
        }
    }

    Ok((graph, diagnostics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CardKind;
    use serde_json::json;

    fn card(id: &str, json: serde_json::Value) -> CardEntry {
        CardEntry { id: id.into(), kind: CardKind::AdaptiveCard(json) }
    }

    #[test]
    fn edges_preserve_declaration_order() {
        let cards = vec![
            card("welcome", json!({
                "actions":[
                    {"type":"Action.Submit","data":{"routeToCardId":"a","action_id":"go_a"}},
                    {"type":"Action.Submit","data":{"routeToCardId":"b","action_id":"go_b"}}
                ]
            })),
            card("a", json!({})),
            card("b", json!({})),
        ];
        let (g, diags) = build_routing(&cards, false).unwrap();
        assert!(diags.is_empty());
        let edges = g.edges.get("welcome").unwrap();
        assert_eq!(edges[0].target, "a");
        assert_eq!(edges[1].target, "b");
        assert_eq!(edges[0].action_id, "go_a");
    }

    #[test]
    fn back_edges_preserved() {
        let cards = vec![
            card("welcome", json!({"actions":[{"type":"Action.Submit","data":{"routeToCardId":"chat"}}]})),
            card("chat", json!({"actions":[{"type":"Action.Submit","data":{"routeToCardId":"welcome"}}]})),
        ];
        let (g, _) = build_routing(&cards, false).unwrap();
        // chat → welcome (back-edge) MUST be present.
        let chat_edges = g.edges.get("chat").unwrap();
        assert_eq!(chat_edges[0].target, "welcome");
    }

    #[test]
    fn dangling_route_strict_errors() {
        let cards = vec![card("welcome", json!({"actions":[
            {"type":"Action.Submit","data":{"routeToCardId":"missing"}}
        ]}))];
        let err = build_routing(&cards, true).unwrap_err();
        assert_eq!(err.code(), "E_DANGLING_ROUTE");
    }

    #[test]
    fn dangling_route_lenient_diagnostic() {
        let cards = vec![card("welcome", json!({"actions":[
            {"type":"Action.Submit","data":{"routeToCardId":"missing"}}
        ]}))];
        let (g, diags) = build_routing(&cards, false).unwrap();
        assert!(g.edges.is_empty());
        assert_eq!(diags.len(), 1);
        assert!(matches!(diags[0].kind, DiagnosticKind::DanglingRoute));
    }

    #[test]
    fn synthesizes_action_id_when_missing() {
        let cards = vec![
            card("welcome", json!({"actions":[
                {"type":"Action.Submit","data":{"routeToCardId":"target"}}
            ]})),
            card("target", json!({})),
        ];
        let (g, _) = build_routing(&cards, false).unwrap();
        assert_eq!(g.edges.get("welcome").unwrap()[0].action_id, "goto_target");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core routing
```

Expected: 5 PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/routing.rs
git commit -m "feat(cards2pack-core): routing graph builder (preserves back-edges)"
```

---

### Task 7: HTTP node injection

For each `CardEntry` with `kind: CardKind::Http`, emit a synthetic flow node entry that the YGTc emitter renders as `component.exec`. The HTTP node also inserts an outbound route to its `next_entry_id` (so the flow continues after the API call).

**Files:**
- Create: `crates/cards2pack-core/src/http_inject.rs`

- [ ] **Step 1: Write the file**

`crates/cards2pack-core/src/http_inject.rs`:

```rust
//! Synthesize HTTP flow nodes from `CardKind::Http` entries.

use crate::routing::{RouteEdge, RoutingGraph};
use crate::types::{CardEntry, CardKind, HttpConfig};
use std::collections::HashMap;

/// Information about each HTTP node that the emitter will render.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpNode {
    pub id: String,
    pub config: HttpConfig,
}

/// Append HTTP-derived nodes to the routing graph and return the list of HTTP nodes
/// for the emitter to materialize.
pub fn inject_http_nodes(
    cards: &[CardEntry],
    routing: &mut RoutingGraph,
) -> Vec<HttpNode> {
    let mut http_nodes = Vec::new();
    let known: std::collections::HashSet<&str> = cards.iter().map(|c| c.id.as_str()).collect();

    for card in cards {
        let CardKind::Http(cfg) = &card.kind else { continue };
        http_nodes.push(HttpNode {
            id: card.id.clone(),
            config: cfg.clone(),
        });

        // Wire HTTP node → next_entry_id (if specified and known).
        if let Some(next) = cfg.next_entry_id.as_deref()
            && known.contains(next)
        {
            routing.edges.insert(
                card.id.clone(),
                vec![RouteEdge {
                    action_id: format!("after_{}", card.id),
                    target: next.to_owned(),
                }],
            );
        }
    }

    http_nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CardKind;
    use serde_json::json;

    fn ac_card(id: &str) -> CardEntry {
        CardEntry { id: id.into(), kind: CardKind::AdaptiveCard(json!({})) }
    }

    #[test]
    fn emits_http_node_with_next_route() {
        let cards = vec![
            ac_card("welcome"),
            CardEntry {
                id: "api".into(),
                kind: CardKind::Http(HttpConfig {
                    url: "https://x".into(),
                    method: "GET".into(),
                    next_entry_id: Some("done".into()),
                    ..Default::default()
                }),
            },
            ac_card("done"),
        ];
        let mut routing = RoutingGraph::default();
        let nodes = inject_http_nodes(&cards, &mut routing);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "api");
        let route = routing.edges.get("api").unwrap();
        assert_eq!(route[0].target, "done");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core http_inject
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/http_inject.rs
git commit -m "feat(cards2pack-core): HTTP entry → component.exec node + routing"
```

---

### Task 8: YGTc emission

Emit clean YGTc 2.0 YAML with the routing graph + HTTP nodes. **Critical bug fix:** under each card node, write `card.call.payload.{...}` ONLY. Do NOT also write the same fields flat under `card.{...}` (the existing emit_flow.rs has both — that's the schema bloat bug).

YAML shape (per `serde_yaml_bw` — note: stable key ordering):

```yaml
id: <flow_name>
type: messaging
schema_version: 2
start: <entry_id>
nodes:
  <card_id>:
    routing:
      - to: <target>
        when: action.action_id == "<action_id>"  # OR omit when single edge
    card:
      call:
        op: render
        metadata: []
        payload:
          card_source: asset
          card_spec:
            asset_path: assets/cards/<card_id>.json
          mode: renderAndValidate
          node_id: <card_id>
          payload: {}
          session: {}
          state: {}
          validation_mode: warn
  <http_node_id>:
    routing:
      - to: <next_entry_id>
    component:
      exec:
        source: oci://ghcr.io/greenticai/components/component-http:latest
        bindings:
          url: "<url>"
          method: "<method>"
          headers: <headers>
          body_mapping: <body_mapping>
```

**Files:**
- Create: `crates/cards2pack-core/src/emit.rs`

- [ ] **Step 1: Write file with strongly-typed IR + emit function**

`crates/cards2pack-core/src/emit.rs`:

```rust
//! Emit YGTc 2.0 YAML from routing graph + HTTP nodes.

use crate::errors::ConvertError;
use crate::http_inject::HttpNode;
use crate::routing::{RouteEdge, RoutingGraph};
use crate::types::CardEntry;
use serde::Serialize;
use std::collections::BTreeMap;

const HTTP_COMPONENT_REF: &str = "oci://ghcr.io/greenticai/components/component-http:latest";

#[derive(Debug, Serialize)]
struct FlowYaml {
    id: String,
    #[serde(rename = "type")]
    flow_type: &'static str,
    schema_version: u32,
    start: String,
    nodes: BTreeMap<String, NodeYaml>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum NodeYaml {
    Card(CardNode),
    Http(HttpNode2),
}

#[derive(Debug, Serialize)]
struct CardNode {
    routing: Vec<RouteYaml>,
    card: CardCall,
}

#[derive(Debug, Serialize)]
struct HttpNode2 {
    routing: Vec<RouteYaml>,
    component: ComponentExecWrap,
}

#[derive(Debug, Serialize)]
struct ComponentExecWrap {
    exec: ComponentExec,
}

#[derive(Debug, Serialize)]
struct ComponentExec {
    source: String,
    bindings: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct CardCall {
    call: CardCallOp,
}

#[derive(Debug, Serialize)]
struct CardCallOp {
    op: &'static str,
    metadata: Vec<serde_json::Value>,
    payload: CardPayload,
}

#[derive(Debug, Serialize)]
struct CardPayload {
    card_source: &'static str,
    card_spec: CardSpec,
    mode: &'static str,
    node_id: String,
    payload: serde_json::Value,
    session: serde_json::Value,
    state: serde_json::Value,
    validation_mode: &'static str,
}

#[derive(Debug, Serialize)]
struct CardSpec {
    asset_path: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum RouteYaml {
    Conditional { to: String, when: String },
    Unconditional { to: String },
}

pub fn emit_ygtc(
    cards: &[CardEntry],
    entry: &str,
    routing: &RoutingGraph,
    http_nodes: &[HttpNode],
    flow_name: &str,
) -> Result<String, ConvertError> {
    let mut nodes: BTreeMap<String, NodeYaml> = BTreeMap::new();

    // Card nodes (skip HTTP entries, handled below).
    for card in cards {
        if matches!(card.kind, crate::types::CardKind::Http(_)) {
            continue;
        }
        let routes = render_routes(routing.edges.get(&card.id));
        nodes.insert(
            card.id.clone(),
            NodeYaml::Card(CardNode {
                routing: routes,
                card: CardCall {
                    call: CardCallOp {
                        op: "render",
                        metadata: vec![],
                        payload: CardPayload {
                            card_source: "asset",
                            card_spec: CardSpec {
                                asset_path: format!("assets/cards/{}.json", card.id),
                            },
                            mode: "renderAndValidate",
                            node_id: card.id.clone(),
                            payload: serde_json::json!({}),
                            session: serde_json::json!({}),
                            state: serde_json::json!({}),
                            validation_mode: "warn",
                        },
                    },
                },
            }),
        );
    }

    // HTTP nodes.
    for http in http_nodes {
        let routes = render_routes(routing.edges.get(&http.id));
        let bindings = serde_json::json!({
            "url": http.config.url,
            "method": http.config.method,
            "headers": http.config.headers,
            "body_mapping": http.config.body_mapping,
        });
        nodes.insert(
            http.id.clone(),
            NodeYaml::Http(HttpNode2 {
                routing: routes,
                component: ComponentExecWrap {
                    exec: ComponentExec {
                        source: HTTP_COMPONENT_REF.into(),
                        bindings,
                    },
                },
            }),
        );
    }

    let flow = FlowYaml {
        id: flow_name.into(),
        flow_type: "messaging",
        schema_version: 2,
        start: entry.into(),
        nodes,
    };

    serde_yaml_bw::to_string(&flow).map_err(|e| ConvertError::Emit(e.to_string()))
}

fn render_routes(edges: Option<&Vec<RouteEdge>>) -> Vec<RouteYaml> {
    let Some(edges) = edges else { return vec![] };
    if edges.len() == 1 {
        // Single edge: emit unconditional `- to: target`.
        return vec![RouteYaml::Unconditional {
            to: edges[0].target.clone(),
        }];
    }
    edges
        .iter()
        .map(|e| RouteYaml::Conditional {
            to: e.target.clone(),
            when: format!("action.action_id == \"{}\"", e.action_id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CardEntry, CardKind};
    use serde_json::json;

    #[test]
    fn emits_minimal_single_node_flow() {
        let cards = vec![CardEntry {
            id: "welcome".into(),
            kind: CardKind::AdaptiveCard(json!({})),
        }];
        let yaml = emit_ygtc(&cards, "welcome", &RoutingGraph::default(), &[], "demo").unwrap();
        assert!(yaml.contains("start: welcome"));
        assert!(yaml.contains("schema_version: 2"));
        assert!(yaml.contains("type: messaging"));
        assert!(yaml.contains("asset_path: assets/cards/welcome.json"));
        // Bug-fix assertion: no duplicate flat fields outside card.call.payload.
        // If duplicate emission existed, "card_source: asset" would appear twice.
        let occurrences = yaml.matches("card_source: asset").count();
        assert_eq!(occurrences, 1, "duplicate flat fields detected:\n{yaml}");
    }

    #[test]
    fn multi_route_emits_conditional_when() {
        use crate::routing::RouteEdge;
        let cards = vec![
            CardEntry { id: "menu".into(), kind: CardKind::AdaptiveCard(json!({})) },
            CardEntry { id: "a".into(), kind: CardKind::AdaptiveCard(json!({})) },
            CardEntry { id: "b".into(), kind: CardKind::AdaptiveCard(json!({})) },
        ];
        let mut routing = RoutingGraph::default();
        routing.edges.insert("menu".into(), vec![
            RouteEdge { action_id: "go_a".into(), target: "a".into() },
            RouteEdge { action_id: "go_b".into(), target: "b".into() },
        ]);
        let yaml = emit_ygtc(&cards, "menu", &routing, &[], "demo").unwrap();
        assert!(yaml.contains(r#"when: action.action_id == "go_a""#));
        assert!(yaml.contains(r#"when: action.action_id == "go_b""#));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core emit
```

Expected: 2 PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/emit.rs
git commit -m "feat(cards2pack-core): YGTc YAML emission (no duplicate flat fields)"
```

---

### Task 9: convert() public API

`convert()` orchestrates: parse cards (already done by caller in real use, but we accept `Vec<CardEntry>` here) → detect entry → build routing → inject HTTP → emit.

Wait — looking at the public API in `lib.rs`, `convert()` takes `&[CardEntry]` directly. So callers feed parsed cards. `parse_cards` is exposed separately for the WASM ext to use.

**Files:**
- Create: `crates/cards2pack-core/src/convert.rs`

- [ ] **Step 1: Write convert + test with NOC fixture sub-scenario**

`crates/cards2pack-core/src/convert.rs`:

```rust
//! Public convert() orchestrator.

use crate::emit::emit_ygtc;
use crate::entry::detect_entry;
use crate::errors::ConvertError;
use crate::http_inject::inject_http_nodes;
use crate::routing::build_routing;
use crate::types::{CardEntry, ConvertOptions, ConvertResult};

pub fn convert(
    cards: &[CardEntry],
    opts: &ConvertOptions,
) -> Result<ConvertResult, ConvertError> {
    if cards.is_empty() {
        return Err(ConvertError::NoCards);
    }

    let entry = detect_entry(cards)?;
    let (mut routing, mut diagnostics) = build_routing(cards, opts.strict)?;
    let http_nodes = inject_http_nodes(cards, &mut routing);

    // Reachability diagnostic (lenient): warn on cards never targeted (unless they ARE the entry).
    let mut reachable: std::collections::HashSet<&str> = std::collections::HashSet::new();
    reachable.insert(entry.as_str());
    let mut frontier = vec![entry.as_str()];
    while let Some(node) = frontier.pop() {
        if let Some(edges) = routing.edges.get(node) {
            for e in edges {
                if reachable.insert(e.target.as_str()) {
                    frontier.push(e.target.as_str());
                }
            }
        }
    }
    for card in cards {
        if !reachable.contains(card.id.as_str()) {
            diagnostics.push(crate::types::Diagnostic {
                kind: crate::types::DiagnosticKind::UnreachableCard,
                message: format!("card '{}' is unreachable from entry '{}'", card.id, entry),
            });
        }
    }

    let flow_yaml = emit_ygtc(cards, &entry, &routing, &http_nodes, &opts.flow_name)?;

    Ok(ConvertResult { flow_yaml, diagnostics })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CardKind, ConvertOptions};
    use serde_json::json;

    #[test]
    fn happy_path_two_cards() {
        let cards = vec![
            CardEntry { id: "welcome".into(), kind: CardKind::AdaptiveCard(json!({
                "actions":[{"type":"Action.Submit","data":{"routeToCardId":"thanks","action_id":"go"}}]
            }))},
            CardEntry { id: "thanks".into(), kind: CardKind::AdaptiveCard(json!({})) },
        ];
        let res = convert(&cards, &ConvertOptions { flow_name: "demo".into(), strict: false }).unwrap();
        assert!(res.flow_yaml.contains("start: welcome"));
        assert!(res.diagnostics.is_empty());
    }

    #[test]
    fn empty_cards_errors() {
        let err = convert(&[], &ConvertOptions { flow_name: "x".into(), strict: false }).unwrap_err();
        assert_eq!(err.code(), "E_NO_CARDS");
    }

    #[test]
    fn unreachable_card_emits_diagnostic() {
        let cards = vec![
            CardEntry { id: "a".into(), kind: CardKind::AdaptiveCard(json!({})) },
            CardEntry { id: "orphan".into(), kind: CardKind::AdaptiveCard(json!({})) },
        ];
        let res = convert(&cards, &ConvertOptions { flow_name: "demo".into(), strict: false }).unwrap();
        assert!(res.diagnostics.iter().any(|d| matches!(d.kind, crate::types::DiagnosticKind::UnreachableCard)));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p cards2pack-core convert
```

Expected: 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/convert.rs
git commit -m "feat(cards2pack-core): convert() orchestrator + reachability diagnostic"
```

---

### Task 10: Public re-exports + parse_cards exposure

- [ ] **Step 1: Update lib.rs to re-export parse_cards**

Modify `crates/cards2pack-core/src/lib.rs`, replace the existing pub use block with:

```rust
pub use convert::convert;
pub use errors::ConvertError;
pub use http_inject::HttpNode;
pub use parse::parse_cards;
pub use routing::{build_routing, RouteEdge, RoutingGraph};
pub use types::{
    CardEntry, CardKind, ConvertOptions, ConvertResult, Diagnostic, DiagnosticKind, HttpConfig,
};
```

- [ ] **Step 2: Verify all tests still pass**

```bash
cargo test -p cards2pack-core
```

Expected: all PASS (parse + entry + routing + http_inject + emit + convert + types + errors).

- [ ] **Step 3: Commit**

```bash
git add crates/cards2pack-core/src/lib.rs
git commit -m "feat(cards2pack-core): expose parse_cards + routing internals"
```

---

### Task 11: NOC golden test (regression-proof bug fixes)

Use the 13 cards from `/tmp/inspect-app/assets/cards/*.json` (extracted from the user's NOC bundle on 2026-04-23) as the golden fixture. The expected output proves all four bug fixes:
- `start: welcome` (NOT `start: demo_wrapup`)
- routing follows `routeToCardId`, not alphabetical chain
- Back-edges preserved (welcome ↔ scenes via "Back to Main Menu")
- No duplicate flat fields under any card node

**Files:**
- Create: `crates/cards2pack-core/tests/golden.rs`
- Create: `crates/cards2pack-core/tests/fixtures/noc_alert/cards.json`

- [ ] **Step 1: Generate fixture file**

```bash
mkdir -p crates/cards2pack-core/tests/fixtures/noc_alert
python3 - <<'EOF'
import json, os, glob
src = "/tmp/inspect-app/assets/cards"
cards = []
for path in sorted(glob.glob(f"{src}/*.json")):
    cid = os.path.splitext(os.path.basename(path))[0]
    with open(path) as f:
        cards.append({"id": cid, "json": json.load(f)})
out = "crates/cards2pack-core/tests/fixtures/noc_alert/cards.json"
with open(out, "w") as f:
    json.dump(cards, f, indent=2)
print(f"wrote {len(cards)} cards to {out}")
EOF
```

Expected: "wrote 13 cards to crates/cards2pack-core/tests/fixtures/noc_alert/cards.json"

- [ ] **Step 2: Write golden test**

`crates/cards2pack-core/tests/golden.rs`:

```rust
//! Snapshot tests that lock in the four bug-fix expectations.

use cards2pack_core::{convert, parse_cards, ConvertOptions};

fn run_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}/cards.json");
    let raw = std::fs::read_to_string(&path).expect("fixture missing");
    let cards = parse_cards(&raw).expect("parse_cards");
    let res = convert(&cards, &ConvertOptions {
        flow_name: name.replace('_', "-"),
        strict: false,
    }).expect("convert");
    res.flow_yaml
}

#[test]
fn noc_alert_golden() {
    let yaml = run_fixture("noc_alert");
    insta::assert_snapshot!("noc_alert", yaml);
}

#[test]
fn noc_alert_start_is_welcome_not_demo_wrapup() {
    let yaml = run_fixture("noc_alert");
    assert!(yaml.contains("start: welcome"), "start should be welcome (menu card with 4 routes); got:\n{yaml}");
    assert!(!yaml.contains("start: demo_wrapup"), "start must NOT be demo_wrapup");
}

#[test]
fn noc_alert_no_duplicate_flat_fields() {
    let yaml = run_fixture("noc_alert");
    // If schema bloat existed, "card_source: asset" appears 2× per card.
    // 13 cards → 13 occurrences expected, NOT 26.
    let count = yaml.matches("card_source: asset").count();
    assert_eq!(count, 13, "expected 13 occurrences (1 per card), got {count}");
}

#[test]
fn noc_alert_routing_uses_routeToCardId_not_alphabetical_chain() {
    let yaml = run_fixture("noc_alert");
    // welcome card has 4 menu actions → must emit 4 conditional routes.
    let when_count = yaml.matches("when: action.action_id ==").count();
    assert!(when_count >= 4, "expected >=4 conditional routes from welcome menu; got {when_count}");
}
```

- [ ] **Step 3: Run test, capture snapshot**

```bash
cargo test -p cards2pack-core --test golden noc_alert_golden -- --nocapture
```

First run: insta creates `.snap.new` file. Install reviewer if absent and accept:

```bash
command -v cargo-insta >/dev/null || cargo install cargo-insta --locked
cd crates/cards2pack-core
cargo insta review
```

Choose accept for the noc_alert snapshot.

- [ ] **Step 4: Run all noc_alert tests**

```bash
cd ../..
cargo test -p cards2pack-core --test golden noc_alert
```

Expected: 4 PASS.

- [ ] **Step 5: Commit fixture + test + accepted snapshot**

```bash
git add crates/cards2pack-core/tests/
git commit -m "test(cards2pack-core): NOC golden test (locks in 4 bug fixes)"
```

---

### Task 12: Additional fixtures (chatbot loop, http form, multi-form)

For each fixture, create `cards.json` and add a snapshot test. Below is the cards.json for each — engineer copies verbatim.

- [ ] **Step 1: chatbot_loop fixture**

`crates/cards2pack-core/tests/fixtures/chatbot_loop/cards.json`:

```json
[
  { "id": "welcome", "json": {
      "type": "AdaptiveCard",
      "actions": [
        { "type": "Action.Submit", "title": "Start", "data": { "routeToCardId": "chat_input", "action_id": "go_chat" } },
        { "type": "Action.Submit", "title": "Quit", "data": { "routeToCardId": "bye", "action_id": "go_bye" } }
      ]
  }},
  { "id": "chat_input", "json": {
      "type": "AdaptiveCard",
      "actions": [
        { "type": "Action.Submit", "title": "Send", "data": { "routeToCardId": "chat_reply", "action_id": "send" } },
        { "type": "Action.Submit", "title": "Home", "data": { "routeToCardId": "welcome", "action_id": "home" } }
      ]
  }},
  { "id": "chat_reply", "json": {
      "type": "AdaptiveCard",
      "actions": [
        { "type": "Action.Submit", "title": "Continue", "data": { "routeToCardId": "chat_input", "action_id": "continue" } },
        { "type": "Action.Submit", "title": "Done", "data": { "routeToCardId": "bye", "action_id": "done" } }
      ]
  }},
  { "id": "bye", "json": { "type": "AdaptiveCard" } }
]
```

- [ ] **Step 2: http_form fixture**

`crates/cards2pack-core/tests/fixtures/http_form/cards.json`:

```json
[
  { "id": "form", "json": {
      "type": "AdaptiveCard",
      "actions": [
        { "type": "Action.Submit", "title": "Submit", "data": { "routeToCardId": "submit_api", "action_id": "submit" } }
      ]
  }},
  { "id": "submit_api", "json": {
      "type": "http",
      "config": {
        "url": "https://api.example.com/submit",
        "method": "POST",
        "headers": { "Content-Type": "application/json" },
        "body_mapping": { "name": "$.formData.name" },
        "next_entry_id": "thanks"
      }
  }},
  { "id": "thanks", "json": { "type": "AdaptiveCard" } }
]
```

- [ ] **Step 3: multi_form fixture**

`crates/cards2pack-core/tests/fixtures/multi_form/cards.json`:

```json
[
  { "id": "menu", "json": {
      "type": "AdaptiveCard",
      "actions": [
        { "type": "Action.Submit", "title": "Form A", "data": { "routeToCardId": "form_a", "action_id": "go_a" } },
        { "type": "Action.Submit", "title": "Form B", "data": { "routeToCardId": "form_b", "action_id": "go_b" } },
        { "type": "Action.Submit", "title": "Form C", "data": { "routeToCardId": "form_c", "action_id": "go_c" } }
      ]
  }},
  { "id": "form_a", "json": {
      "type": "AdaptiveCard",
      "actions": [{ "type": "Action.Submit", "title": "Done", "data": { "routeToCardId": "thanks", "action_id": "done_a" } }]
  }},
  { "id": "form_b", "json": {
      "type": "AdaptiveCard",
      "actions": [{ "type": "Action.Submit", "title": "Done", "data": { "routeToCardId": "thanks", "action_id": "done_b" } }]
  }},
  { "id": "form_c", "json": {
      "type": "AdaptiveCard",
      "actions": [{ "type": "Action.Submit", "title": "Done", "data": { "routeToCardId": "thanks", "action_id": "done_c" } }]
  }},
  { "id": "thanks", "json": { "type": "AdaptiveCard" } }
]
```

- [ ] **Step 4: Add tests for new fixtures**

Append to `crates/cards2pack-core/tests/golden.rs`:

```rust
#[test]
fn chatbot_loop_golden() {
    let yaml = run_fixture("chatbot_loop");
    insta::assert_snapshot!("chatbot_loop", yaml);
}

#[test]
fn chatbot_loop_back_edges_preserved() {
    let yaml = run_fixture("chatbot_loop");
    // chat_reply → chat_input is a back-edge; must NOT be stripped.
    assert!(yaml.contains(r#"to: chat_input"#));
    // chat_input → welcome is also a back-edge.
    assert!(yaml.contains(r#"to: welcome"#));
}

#[test]
fn http_form_golden() {
    let yaml = run_fixture("http_form");
    insta::assert_snapshot!("http_form", yaml);
}

#[test]
fn http_form_emits_component_exec() {
    let yaml = run_fixture("http_form");
    assert!(yaml.contains("component-http"));
    assert!(yaml.contains("https://api.example.com/submit"));
}

#[test]
fn multi_form_golden() {
    let yaml = run_fixture("multi_form");
    insta::assert_snapshot!("multi_form", yaml);
}

#[test]
fn multi_form_three_conditional_routes() {
    let yaml = run_fixture("multi_form");
    let when_count = yaml.matches("when: action.action_id ==").count();
    assert!(when_count >= 3, "expected >=3 conditional routes from menu; got {when_count}");
}
```

- [ ] **Step 5: Generate + accept snapshots**

```bash
cargo test -p cards2pack-core --test golden -- --nocapture
command -v cargo-insta >/dev/null || cargo install cargo-insta --locked
cd crates/cards2pack-core && cargo insta review && cd ../..
```

Accept all 3 new snapshots.

- [ ] **Step 6: Run full golden suite**

```bash
cargo test -p cards2pack-core --test golden
```

Expected: 10 PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/cards2pack-core/tests/
git commit -m "test(cards2pack-core): add chatbot_loop, http_form, multi_form fixtures"
```

---

### Task 13: PR1 final verification

- [ ] **Step 1: Run full local check**

```bash
bash ci/local_check.sh
```

Expected: PASS (fmt, clippy with -D warnings, all tests).

- [ ] **Step 2: Push branch + open PR**

```bash
git push -u origin feat/cards2pack-core-extract
gh pr create --base main \
  --title "feat: extract cards2pack-core pure-Rust crate (Wave 1.1)" \
  --body "$(cat <<'EOF'
## Summary

- New `crates/cards2pack-core/` workspace member
- Pure-Rust cards → flow YGTc converter (no tokio/subprocess/native I/O)
- Cross-compiles cleanly to `wasm32-wasip2` (verified by absence of native deps)
- Bakes in 4 bug fixes vs `greentic-cards2pack v0.4` (alphabetical ordering, demo_wrapup-as-start, routeToCardId ignored, schema bloat)

## Test plan

- [ ] `cargo test -p cards2pack-core` — 25+ unit tests + 10 golden tests pass
- [ ] `bash ci/local_check.sh` green
- [ ] NOC fixture (regression-proof) snapshot accepted
- [ ] No tokio/walkdir/tempfile in dep tree (`cargo tree -p cards2pack-core | grep -E "tokio|walkdir|tempfile"` returns empty)

Part of Wave 1 of cards2pack removal migration. See spec at `greentic-designer/docs/superpowers/specs/2026-04-23-cards2pack-removal-design.md`.
EOF
)"
```

- [ ] **Step 3: Verify CI green** before continuing to PR2.

---

## PR2: bundle-standard-core extract

PR title: `feat: extract bundle-standard-core pure-Rust crate (Wave 1.2)`
Branch: `feat/bundle-standard-core-extract`
Base: `main` (NOT depends on PR1; can run in parallel)

### Task 14: Scaffold crate

**Files:**
- Create: `crates/bundle-standard-core/Cargo.toml`
- Create: `crates/bundle-standard-core/src/lib.rs`
- Modify: `Cargo.toml` (root, add to workspace.members)

- [ ] **Step 1: Create branch**

```bash
git checkout main
git pull
git checkout -b feat/bundle-standard-core-extract
mkdir -p crates/bundle-standard-core/src crates/bundle-standard-core/tests
```

- [ ] **Step 2: Write Cargo.toml**

`crates/bundle-standard-core/Cargo.toml`:

```toml
[package]
name = "bundle-standard-core"
version.workspace = true
edition = "2024"
rust-version = "1.91"
license = "MIT"
description = "Pure-Rust workspace+ZIP assembly for the bundle-standard recipe."
repository = "https://github.com/greenticai/greentic-bundle"

[lib]
path = "src/lib.rs"

[dependencies]
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror = "2"
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 3: Add to workspace members**

In root `Cargo.toml`, modify `[workspace]`:

```toml
[workspace]
members = [
    "crates/greentic-bundle-reader",
    "crates/cards2pack-core",
    "crates/bundle-standard-core",
]
```

Add to `[workspace.dependencies]` if missing:

```toml
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 4: Initial lib.rs (modules added incrementally per task)**

`crates/bundle-standard-core/src/lib.rs`:

```rust
//! Pure-Rust workspace assembly + ZIP for the bundle-standard recipe.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tempfile`, NO `walkdir`,
//! NO `std::fs::write/create_dir_all`. Everything in-memory: `Vec<u8>` bytes,
//! `Vec<(String, Vec<u8>)>` entries.

#![forbid(unsafe_code)]
#![deny(rust_2024_idioms)]
```

(Each subsequent task adds its own `mod foo;` line. Final re-export block lands at end of Task 19.)

- [ ] **Step 5: Verify compile + commit**

```bash
cargo check -p bundle-standard-core
git add crates/bundle-standard-core/Cargo.toml crates/bundle-standard-core/src/lib.rs Cargo.toml
git commit -m "feat(bundle-standard-core): scaffold pure-Rust crate"
```

---

### Task 15: Public types

**Files:**
- Create: `crates/bundle-standard-core/src/types.rs`

- [ ] **Step 1: Write types**

`crates/bundle-standard-core/src/types.rs`:

```rust
//! Public input + output types. Schema mirrors the existing
//! `greentic-bundle::ext::builtin_bridge::DesignerSession + StandardConfig` so
//! the wrapper transition (Task 21) is mechanical.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct PackInputs<'a> {
    pub config: &'a StandardConfig,
    pub flows: &'a [FlowEntry],
    pub cards: &'a [CardContentEntry],
    pub assets: &'a [(String, Vec<u8>)],
    pub capabilities: &'a [String],
}

#[derive(Debug, Clone)]
pub struct FlowEntry {
    pub name: String,
    pub yaml: String,
}

#[derive(Debug, Clone)]
pub struct CardContentEntry {
    pub id: String,
    pub json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardConfig {
    pub metadata: StandardMetadata,
    pub channels: Vec<String>,
    #[serde(default = "default_embed_ui")]
    pub embed_ui: String,
    #[serde(default)]
    pub i18n: I18nConfig,
    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct I18nConfig {
    #[serde(default = "default_i18n_source")]
    pub source: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

fn default_embed_ui() -> String { "none".into() }
fn default_format() -> String { "gtpack-legacy".into() }
fn default_i18n_source() -> String { "en".into() }

#[derive(Debug, Clone)]
pub struct PackOutput {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}
```

- [ ] **Step 2: Sanity test + commit**

```rust
// At bottom of types.rs:
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_round_trips() {
        let raw = r#"{"metadata":{"name":"x","version":"0.1.0"},"channels":["webchat"],"format":"gtpack-legacy"}"#;
        let cfg: StandardConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.metadata.name, "x");
        assert_eq!(cfg.embed_ui, "none");
        assert_eq!(cfg.i18n.source, "en");
    }
}
```

```bash
cargo test -p bundle-standard-core types
git add crates/bundle-standard-core/src/types.rs
git commit -m "feat(bundle-standard-core): public types matching DesignerSession schema"
```

---

### Task 16: Errors

**Files:**
- Create: `crates/bundle-standard-core/src/errors.rs`

- [ ] **Step 1: Write file**

```rust
//! Typed errors with stable codes.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("E_INVALID_FORMAT: format '{0}' not supported (only 'gtpack-legacy' in Phase A)")]
    InvalidFormat(String),
    #[error("E_INVALID_CONFIG: {0}")]
    InvalidConfig(String),
    #[error("E_ZIP: {0}")]
    Zip(String),
    #[error("E_SERDE: {0}")]
    Serde(String),
}

impl PackError {
    pub fn code(&self) -> &'static str {
        match self {
            PackError::InvalidFormat(_) => "E_INVALID_FORMAT",
            PackError::InvalidConfig(_) => "E_INVALID_CONFIG",
            PackError::Zip(_) => "E_ZIP",
            PackError::Serde(_) => "E_SERDE",
        }
    }
}

impl From<serde_json::Error> for PackError {
    fn from(e: serde_json::Error) -> Self { PackError::Serde(e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_stable() {
        assert_eq!(PackError::InvalidFormat("x".into()).code(), "E_INVALID_FORMAT");
    }
}
```

- [ ] **Step 2: Test + commit**

```bash
cargo test -p bundle-standard-core errors
git add crates/bundle-standard-core/src/errors.rs
git commit -m "feat(bundle-standard-core): typed errors with stable codes"
```

---

### Task 17: Workspace synthesis (in-memory entries)

Produces a `Vec<(String, Vec<u8>)>` representing the file tree to ZIP. NO disk I/O.

**Files:**
- Create: `crates/bundle-standard-core/src/workspace.rs`

- [ ] **Step 1: Write file**

```rust
//! Synthesize the bundle workspace tree as in-memory entries.

use crate::errors::PackError;
use crate::types::{CardContentEntry, FlowEntry, PackInputs, StandardConfig};

pub fn synthesize_workspace(inputs: &PackInputs<'_>) -> Result<Vec<(String, Vec<u8>)>, PackError> {
    if inputs.config.format != "gtpack-legacy" {
        return Err(PackError::InvalidFormat(inputs.config.format.clone()));
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();

    // bundle.yaml
    entries.push(("bundle.yaml".into(), bundle_yaml(inputs.config).into_bytes()));

    // flows/<name>.ygtc
    for flow in inputs.flows {
        entries.push((
            format!("flows/{}.ygtc", flow.name),
            flow.yaml.as_bytes().to_vec(),
        ));
    }

    // assets/cards/<id>.json
    for card in inputs.cards {
        let pretty = serde_json::to_vec_pretty(&card.json)?;
        entries.push((format!("assets/cards/{}.json", card.id), pretty));
    }

    // assets/<rel_path> from raw assets
    for (rel, bytes) in inputs.assets {
        entries.push((format!("assets/{rel}"), bytes.clone()));
    }

    // tenants/default/tenant.gmap
    entries.push((
        "tenants/default/tenant.gmap".into(),
        tenant_gmap(inputs.capabilities).into_bytes(),
    ));

    // Sort for deterministic ZIP ordering.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(entries)
}

fn bundle_yaml(config: &StandardConfig) -> String {
    let channels: String = config.channels.iter().map(|c| format!("  - {c}\n")).collect();
    format!(
        "apiVersion: greentic.ai/v1\nkind: BundleWorkspace\nmetadata:\n  name: {}\n  version: {}\nchannels:\n{}",
        config.metadata.name, config.metadata.version, channels,
    )
}

fn tenant_gmap(caps: &[String]) -> String {
    let caps: String = caps.iter().map(|c| format!("  - {c}\n")).collect();
    format!("# generated by bundle-standard-core\ntenant: default\ncapabilities:\n{caps}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StandardConfig, StandardMetadata, FlowEntry, CardContentEntry, I18nConfig};
    use serde_json::json;

    fn cfg() -> StandardConfig {
        StandardConfig {
            metadata: StandardMetadata { name: "demo".into(), version: "0.1.0".into(), author: None },
            channels: vec!["webchat".into()],
            embed_ui: "webchat".into(),
            i18n: I18nConfig::default(),
            format: "gtpack-legacy".into(),
        }
    }

    #[test]
    fn entries_sorted() {
        let cfg = cfg();
        let flows = vec![FlowEntry { name: "main".into(), yaml: "x".into() }];
        let cards = vec![CardContentEntry { id: "welcome".into(), json: json!({}) }];
        let inputs = PackInputs { config: &cfg, flows: &flows, cards: &cards, assets: &[], capabilities: &[] };
        let entries = synthesize_workspace(&inputs).unwrap();
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn rejects_non_legacy_format() {
        let mut config = cfg();
        config.format = "apack".into();
        let inputs = PackInputs { config: &config, flows: &[], cards: &[], assets: &[], capabilities: &[] };
        let err = synthesize_workspace(&inputs).unwrap_err();
        assert_eq!(err.code(), "E_INVALID_FORMAT");
    }
}
```

- [ ] **Step 2: Test + commit**

```bash
cargo test -p bundle-standard-core workspace
git add crates/bundle-standard-core/src/workspace.rs
git commit -m "feat(bundle-standard-core): synthesize workspace entries (in-memory)"
```

---

### Task 18: ZIP writer

**Files:**
- Create: `crates/bundle-standard-core/src/zip_writer.rs`

- [ ] **Step 1: Write file**

```rust
//! ZIP a sorted entries vector into Vec<u8>. Deterministic.

use crate::errors::PackError;
use std::io::Write;

pub fn zip_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, PackError> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (name, bytes) in entries {
            zip.start_file(name, options).map_err(zip_err)?;
            zip.write_all(bytes).map_err(|e| PackError::Zip(e.to_string()))?;
        }
        zip.finish().map_err(zip_err)?;
    }
    Ok(buf)
}

fn zip_err(e: zip::result::ZipError) -> PackError {
    PackError::Zip(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let entries = vec![
            ("a.txt".into(), b"hello".to_vec()),
            ("b/c.txt".into(), b"world".to_vec()),
        ];
        let bytes = zip_entries(&entries).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        let mut f = zip.by_name("a.txt").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn deterministic_bytes() {
        let entries = vec![("a.txt".into(), b"x".to_vec())];
        let a = zip_entries(&entries).unwrap();
        let b = zip_entries(&entries).unwrap();
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Test + commit**

```bash
cargo test -p bundle-standard-core zip_writer
git add crates/bundle-standard-core/src/zip_writer.rs
git commit -m "feat(bundle-standard-core): in-memory ZIP writer (deterministic)"
```

---

### Task 19: build_pack() public API + sha256 + filename

**Files:**
- Create: `crates/bundle-standard-core/src/build.rs`

- [ ] **Step 1: Write file**

```rust
//! Public build_pack orchestrator: synthesize → ZIP → hash → name.

use crate::errors::PackError;
use crate::types::{PackInputs, PackOutput};
use crate::workspace::synthesize_workspace;
use crate::zip_writer::zip_entries;
use sha2::{Digest, Sha256};

pub fn build_pack(inputs: &PackInputs<'_>) -> Result<PackOutput, PackError> {
    let entries = synthesize_workspace(inputs)?;
    let bytes = zip_entries(&entries)?;
    let sha256 = hex_sha256(&bytes);
    let filename = format!(
        "{}-{}.gtpack",
        inputs.config.metadata.name, inputs.config.metadata.version
    );
    Ok(PackOutput { filename, bytes, sha256 })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in out { s.push(HEX[(b >> 4) as usize] as char); s.push(HEX[(b & 0x0f) as usize] as char); }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StandardConfig, StandardMetadata, I18nConfig, FlowEntry, CardContentEntry};
    use serde_json::json;

    fn min_inputs<'a>(
        cfg: &'a StandardConfig,
        flows: &'a [FlowEntry],
        cards: &'a [CardContentEntry],
    ) -> PackInputs<'a> {
        PackInputs { config: cfg, flows, cards, assets: &[], capabilities: &[] }
    }

    #[test]
    fn happy_path() {
        let cfg = StandardConfig {
            metadata: StandardMetadata { name: "demo".into(), version: "0.1.0".into(), author: None },
            channels: vec!["webchat".into()],
            embed_ui: "webchat".into(),
            i18n: I18nConfig::default(),
            format: "gtpack-legacy".into(),
        };
        let flows = vec![FlowEntry { name: "main".into(), yaml: "schema_version: 2".into() }];
        let cards = vec![CardContentEntry { id: "welcome".into(), json: json!({"type":"AdaptiveCard"}) }];
        let out = build_pack(&min_inputs(&cfg, &flows, &cards)).unwrap();
        assert_eq!(out.filename, "demo-0.1.0.gtpack");
        assert_eq!(out.sha256.len(), 64);
        assert!(!out.bytes.is_empty());
    }

    #[test]
    fn deterministic_sha() {
        let cfg = StandardConfig {
            metadata: StandardMetadata { name: "x".into(), version: "1".into(), author: None },
            channels: vec![], embed_ui: "none".into(), i18n: I18nConfig::default(),
            format: "gtpack-legacy".into(),
        };
        let a = build_pack(&min_inputs(&cfg, &[], &[])).unwrap();
        let b = build_pack(&min_inputs(&cfg, &[], &[])).unwrap();
        assert_eq!(a.sha256, b.sha256);
    }
}
```

- [ ] **Step 2: Finalize lib.rs re-exports**

Replace `crates/bundle-standard-core/src/lib.rs` with the full re-export block:

```rust
//! Pure-Rust workspace assembly + ZIP for the bundle-standard recipe.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tempfile`, NO `walkdir`,
//! NO `std::fs::write/create_dir_all`. Everything in-memory: `Vec<u8>` bytes,
//! `Vec<(String, Vec<u8>)>` entries.

#![forbid(unsafe_code)]
#![deny(rust_2024_idioms)]

mod build;
mod errors;
mod types;
mod workspace;
mod zip_writer;

pub use build::build_pack;
pub use errors::PackError;
pub use types::{
    CardContentEntry, FlowEntry, I18nConfig, PackInputs, PackOutput, StandardConfig,
    StandardMetadata,
};
```

- [ ] **Step 3: Test + commit**

```bash
cargo test -p bundle-standard-core build
git add crates/bundle-standard-core/src/build.rs crates/bundle-standard-core/src/lib.rs
git commit -m "feat(bundle-standard-core): build_pack orchestrator + finalize re-exports"
```

---

### Task 20: Round-trip integration test

**Files:**
- Create: `crates/bundle-standard-core/tests/round_trip.rs`

- [ ] **Step 1: Write integration test**

```rust
//! Build a pack, unzip it, assert workspace structure intact.

use bundle_standard_core::*;
use serde_json::json;
use std::io::Read;

fn cfg(name: &str) -> StandardConfig {
    StandardConfig {
        metadata: StandardMetadata { name: name.into(), version: "0.1.0".into(), author: None },
        channels: vec!["webchat".into()],
        embed_ui: "webchat".into(),
        i18n: I18nConfig::default(),
        format: "gtpack-legacy".into(),
    }
}

#[test]
fn pack_unzip_contains_expected_files() {
    let cfg = cfg("demo");
    let flows = vec![FlowEntry { name: "main".into(), yaml: "id: demo\nschema_version: 2\n".into() }];
    let cards = vec![CardContentEntry { id: "welcome".into(), json: json!({"type":"AdaptiveCard"}) }];
    let inputs = PackInputs { config: &cfg, flows: &flows, cards: &cards, assets: &[], capabilities: &[] };

    let out = build_pack(&inputs).unwrap();
    assert_eq!(out.filename, "demo-0.1.0.gtpack");

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
    let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_owned()).collect();
    assert!(names.iter().any(|n| n == "bundle.yaml"));
    assert!(names.iter().any(|n| n == "flows/main.ygtc"));
    assert!(names.iter().any(|n| n == "assets/cards/welcome.json"));
    assert!(names.iter().any(|n| n == "tenants/default/tenant.gmap"));

    let mut f = zip.by_name("flows/main.ygtc").unwrap();
    let mut s = String::new();
    f.read_to_string(&mut s).unwrap();
    assert!(s.contains("id: demo"));
}

#[test]
fn assets_pass_through_verbatim() {
    let cfg = cfg("demo");
    let png_bytes = vec![0x89, 0x50, 0x4e, 0x47]; // PNG header
    let assets = vec![("logo.png".into(), png_bytes.clone())];
    let inputs = PackInputs { config: &cfg, flows: &[], cards: &[], assets: &assets, capabilities: &[] };

    let out = build_pack(&inputs).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(out.bytes)).unwrap();
    let mut f = zip.by_name("assets/logo.png").unwrap();
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, png_bytes);
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p bundle-standard-core --test round_trip
git add crates/bundle-standard-core/tests/round_trip.rs
git commit -m "test(bundle-standard-core): round-trip integration tests"
```

---

### Task 21: Refactor builtin_bridge.rs to delegate to new core

The existing `handle_standard()` in `src/ext/builtin_bridge.rs` becomes a thin wrapper that converts old-shape inputs into `PackInputs` and calls `bundle_standard_core::build_pack()`. Existing tests in that file MUST continue to pass unchanged.

**Files:**
- Modify: `Cargo.toml` (top-level package, add `bundle-standard-core` dep)
- Modify: `src/ext/builtin_bridge.rs`

- [ ] **Step 1: Add path dep**

In root `Cargo.toml`, under `[dependencies]` (top-level package, NOT workspace.dependencies):

```toml
bundle-standard-core = { path = "crates/bundle-standard-core" }
```

- [ ] **Step 2: Refactor handle_standard()**

Replace the body of `handle_standard()` in `src/ext/builtin_bridge.rs` (everything after the deserialize calls, lines ~71-91) with delegation:

```rust
pub fn handle_standard(
    config_json: &str,
    session_json: &str,
) -> Result<RenderedArtifact, ExtensionError> {
    use bundle_standard_core::{
        build_pack, CardContentEntry, FlowEntry, PackInputs, StandardConfig as BSConfig,
    };

    // Parse old-shape inputs (preserves backward-compat for callers).
    let session: DesignerSession = serde_json::from_str(session_json)?;
    // bundle-standard-core owns StandardConfig now; parse once, use for validation + build.
    let bs_config: BSConfig = serde_json::from_str(config_json)?;

    let flows: Vec<FlowEntry> = serde_json::from_str::<Vec<serde_json::Value>>(&session.flows_json)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| FlowEntry {
            name: v.get("name").and_then(|x| x.as_str()).map(str::to_owned).unwrap_or_else(|| format!("flow-{i:03}")),
            yaml: v.get("yaml").and_then(|x| x.as_str()).unwrap_or("").to_owned(),
        })
        .collect();

    let cards: Vec<CardContentEntry> = serde_json::from_str::<Vec<serde_json::Value>>(&session.contents_json)?
        .into_iter()
        .filter_map(|v| Some(CardContentEntry {
            id: v.get("id").and_then(|x| x.as_str())?.to_owned(),
            json: v.get("json")?.clone(),
        }))
        .collect();

    let inputs = PackInputs {
        config: &bs_config,
        flows: &flows,
        cards: &cards,
        assets: &session.assets,
        capabilities: &session.capabilities_used,
    };

    let pack = build_pack(&inputs).map_err(|e| match e.code() {
        "E_INVALID_FORMAT" => ExtensionError::InvalidConfig(e.to_string()),
        _ => ExtensionError::Io(std::io::Error::other(e.to_string())),
    })?;

    Ok(RenderedArtifact {
        filename: pack.filename,
        bytes: pack.bytes,
        sha256: pack.sha256,
    })
}
```

Now delete the now-unused private helpers in `builtin_bridge.rs`:
- `compute_session_id` (no longer used; bundle-standard-core determinism comes from sorted entries + deterministic ZIP)
- `hex_sha256` and `hex_encode` (replaced by bundle-standard-core's own hashing)
- `write_ephemeral_workspace` (replaced by `synthesize_workspace`)
- `zip_workspace` and `zip_io` (replaced by `zip_entries`)

Keep:
- The `DesignerSession`, `StandardConfig`, `StandardMetadata`, `I18nConfig` local Deserialize structs (still needed to parse session_json)
- All public functions
- All `#[cfg(test)] mod tests` — these MUST continue to pass

- [ ] **Step 3: Verify all builtin_bridge.rs tests still pass**

```bash
cargo test -p greentic-bundle ext::builtin_bridge
```

Expected: 4 tests PASS (`session_id_deterministic`, `session_id_differs_on_different_inputs`, `rejects_unsupported_format`, `happy_path_produces_artifact`, `artifact_is_a_valid_zip_containing_bundle_yaml`).

**WAIT** — `session_id_*` tests reference the deleted `compute_session_id`. These tests no longer make sense (functionality moved to bundle-standard-core, where determinism is implicit). DELETE those two tests. Keep `rejects_unsupported_format`, `happy_path_produces_artifact`, `artifact_is_a_valid_zip_containing_bundle_yaml`.

- [ ] **Step 4: Re-run tests**

```bash
cargo test -p greentic-bundle ext::builtin_bridge
```

Expected: 3 PASS.

- [ ] **Step 5: Run full bundle test suite to verify no regression**

```bash
cargo test -p greentic-bundle
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/ext/builtin_bridge.rs
git commit -m "refactor(ext): builtin_bridge delegates to bundle-standard-core"
```

---

### Task 22: PR2 final verification

- [ ] **Step 1: Run full local check**

```bash
bash ci/local_check.sh
```

Expected: PASS.

- [ ] **Step 2: Push + open PR**

```bash
git push -u origin feat/bundle-standard-core-extract
gh pr create --base main \
  --title "feat: extract bundle-standard-core pure-Rust crate (Wave 1.2)" \
  --body "$(cat <<'EOF'
## Summary

- New `crates/bundle-standard-core/` workspace member
- Pure-Rust workspace+ZIP assembly (no `tempfile`, no `walkdir`, no `std::fs::write/create_dir_all`)
- Cross-compiles cleanly to `wasm32-wasip2` (verified by absence of native deps)
- `src/ext/builtin_bridge.rs::handle_standard` refactored to thin wrapper calling `bundle_standard_core::build_pack`
- Deleted obsolete helpers (`compute_session_id`, `hex_sha256`, `write_ephemeral_workspace`, `zip_workspace`)
- Existing builtin_bridge tests (`rejects_unsupported_format`, `happy_path_produces_artifact`, `artifact_is_a_valid_zip_containing_bundle_yaml`) continue to pass

## Test plan

- [ ] `cargo test -p bundle-standard-core` — 8 unit tests + 2 round-trip tests pass
- [ ] `cargo test -p greentic-bundle` — no regression
- [ ] `bash ci/local_check.sh` green
- [ ] No tempfile/walkdir in dep tree (`cargo tree -p bundle-standard-core | grep -E "tokio|walkdir|tempfile"` returns empty)

Part of Wave 1 of cards2pack removal migration. See spec at `greentic-designer/docs/superpowers/specs/2026-04-23-cards2pack-removal-design.md`.
EOF
)"
```

- [ ] **Step 3: Verify CI green** before declaring Wave 1 complete.

---

## Wave 1 completion checklist

After both PRs merged to `main`:

- [ ] `cargo tree -p cards2pack-core | grep -E "^(tokio|walkdir|tempfile)"` returns empty
- [ ] `cargo tree -p bundle-standard-core | grep -E "^(tokio|walkdir|tempfile)"` returns empty
- [ ] `cargo test --workspace` green (cards2pack-core 25+ tests, bundle-standard-core 10+ tests, greentic-bundle no regression)
- [ ] `bash ci/local_check.sh` green
- [ ] Memory entry updated: append progress note to `bundle-extension-migration.md` reflecting Wave 1 done
- [ ] Spec memory `cards2pack-bundle-pipeline-2026-04-23.md` cross-referenced from new memory entry "Wave 1 cards2pack removal cores extracted"

Wave 2 (Mode B WASM execution dispatcher in `greentic-bundle/src/ext/wasm.rs`) plan to be written after Wave 1 merges, when actual API surface of cores is finalized.
