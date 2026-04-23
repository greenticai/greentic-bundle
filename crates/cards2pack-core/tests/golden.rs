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
