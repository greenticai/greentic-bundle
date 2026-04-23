//! Public convert() orchestrator.

use crate::emit::emit_ygtc;
use crate::entry::detect_entry;
use crate::errors::ConvertError;
use crate::http_inject::inject_http_nodes;
use crate::routing::build_routing;
use crate::types::{CardEntry, ConvertOptions, ConvertResult};

pub fn convert(cards: &[CardEntry], opts: &ConvertOptions) -> Result<ConvertResult, ConvertError> {
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

    Ok(ConvertResult {
        flow_yaml,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CardKind, ConvertOptions};
    use serde_json::json;

    #[test]
    fn happy_path_two_cards() {
        let cards = vec![
            CardEntry {
                id: "welcome".into(),
                kind: CardKind::AdaptiveCard(json!({
                    "actions":[{"type":"Action.Submit","data":{"routeToCardId":"thanks","action_id":"go"}}]
                })),
            },
            CardEntry {
                id: "thanks".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
        ];
        let res = convert(
            &cards,
            &ConvertOptions {
                flow_name: "demo".into(),
                strict: false,
            },
        )
        .unwrap();
        assert!(res.flow_yaml.contains("start: welcome"));
        assert!(res.diagnostics.is_empty());
    }

    #[test]
    fn empty_cards_errors() {
        let err = convert(
            &[],
            &ConvertOptions {
                flow_name: "x".into(),
                strict: false,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "E_NO_CARDS");
    }

    #[test]
    fn unreachable_card_emits_diagnostic() {
        let cards = vec![
            CardEntry {
                id: "a".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
            CardEntry {
                id: "orphan".into(),
                kind: CardKind::AdaptiveCard(json!({})),
            },
        ];
        let res = convert(
            &cards,
            &ConvertOptions {
                flow_name: "demo".into(),
                strict: false,
            },
        )
        .unwrap();
        assert!(
            res.diagnostics
                .iter()
                .any(|d| matches!(d.kind, crate::types::DiagnosticKind::UnreachableCard))
        );
    }
}
