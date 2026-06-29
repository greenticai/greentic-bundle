//! Extract `dw.agent` references from compiled flow manifests and collect
//! agent ids provided by agentic-worker packs.
//!
//! These two helpers are the read-side primitives for Task 5's auto-wiring
//! resolution pass:
//!
//! * [`referenced_dw_agents`] — scans `manifest.cbor` bytes from application
//!   packs and returns every `(flow_id, node_id, agent_id)` triple where a node
//!   has `component.id == "dw.agent"` and a non-empty `component.operation`.
//!
//! * [`provided_agent_ids`] — reads `dw-agents.json` sidecar bytes (one per
//!   pack) and returns the top-level JSON-object keys (the agent ids declared
//!   by that pack).  The discriminator for "this pack provides agents" is the
//!   **presence of the `dw-agents.json` sidecar** inside the `.gtpack` — NOT
//!   the manifest `kind` field (packc collapses `dw-application` → `application`
//!   and hardcodes the manifest `agents` map empty; the real data lives only in
//!   the sidecar).
//!
//! ## IR shape — `referenced_dw_agents`
//!
//! `flow_manifests` are raw `manifest.cbor` bytes already extracted from a
//! `.gtpack` ZIP.  The manifest.cbor IR uses interned symbol tables:
//! `node.component.id` is an integer index into `symbols.component_ids`, and
//! `node.id` is an integer index into `symbols.node_ids`.  Inline string ids
//! are accepted as a defensive fallback (matching the rule in `greentic-start`'s
//! `agent_preflight`).
//!
//! ## IR shape — `provided_agent_ids`
//!
//! `sidecars` are raw `dw-agents.json` bytes, one per pack (packs that do not
//! contain a `dw-agents.json` file should not contribute a slice at all).  The
//! sidecar is a bare JSON object `{ "<agent_id>": <AgentConfig> }`; the caller
//! (Task 5) is responsible for reading the file from the ZIP.  This matches the
//! wire format produced by `packc agent_pack.rs` and consumed by
//! `greentic-runner pack.rs:2314–2327`.
//!
//! ## Mirror contract
//!
//! The extraction predicate here must stay byte-for-byte identical to
//! `greentic-start`'s `agent_preflight::referenced_dw_agents_from_manifest`
//! so that the bundle build and the runtime guard agree on what "referenced"
//! means.

// These helpers are consumed by the test suite now and by Task 5's
// `auto_wire_agent_packs` call-site later.  The `dead_code` lint fires on
// non-test builds because Task 5 hasn't wired the call yet; suppress it here
// rather than polluting the public-item surface with forced re-exports.
#![allow(dead_code)]

use std::collections::BTreeSet;

use ciborium::Value;

/// Component id that marks an agentic-worker node in a compiled flow.
const DW_AGENT_COMPONENT_ID: &str = "dw.agent";

/// Scan `flow_manifests` (raw `manifest.cbor` bytes, one per pack) and collect
/// every `dw.agent` node reference.
///
/// Returns `(flow_id, node_id, agent_id)` for each node whose resolved
/// component id equals `"dw.agent"` and whose `component.operation` is
/// non-empty.  Mirrors `greentic-start`'s
/// `agent_preflight::referenced_dw_agents_from_manifest`.
///
/// Invalid / unreadable byte slices are silently skipped (fail-soft); only a
/// genuine `dw.agent` reference is surfaced.
pub(crate) fn referenced_dw_agents(flow_manifests: &[&[u8]]) -> Vec<(String, String, String)> {
    let mut refs = Vec::new();
    for bytes in flow_manifests {
        let Ok(manifest) = ciborium::de::from_reader::<Value, _>(*bytes) else {
            continue;
        };
        extract_dw_agent_refs(&manifest, &mut refs);
    }
    refs
}

/// Extract `dw.agent` references from one decoded `manifest.cbor`.
fn extract_dw_agent_refs(manifest: &Value, refs: &mut Vec<(String, String, String)>) {
    let component_ids = symbol_table(manifest, "component_ids");
    let node_ids = symbol_table(manifest, "node_ids");

    let Some(Value::Array(flows)) = map_get(manifest, "flows") else {
        return;
    };

    for flow_entry in flows {
        let flow_id = map_get(flow_entry, "id")
            .and_then(as_text)
            .unwrap_or_else(|| "<unknown-flow>".to_string());

        let Some(inner) = map_get(flow_entry, "flow") else {
            continue;
        };
        let Some(Value::Array(nodes)) = map_get(inner, "nodes") else {
            continue;
        };
        for node in nodes {
            let Some(component) = map_get(node, "component") else {
                continue;
            };
            let Some(component_id) = resolve_component_id(component, &component_ids) else {
                continue;
            };
            if component_id != DW_AGENT_COMPONENT_ID {
                continue;
            }
            let Some(agent_id) = map_get(component, "operation").and_then(as_text) else {
                continue;
            };
            if agent_id.is_empty() {
                continue;
            }
            let node_id = resolve_node_id(node, &node_ids);
            refs.push((flow_id.clone(), node_id, agent_id));
        }
    }
}

