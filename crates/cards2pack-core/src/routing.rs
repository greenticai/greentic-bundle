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
        CardEntry {
            id: id.into(),
            kind: CardKind::AdaptiveCard(json),
        }
    }

    #[test]
    fn edges_preserve_declaration_order() {
        let cards = vec![
            card(
                "welcome",
                json!({
                    "actions":[
                        {"type":"Action.Submit","data":{"routeToCardId":"a","action_id":"go_a"}},
                        {"type":"Action.Submit","data":{"routeToCardId":"b","action_id":"go_b"}}
                    ]
                }),
            ),
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
            card(
                "welcome",
                json!({"actions":[{"type":"Action.Submit","data":{"routeToCardId":"chat"}}]}),
            ),
            card(
                "chat",
                json!({"actions":[{"type":"Action.Submit","data":{"routeToCardId":"welcome"}}]}),
            ),
        ];
        let (g, _) = build_routing(&cards, false).unwrap();
        let chat_edges = g.edges.get("chat").unwrap();
        assert_eq!(chat_edges[0].target, "welcome");
    }

    #[test]
    fn dangling_route_strict_errors() {
        let cards = vec![card(
            "welcome",
            json!({"actions":[
                {"type":"Action.Submit","data":{"routeToCardId":"missing"}}
            ]}),
        )];
        let err = build_routing(&cards, true).unwrap_err();
        assert_eq!(err.code(), "E_DANGLING_ROUTE");
    }

    #[test]
    fn dangling_route_lenient_diagnostic() {
        let cards = vec![card(
            "welcome",
            json!({"actions":[
                {"type":"Action.Submit","data":{"routeToCardId":"missing"}}
            ]}),
        )];
        let (g, diags) = build_routing(&cards, false).unwrap();
        assert!(g.edges.is_empty());
        assert_eq!(diags.len(), 1);
        assert!(matches!(diags[0].kind, DiagnosticKind::DanglingRoute));
    }

    #[test]
    fn synthesizes_action_id_when_missing() {
        let cards = vec![
            card(
                "welcome",
                json!({"actions":[
                    {"type":"Action.Submit","data":{"routeToCardId":"target"}}
                ]}),
            ),
            card("target", json!({})),
        ];
        let (g, _) = build_routing(&cards, false).unwrap();
        assert_eq!(g.edges.get("welcome").unwrap()[0].action_id, "goto_target");
    }
}
