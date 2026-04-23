//! Synthesize HTTP flow nodes from `CardKind::Http` entries.

use crate::routing::{RouteEdge, RoutingGraph};
use crate::types::{CardEntry, CardKind, HttpConfig};

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
