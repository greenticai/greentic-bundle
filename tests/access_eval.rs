use greentic_bundle::access::{GmapPath, Policy, eval_policy, eval_with_overlay, parse_str};

fn target(pack: &str, flow: Option<&str>, node: Option<&str>) -> GmapPath {
    GmapPath {
        pack: Some(pack.to_string()),
        flow: flow.map(ToOwned::to_owned),
        node: node.map(ToOwned::to_owned),
    }
}

#[test]
fn specific_rule_overrides_default_rule() {
    let rules = parse_str("_ = forbidden\npack-a/main = public\n").expect("parse rules");
    let decision = eval_policy(&rules, &target("pack-a", Some("main"), None)).expect("decision");
    assert_eq!(decision.policy, Policy::Public);
    assert_eq!(decision.rank, 4);
}

#[test]
fn later_rule_wins_for_same_specificity() {
    let rules = parse_str("pack-a = forbidden\npack-a = public\n").expect("parse rules");
    let decision = eval_policy(&rules, &target("pack-a", None, None)).expect("decision");
    assert_eq!(decision.policy, Policy::Public);
    assert_eq!(decision.rank, 3);
}

#[test]
fn team_overlay_wins_over_tenant_rules() {
    let tenant_rules = parse_str("_ = forbidden\npack-a/main = forbidden\n").expect("tenant");
    let team_rules = parse_str("pack-a/main = public\n").expect("team");
    let decision = eval_with_overlay(
        &tenant_rules,
        &team_rules,
        &target("pack-a", Some("main"), None),
    )
    .expect("decision");
    assert_eq!(decision.policy, Policy::Public);
}
