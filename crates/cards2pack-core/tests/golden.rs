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

#[test]
fn chatbot_loop_golden() {
    let yaml = run_fixture("chatbot_loop");
    insta::assert_snapshot!("chatbot_loop", yaml);
}

#[test]
fn chatbot_loop_back_edges_preserved() {
    let yaml = run_fixture("chatbot_loop");
    // chat_reply → chat_input is a back-edge; must NOT be stripped.
    assert!(yaml.contains(r#"to: chat_input"#));
    // chat_input → welcome is also a back-edge.
    assert!(yaml.contains(r#"to: welcome"#));
}

#[test]
fn http_form_golden() {
    let yaml = run_fixture("http_form");
    insta::assert_snapshot!("http_form", yaml);
}

#[test]
fn http_form_emits_component_exec() {
    let yaml = run_fixture("http_form");
    assert!(yaml.contains("component-http"));
    assert!(yaml.contains("https://api.example.com/submit"));
}

#[test]
fn multi_form_golden() {
    let yaml = run_fixture("multi_form");
    insta::assert_snapshot!("multi_form", yaml);
}

#[test]
fn multi_form_three_conditional_routes() {
    let yaml = run_fixture("multi_form");
    let when_count = yaml.matches("when: action.action_id ==").count();
    assert!(when_count >= 3, "expected >=3 conditional routes from menu; got {when_count}");
}
