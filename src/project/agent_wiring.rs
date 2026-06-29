//! Extract `dw.agent` references from compiled flow manifests and collect
//! agent ids already provided by `dw-application` packs.
//!
//! These two helpers are the read-side primitives for Task 5's auto-wiring
//! resolution pass:
//!
//! * [`referenced_dw_agents`] — scans `manifest.cbor` bytes from application
//!   packs and returns every `(flow_id, node_id, agent_id)` triple where a node
//!   has `component.id == "dw.agent"` and a non-empty `component.operation`.
//!
//! * [`provided_agent_ids`] — reads `manifest.cbor` bytes from the bundle's
//!   existing packs, filters to those with `kind == "dw-application"`, and
//!   returns the string keys of their `agents` map.
//!
//! ## IR shape
//!
//! Both functions consume raw CBOR bytes already extracted from a `.gtpack`
//! ZIP.  The manifest.cbor IR uses interned symbol tables: `node.component.id`
//! is an integer index into `symbols.component_ids`, and `node.id` is an
//! integer index into `symbols.node_ids`.  Inline string ids are accepted as a
//! defensive fallback (matching the rule in `greentic-start`'s
//! `agent_preflight`).
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

/// Pack kind that declares one or more agentic workers.
const DW_APPLICATION_KIND: &str = "dw-application";

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

/// Scan `pack_manifests` (raw `manifest.cbor` bytes, one per pack) and collect
/// agent ids declared by every `dw-application` pack.
///
/// For each byte slice:
///
/// 1. Decode as CBOR.
/// 2. Skip unless `kind == "dw-application"`.
/// 3. Collect the string keys of the `agents` map.
///
/// A dw-application pack may provide more than one agent id; all keys are
/// returned.  Invalid / unreadable slices and non-dw-application packs are
/// silently skipped (fail-soft).
///
/// Mirrors the concept of `greentic-start`'s `dw_agents_from_bundle` /
/// `provided_agent_from_pack`.
pub(crate) fn provided_agent_ids(pack_manifests: &[&[u8]]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for bytes in pack_manifests {
        let Ok(manifest) = ciborium::de::from_reader::<Value, _>(*bytes) else {
            continue;
        };
        collect_provided_agent_ids(&manifest, &mut ids);
    }
    ids
}

/// Collect agent ids from one decoded `manifest.cbor` if it is a
/// `dw-application` pack.
fn collect_provided_agent_ids(manifest: &Value, ids: &mut BTreeSet<String>) {
    let Some(kind) = map_get(manifest, "kind").and_then(as_text) else {
        return;
    };
    if kind != DW_APPLICATION_KIND {
        return;
    }
    let Some(Value::Map(agents_map)) = map_get(manifest, "agents") else {
        return;
    };
    for (key, _) in agents_map {
        if let Some(agent_id) = as_text(key)
            && !agent_id.is_empty()
        {
            ids.insert(agent_id);
        }
    }
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

    /// A `manifest.cbor` for a `dw-application` pack providing `tavily_researcher`.
    fn dw_application_manifest_bytes(agent_id: &str) -> Vec<u8> {
        let agents_map = cbor_map(vec![(agent_id, text("agent-config-placeholder"))]);
        let manifest = cbor_map(vec![
            ("kind", text("dw-application")),
            ("agents", agents_map),
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

    // --- provided_agent_ids --------------------------------------------------

    #[test]
    fn returns_agent_ids_from_dw_application_pack() {
        let bytes = dw_application_manifest_bytes("tavily_researcher");
        let ids = provided_agent_ids(&[&bytes]);
        assert!(
            ids.contains("tavily_researcher"),
            "tavily_researcher must be in the provided set"
        );
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn multi_agent_dw_application_pack_returns_all_agents() {
        let agents_map = cbor_map(vec![
            ("agent_one", text("config-a")),
            ("agent_two", text("config-b")),
        ]);
        let manifest = cbor_map(vec![
            ("kind", text("dw-application")),
            ("agents", agents_map),
        ]);
        let bytes = to_cbor_bytes(&manifest);
        let ids = provided_agent_ids(&[&bytes]);
        assert!(ids.contains("agent_one"));
        assert!(ids.contains("agent_two"));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn non_dw_application_pack_is_skipped() {
        let manifest = cbor_map(vec![
            ("kind", text("application")),
            ("agents", cbor_map(vec![("some_agent", text("config"))])),
        ]);
        let bytes = to_cbor_bytes(&manifest);
        assert!(
            provided_agent_ids(&[&bytes]).is_empty(),
            "packs with kind != dw-application must be skipped"
        );
    }

    #[test]
    fn pack_without_kind_is_skipped() {
        let manifest = cbor_map(vec![(
            "agents",
            cbor_map(vec![("some_agent", text("config"))]),
        )]);
        let bytes = to_cbor_bytes(&manifest);
        assert!(
            provided_agent_ids(&[&bytes]).is_empty(),
            "packs without a kind field must be skipped"
        );
    }

    #[test]
    fn dw_application_pack_without_agents_map_returns_empty() {
        let manifest = cbor_map(vec![("kind", text("dw-application"))]);
        let bytes = to_cbor_bytes(&manifest);
        assert!(
            provided_agent_ids(&[&bytes]).is_empty(),
            "a dw-application pack with no agents map yields an empty set"
        );
    }

    #[test]
    fn multiple_packs_are_scanned() {
        let bytes_a = dw_application_manifest_bytes("agent_a");
        let bytes_b = dw_application_manifest_bytes("agent_b");
        let ids = provided_agent_ids(&[&bytes_a, &bytes_b]);
        assert!(ids.contains("agent_a"));
        assert!(ids.contains("agent_b"));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn invalid_pack_bytes_are_skipped() {
        let bad: &[u8] = b"not cbor";
        assert!(provided_agent_ids(&[bad]).is_empty());
    }
}
