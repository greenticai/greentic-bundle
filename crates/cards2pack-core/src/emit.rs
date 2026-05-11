//! Emit YGTc 2.0 YAML from routing graph + HTTP nodes.

use crate::errors::ConvertError;
use crate::http_inject::HttpNode;
use crate::routing::{RouteEdge, RoutingGraph};
use crate::types::CardEntry;
use serde::Serialize;
use std::collections::BTreeMap;

const HTTP_COMPONENT_REF: &str = "oci://ghcr.io/greenticai/component/component-http:stable";

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
    /// Multi-route node: each entry pairs a target with a `condition`
    /// expression that must evaluate true. The field is named
    /// `condition` to match `greentic-flow`'s YGTC schema (legacy code
    /// emitted `when`, which `additionalProperties: false` rejects —
    /// runtime then silently fails to load the flow and autoStart
    /// never fires the start node).
    Conditional {
        to: String,
        condition: String,
    },
    Unconditional {
        to: String,
    },
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
        return vec![RouteYaml::Unconditional {
            to: edges[0].target.clone(),
        }];
    }
    edges
        .iter()
        .map(|e| RouteYaml::Conditional {
            to: e.target.clone(),
            condition: format!("action.action_id == \"{}\"", e.action_id),
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
        let occurrences = yaml.matches("card_source: asset").count();
        assert_eq!(occurrences, 1, "duplicate flat fields detected:\n{yaml}");
    }

    #[test]
    fn multi_route_emits_conditional_when() {
        use crate::routing::RouteEdge;
        let cards = vec![
            CardEntry {
                id: "menu".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
            CardEntry {
                id: "a".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
            CardEntry {
                id: "b".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
        ];
        let mut routing = RoutingGraph::default();
        routing.edges.insert(
            "menu".into(),
            vec![
                RouteEdge {
                    action_id: "go_a".into(),
                    target: "a".into(),
                },
                RouteEdge {
                    action_id: "go_b".into(),
                    target: "b".into(),
                },
            ],
        );
        let yaml = emit_ygtc(&cards, "menu", &routing, &[], "demo").unwrap();
        assert!(yaml.contains(r#"condition: action.action_id == "go_a""#));
        assert!(yaml.contains(r#"condition: action.action_id == "go_b""#));
    }
}
