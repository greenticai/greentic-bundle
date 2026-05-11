#[test]
fn support_ticket_router_fixture_starts_at_concierge_home() {
    let raw = std::fs::read_to_string("tests/fixtures/support_ticket_router/cards.json")
        .expect("fixture");
    let cards = cards2pack_core::parse_cards(&raw).expect("parse");
    let res = cards2pack_core::convert(
        &cards,
        &cards2pack_core::ConvertOptions {
            flow_name: "support-ticket-router".into(),
            strict: false,
        },
    )
    .expect("convert");
    let start_line = res
        .flow_yaml
        .lines()
        .find(|l| l.starts_with("start:"))
        .unwrap_or("(none)");
    println!("{}", start_line);
    assert!(
        res.flow_yaml
            .contains("start: digital_travel_concierge_home")
    );
}
