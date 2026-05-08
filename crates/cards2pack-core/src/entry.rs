//! Entry node detection.
//!
//! The flow's `start` is whichever Adaptive Card the user should land on
//! when the bundle is launched. We pick it topologically: a card that no
//! other card routes to is a root. Among multiple roots, the most
//! "branchy" one (most outgoing routes) wins, which on real bundles is
//! the welcome menu rather than a sub-flow leaf.
//!
//! The previous heuristic walked declaration order looking for the first
//! menu-style card (≥2 outgoing routes). On the Travel-Concierge bundle
//! that surfaced "Restaurant Reservations" before "Digital Travel
//! Concierge" because the sub-menu happened to come first in the
//! generator's output — the welcome card is reachable from itself plus
//! every sub-menu, but declaration order did not encode that.

use std::collections::HashSet;

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind};

/// Returns the id of the card that should be the flow's `start` node.
pub fn detect_entry(cards: &[CardEntry]) -> Result<String, ConvertError> {
    if cards.is_empty() {
        return Err(ConvertError::NoCards);
    }

    let targeted = collect_route_targets(cards);

    let mut best_root: Option<(usize, &CardEntry)> = None;
    for card in cards {
        if !matches!(card.kind, CardKind::AdaptiveCard(_)) {
            continue;
        }
        if targeted.contains(card.id.as_str()) {
            continue;
        }
        let outgoing = outgoing_route_count(card);
        let replace = match best_root {
            None => true,
            Some((best_outgoing, _)) => outgoing > best_outgoing,
        };
        if replace {
            best_root = Some((outgoing, card));
        }
    }
    if let Some((_, card)) = best_root {
        return Ok(card.id.clone());
    }

    if let Some(menu) = cards.iter().find(|c| is_menu_card(c)) {
        return Ok(menu.id.clone());
    }

    if let Some(first) = cards
        .iter()
        .find(|c| matches!(c.kind, CardKind::AdaptiveCard(_)))
    {
        return Ok(first.id.clone());
    }

    Err(ConvertError::NoEntryCard { count: cards.len() })
}

fn collect_route_targets(cards: &[CardEntry]) -> HashSet<&str> {
    let mut targets = HashSet::new();
    for card in cards {
        let CardKind::AdaptiveCard(json) = &card.kind else {
            continue;
        };
        let Some(actions) = json.get("actions").and_then(|a| a.as_array()) else {
            continue;
        };
        for action in actions {
            if action.get("type").and_then(|v| v.as_str()) != Some("Action.Submit") {
                continue;
            }
            if let Some(target) = action
                .get("data")
                .and_then(|d| d.get("nextCardId").or_else(|| d.get("routeToCardId")))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                targets.insert(target);
            }
        }
    }
    targets
}

fn outgoing_route_count(card: &CardEntry) -> usize {
    let CardKind::AdaptiveCard(json) = &card.kind else {
        return 0;
    };
    let Some(actions) = json.get("actions").and_then(|a| a.as_array()) else {
        return 0;
    };
    actions
        .iter()
        .filter(|a| {
            a.get("type").and_then(|v| v.as_str()) == Some("Action.Submit")
                && a.get("data")
                    .and_then(|d| d.get("nextCardId").or_else(|| d.get("routeToCardId")))
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
        })
        .count()
}

fn is_menu_card(card: &CardEntry) -> bool {
    outgoing_route_count(card) >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(id: &str, json: serde_json::Value) -> CardEntry {
        CardEntry {
            id: id.into(),
            kind: CardKind::AdaptiveCard(json),
        }
    }

    #[test]
    fn picks_menu_card_over_first() {
        let cards = vec![
            card("intro", json!({"type":"AdaptiveCard"})),
            card(
                "welcome",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"routeToCardId":"a"}},
                        {"type":"Action.Submit","data":{"routeToCardId":"b"}}
                    ]
                }),
            ),
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
            card(
                "greeter",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[{"type":"Action.Submit","data":{"routeToCardId":"next"}}]
                }),
            ),
            card("after", json!({"type":"AdaptiveCard"})),
        ];
        // greeter has no incoming route, after is targeted by greeter →
        // greeter wins as the only root.
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

    #[test]
    fn picks_menu_card_using_next_card_id() {
        // Cards emitted by the AC extension carry `data.nextCardId`,
        // not `data.routeToCardId`.
        let cards = vec![
            card("intro", json!({"type":"AdaptiveCard"})),
            card(
                "welcome",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"a"}},
                        {"type":"Action.Submit","data":{"nextCardId":"b"}}
                    ]
                }),
            ),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "welcome");
    }

    #[test]
    fn topological_prefers_root_menu_over_inner_menu() {
        // Reproduces the Travel-Concierge bundle shape: the welcome
        // card branches into specialised sub-menus, each of which is
        // also a menu (≥2 routes). Declaration order lists the
        // sub-menu first, so the previous heuristic returned the
        // wrong start. The topological heuristic must pick the
        // welcome because nothing routes to it.
        let cards = vec![
            card(
                "restaurant_reservations",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"reserve_a"}},
                        {"type":"Action.Submit","data":{"nextCardId":"reserve_b"}}
                    ]
                }),
            ),
            card(
                "concierge_welcome",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"restaurant_reservations"}},
                        {"type":"Action.Submit","data":{"nextCardId":"hotel_booking"}},
                        {"type":"Action.Submit","data":{"nextCardId":"flight_search"}}
                    ]
                }),
            ),
            card(
                "hotel_booking",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"hotel_a"}},
                        {"type":"Action.Submit","data":{"nextCardId":"hotel_b"}}
                    ]
                }),
            ),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "concierge_welcome");
    }

    #[test]
    fn falls_back_to_first_menu_on_full_cycle() {
        // Pathological: every card has an incoming route, so the
        // topological phase finds no root. Falls back to the first
        // menu-style card by declaration order.
        let cards = vec![
            card(
                "loop_a",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"loop_b"}},
                        {"type":"Action.Submit","data":{"nextCardId":"loop_b"}}
                    ]
                }),
            ),
            card(
                "loop_b",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"loop_a"}},
                        {"type":"Action.Submit","data":{"nextCardId":"loop_a"}}
                    ]
                }),
            ),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "loop_a");
    }
}