/// Scan `sidecars` (raw `dw-agents.json` bytes, one per pack) and collect
/// agent ids declared by each sidecar.
///
/// Each `&[u8]` is the bytes of one pack's `dw-agents.json` sidecar file.  The
/// sidecar is a bare JSON object `{ "<agent_id>": <AgentConfig> }` written by
/// `packc agent_pack.rs` and consumed by `greentic-runner pack.rs:2314–2327`.
///
/// For each byte slice:
///
/// 1. Parse as JSON.
/// 2. Skip unless the root value is a JSON object.
/// 3. Collect the non-empty top-level keys (the agent ids).
///
/// Malformed or non-object blobs are silently skipped (fail-soft, no panic),
/// mirroring the runner's "ignore malformed dw-agents.json" behavior.  The
/// caller is responsible for only passing slices from packs that actually
/// contain a `dw-agents.json` file; packs without the sidecar provide no
/// agents.
pub(crate) fn provided_agent_ids(sidecars: &[&[u8]]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for bytes in sidecars {
        let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        let Some(agent_map) = json_value.as_object() else {
            continue;
        };
        for key in agent_map.keys() {
            if !key.is_empty() {
                ids.insert(key.clone());
            }
        }
    }
    ids
}

// ---------------------------------------------------------------------------
// CBOR helpers — self-contained, using ciborium::Value.
// Mirrors greentic-start's agent_preflight helpers (same logic; different
// CBOR library: greentic-bundle uses ciborium, greentic-start uses serde_cbor).
// ---------------------------------------------------------------------------

/// Return `map[key]` for a text key, or `None` if the value is not a map or
/// the key is absent.
fn map_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Map(map) = value else {
        return None;
    };
    map.iter()
        .find(|(k, _)| matches!(k, Value::Text(t) if t == key))
        .map(|(_, v)| v)
}

fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// Convert a CBOR integer to a `usize` index; returns `None` for negative
/// values or out-of-range conversions.
fn as_index(value: &Value) -> Option<usize> {
    if let Value::Integer(int) = value {
        let n: i128 = (*int).into();
        usize::try_from(n).ok()
    } else {
        None
    }
}

/// Read `manifest.symbols.<name>` as a vector of interned strings.
fn symbol_table(manifest: &Value, name: &str) -> Vec<String> {
    let Some(symbols) = map_get(manifest, "symbols") else {
        return Vec::new();
    };
    let Some(Value::Array(items)) = map_get(symbols, name) else {
        return Vec::new();
    };
    items.iter().filter_map(as_text).collect()
}

/// Resolve a node's component id from either an inline string (defensive) or
/// a symbol-table index (the normal compiled form).
fn resolve_component_id(component: &Value, component_ids: &[String]) -> Option<String> {
    let id = map_get(component, "id")?;
    if let Some(text) = as_text(id) {
        return Some(text);
    }
    let index = as_index(id)?;
    component_ids.get(index).cloned()
}

