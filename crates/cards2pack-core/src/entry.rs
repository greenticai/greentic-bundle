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
