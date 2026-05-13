//! Entry node detection.
//!
//! The flow's `start` is whichever Adaptive Card the user should land
//! on when the bundle is launched. Two observed shapes drive the
//! heuristic:
//!
//! 1. **First-cut flows** — the welcome card is the only or
//!    first-declared card with no card routes pointing at it. Sub-menus
//!    branch off it. This is the case the original `detect_entry`
//!    fix targeted (PR #99 in greentic-bundle): pick the no-incoming
//!    AdaptiveCard with the most outgoing routes; tie-break by
//!    declaration order.
//!
//! 2. **Established flows** — every sub-flow's last card has a
//!    "🏠 Return to Concierge Home" / "Cancel" / "Modify" submit, so
//!    the welcome card now has *high* incoming as well as high
//!    outgoing. Worse, an unhappy-path card (error-recovery) ends up
//!    being the only card with zero raw incoming, because nothing
//!    routes back to it. The first-cut heuristic loses to recovery in
//!    that case.
//!
//! The fix: try the *centrality* heuristic first — among cards that
//! are both menu-style (≥2 outgoing) and "returned to" (≥2 incoming),
//! pick the one with the highest (in_degree + out_degree) score; ties
//! by declaration order. Only when no card meets that threshold do we
//! fall back to the original "no-incoming + most-outgoing" logic. This
//! preserves every existing test and resolves the
//! support-ticket-router regression where `error_recovery` was beating
//! `digital_travel_concierge_home`.

use std::collections::HashMap;

use crate::errors::ConvertError;
use crate::types::{CardEntry, CardKind};

/// Returns the id of the card that should be the flow's `start` node.
pub fn detect_entry(cards: &[CardEntry]) -> Result<String, ConvertError> {
    if cards.is_empty() {
        return Err(ConvertError::NoCards);
    }

    let in_degree = compute_in_degrees(cards);

    // Pass 1: established-flow centrality. Pick the highest-score card
    // among "menu cards that several other cards return to". Both
    // thresholds are ≥3 — strict enough to skip small loops where every
    // node looks central (chatbot-loop golden: 3 cards with 2 in / 2
    // out each, none qualify, falls through to pass 3 which picks the
    // first declared menu) but loose enough to catch real welcome
    // cards (support-ticket-router: home has 4 in / 5 out, qualifies
    // and beats `error_recovery` and `modify_trip`).
    let mut best_central: Option<(usize, &CardEntry)> = None;
    for card in cards {
        if !matches!(card.kind, CardKind::AdaptiveCard(_)) {
            continue;
        }
        let outgoing = outgoing_route_count(card);
        let incoming = in_degree.get(card.id.as_str()).copied().unwrap_or(0);
        if outgoing < 3 || incoming < 3 {
            continue;
        }
        let score = outgoing + incoming;
        if best_central.map(|(s, _)| score > s).unwrap_or(true) {
            best_central = Some((score, card));
        }
    }
    if let Some((_, card)) = best_central {
        return Ok(card.id.clone());
    }

    // Pass 2: first-cut flow — no-incoming AdaptiveCard with the most
    // outgoing routes (PR #99's original heuristic).
    let mut best_root: Option<(usize, &CardEntry)> = None;
    for card in cards {
        if !matches!(card.kind, CardKind::AdaptiveCard(_)) {
            continue;
        }
        if in_degree.get(card.id.as_str()).copied().unwrap_or(0) > 0 {
            continue;
        }
        let outgoing = outgoing_route_count(card);
        if best_root.map(|(o, _)| outgoing > o).unwrap_or(true) {
            best_root = Some((outgoing, card));
        }
    }
    if let Some((_, card)) = best_root {
        return Ok(card.id.clone());
    }

    // Pass 3: full-cycle / no-clear-root — first menu card by
    // declaration order.
    if let Some(menu) = cards.iter().find(|c| is_menu_card(c)) {
        return Ok(menu.id.clone());
    }

    // Pass 4: any AdaptiveCard at all.
    if let Some(first) = cards
        .iter()
        .find(|c| matches!(c.kind, CardKind::AdaptiveCard(_)))
    {
        return Ok(first.id.clone());
    }

    Err(ConvertError::NoEntryCard { count: cards.len() })
}

fn compute_in_degrees(cards: &[CardEntry]) -> HashMap<String, usize> {
    let mut in_degree = HashMap::new();
    for card in cards {
        if matches!(card.kind, CardKind::AdaptiveCard(_)) {
            in_degree.insert(card.id.clone(), 0);
        }
    }
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
                && let Some(count) = in_degree.get_mut(target)
            {
                *count += 1;
            }
        }
    }
    in_degree
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

    #[test]
    fn established_flow_centrality_beats_orphan_recovery_card() {
        // Reproduces the support-ticket-router shape. Sub-flows return
        // to `concierge_home` via "🏠 Back to home" submits. An
        // error-recovery card is the only card with zero raw incoming
        // because nothing routes back to it. Pre-fix the no-incoming
        // heuristic picked recovery (4 outgoing, 0 raw incoming) over
        // home (5 outgoing, 4 raw incoming — all back-routes). The
        // dual heuristic now scores cards by `in + out` among those
        // that are both menus (≥2 out) AND returned-to (≥2 in), so
        // home (5 in + 5 out = 10) wins over recovery (0 in + 4 out =
        // doesn't qualify).
        let cards = vec![
            card(
                "concierge_home",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"plan_trip"}},
                        {"type":"Action.Submit","data":{"nextCardId":"review_itinerary"}},
                        {"type":"Action.Submit","data":{"nextCardId":"modify_trip"}},
                        {"type":"Action.Submit","data":{"nextCardId":"request_approval"}},
                        {"type":"Action.Submit","data":{"nextCardId":"cancel_exit"}}
                    ]
                }),
            ),
            card(
                "plan_trip",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"concierge_home"}}
                    ]
                }),
            ),
            card("review_itinerary", json!({"type":"AdaptiveCard"})),
            card("modify_trip", json!({"type":"AdaptiveCard"})),
            card(
                "request_approval",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"confirm_booking"}}
                    ]
                }),
            ),
            card(
                "confirm_booking",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"concierge_home"}}
                    ]
                }),
            ),
            card(
                "cancel_exit",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"exit_confirmed"}},
                        {"type":"Action.Submit","data":{"nextCardId":"concierge_home"}}
                    ]
                }),
            ),
            card(
                "exit_confirmed",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"concierge_home"}}
                    ]
                }),
            ),
            card(
                "error_recovery",
                json!({
                    "type":"AdaptiveCard",
                    "actions":[
                        {"type":"Action.Submit","data":{"nextCardId":"plan_trip"}},
                        {"type":"Action.Submit","data":{"nextCardId":"review_itinerary"}},
                        {"type":"Action.Submit","data":{"nextCardId":"concierge_home"}},
                        {"type":"Action.Submit","data":{"nextCardId":"modify_trip"}}
                    ]
                }),
            ),
        ];
        assert_eq!(detect_entry(&cards).unwrap(), "concierge_home");
    }
}