/// Resolve a node's human-readable id, falling back to the raw index string
/// when the symbol table cannot resolve it.
fn resolve_node_id(node: &Value, node_ids: &[String]) -> String {
    match map_get(node, "id") {
        Some(value) => {
            if let Some(text) = as_text(value) {
                return text;
            }
            if let Some(index) = as_index(value) {
                if let Some(name) = node_ids.get(index) {
                    return name.clone();
                }
                return format!("#{index}");
            }
            "<unknown-node>".to_string()
        }
        None => "<unknown-node>".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- CBOR fixture helpers ------------------------------------------------

    fn cbor_map(pairs: Vec<(&str, Value)>) -> Value {
        Value::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (Value::Text(k.to_string()), v))
                .collect(),
        )
    }

    fn cbor_array(items: Vec<Value>) -> Value {
        Value::Array(items)
    }

    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(ciborium::value::Integer::from(n))
    }

    fn to_cbor_bytes(value: &Value) -> Vec<u8> {
        let mut bytes = Vec::new();
        ciborium::into_writer(value, &mut bytes).expect("CBOR serialization must succeed");
        bytes
    }

    /// A `manifest.cbor` with `component_ids = ["ai.greentic.component-adaptive-card",
    /// "dw.agent"]`, `node_ids = ["reply", "research"]` and one flow whose
    /// `research` node references `tavily_researcher`.
    fn dw_agent_flow_manifest_bytes() -> Vec<u8> {
        let symbols = cbor_map(vec![
            (
                "component_ids",
                cbor_array(vec![
                    text("ai.greentic.component-adaptive-card"),
                    text("dw.agent"),
                ]),
            ),
            (
                "node_ids",
                cbor_array(vec![text("reply"), text("research")]),
            ),
        ]);
        let research_node = cbor_map(vec![
            ("id", int(1)),
            (
                "component",
                cbor_map(vec![
                    ("id", int(1)),
                    ("operation", text("tavily_researcher")),
                ]),
            ),
        ]);
        let reply_node = cbor_map(vec![
            ("id", int(0)),
            (
                "component",
                cbor_map(vec![("id", int(0)), ("operation", text("card"))]),
            ),
        ]);
        let flow_entry = cbor_map(vec![
            ("id", text("on_message")),
            (
                "flow",
                cbor_map(vec![("nodes", cbor_array(vec![research_node, reply_node]))]),
            ),
        ]);
        let manifest = cbor_map(vec![
            ("symbols", symbols),
            ("flows", cbor_array(vec![flow_entry])),
        ]);
        to_cbor_bytes(&manifest)
    }

    // --- referenced_dw_agents ------------------------------------------------

    #[test]
    fn extracts_dw_agent_reference_with_correct_triple() {
        let bytes = dw_agent_flow_manifest_bytes();
        let refs = referenced_dw_agents(&[&bytes]);
        assert_eq!(refs.len(), 1, "expected exactly one dw.agent reference");
        assert_eq!(
            refs[0],
            (
                "on_message".to_string(),
                "research".to_string(),
                "tavily_researcher".to_string()
            )
        );
    }

    #[test]
    fn skips_non_dw_agent_nodes() {
        // The manifest has both a card node (id=0) and a dw.agent node (id=1).
        // Only the dw.agent node must appear in the result.
        let bytes = dw_agent_flow_manifest_bytes();
        let refs = referenced_dw_agents(&[&bytes]);
        assert!(
            refs.iter()
                .all(|(_, _, agent)| agent == "tavily_researcher"),
            "the card node must not appear in the dw.agent references"
        );
    }

    #[test]
    fn empty_operation_node_is_skipped() {
        let symbols = cbor_map(vec![
            ("component_ids", cbor_array(vec![text("dw.agent")])),
            ("node_ids", cbor_array(vec![text("research")])),
        ]);
        let empty_op_node = cbor_map(vec![
            ("id", int(0)),
            (
                "component",
                cbor_map(vec![("id", int(0)), ("operation", text(""))]),
            ),
        ]);
        let flow_entry = cbor_map(vec![
            ("id", text("on_message")),
            (
                "flow",
                cbor_map(vec![("nodes", cbor_array(vec![empty_op_node]))]),
            ),
        ]);
        let manifest = cbor_map(vec![
            ("symbols", symbols),
            ("flows", cbor_array(vec![flow_entry])),
        ]);
        let bytes = to_cbor_bytes(&manifest);
        assert!(
            referenced_dw_agents(&[&bytes]).is_empty(),
            "a dw.agent node with empty operation must be skipped"
        );
    }

    #[test]
    fn multiple_manifests_are_scanned() {
        let bytes_a = dw_agent_flow_manifest_bytes();
        let symbols = cbor_map(vec![
            ("component_ids", cbor_array(vec![text("dw.agent")])),
            ("node_ids", cbor_array(vec![text("helper")])),
        ]);
        let node = cbor_map(vec![
            ("id", int(0)),
            (
                "component",
                cbor_map(vec![("id", int(0)), ("operation", text("demo_assistant"))]),
            ),
        ]);
        let flow_entry = cbor_map(vec![
            ("id", text("demo_flow")),
            ("flow", cbor_map(vec![("nodes", cbor_array(vec![node]))])),
        ]);
        let manifest_b = cbor_map(vec![
            ("symbols", symbols),
            ("flows", cbor_array(vec![flow_entry])),
        ]);
        let bytes_b = to_cbor_bytes(&manifest_b);
        let refs = referenced_dw_agents(&[&bytes_a, &bytes_b]);
        let agents: Vec<&str> = refs.iter().map(|(_, _, a)| a.as_str()).collect();
        assert!(
            agents.contains(&"tavily_researcher"),
            "tavily_researcher from manifest A must be present"
        );
        assert!(
            agents.contains(&"demo_assistant"),
            "demo_assistant from manifest B must be present"
        );
    }

    #[test]
    fn invalid_bytes_are_skipped() {
        let bad_bytes: &[u8] = b"not cbor at all";
        let refs = referenced_dw_agents(&[bad_bytes]);
        assert!(refs.is_empty(), "invalid CBOR must be silently skipped");
    }

    // --- provided_agent_ids (dw-agents.json sidecar) -------------------------

    /// Build a `dw-agents.json` sidecar byte fixture with the given agent ids.
    /// This is the same format packc writes:
    /// `serde_json::to_vec(&BTreeMap<String, serde_json::Value>)`.
    fn dw_agents_sidecar_bytes(agent_ids: &[&str]) -> Vec<u8> {
        let map: std::collections::BTreeMap<String, serde_json::Value> = agent_ids
            .iter()
            .map(|id| (id.to_string(), serde_json::json!({"kind": "placeholder"})))
            .collect();
        serde_json::to_vec(&map).expect("sidecar serialization must succeed")
    }

    #[test]
    fn provided_agent_ids_extracts_keys_from_json_sidecar() {
        // `{"tavily_researcher": {...}, "second_agent": {...}}` as JSON bytes
        let bytes = serde_json::to_vec(&serde_json::json!({
            "tavily_researcher": {"kind": "dw-agent", "llm": "openai"},
            "second_agent": {"kind": "dw-agent"}
        }))
        .unwrap();
        let ids = provided_agent_ids(&[&bytes]);
        assert!(
            ids.contains("tavily_researcher"),
            "tavily_researcher must be in the provided set"
        );
        assert!(
            ids.contains("second_agent"),
            "second_agent must be in the provided set"
        );
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn malformed_sidecar_blob_is_skipped_gracefully() {
        let bad: &[u8] = b"not json at all {{ broken";
        let ids = provided_agent_ids(&[bad]);
        assert!(
            ids.is_empty(),
            "a malformed sidecar must silently yield an empty set (no panic)"
        );
    }

    #[test]
    fn empty_json_object_yields_empty_set() {
        let bytes: &[u8] = b"{}";
        assert!(
            provided_agent_ids(&[bytes]).is_empty(),
            "an empty JSON object sidecar must yield an empty set"
        );
    }

    #[test]
    fn non_object_json_blob_yields_empty_set() {
        // A JSON array is valid JSON but not the expected object shape.
        let bytes = b"[\"some_key\"]";
        assert!(
            provided_agent_ids(&[bytes]).is_empty(),
            "a non-object JSON blob must be skipped (contributes nothing)"
        );
    }

    #[test]
    fn multiple_sidecars_are_aggregated() {
        let bytes_a = dw_agents_sidecar_bytes(&["agent_a"]);
        let bytes_b = dw_agents_sidecar_bytes(&["agent_b"]);
        let ids = provided_agent_ids(&[&bytes_a, &bytes_b]);
        assert!(
            ids.contains("agent_a"),
            "agent_a from sidecar A must be present"
        );
        assert!(
            ids.contains("agent_b"),
            "agent_b from sidecar B must be present"
        );
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn round_trip_guard_matches_packc_wire_format() {
        // Build the sidecar bytes EXACTLY as packc does in agent_pack.rs:
        // serde_json::to_vec(&BTreeMap<String, serde_json::Value>).
        // This pins the wire format and guards against future packc drift.
        let mut agents: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        agents.insert(
            "crm_assistant".to_string(),
            serde_json::json!({"kind": "dw-agent", "llm_provider": "openai"}),
        );
        agents.insert(
            "email_drafter".to_string(),
            serde_json::json!({"kind": "dw-agent", "llm_provider": "anthropic"}),
        );
        let sidecar_bytes =
            serde_json::to_vec(&agents).expect("packc-style serialization must succeed");

        let ids = provided_agent_ids(&[&sidecar_bytes]);
        assert!(
            ids.contains("crm_assistant"),
            "crm_assistant must round-trip through the packc wire format"
        );
        assert!(
            ids.contains("email_drafter"),
            "email_drafter must round-trip through the packc wire format"
        );
        assert_eq!(ids.len(), 2, "only the declared agents must be present");
    }
}
