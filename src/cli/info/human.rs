use super::report::*;
use std::fmt::Write;

pub fn render(r: &InfoReport) -> String {
    let mut s = String::new();
    let header = match &r.version {
        Some(v) => format!("{} {} · {}", r.name, v, r.mode),
        None => format!("{} · {}", r.name, r.mode),
    };
    let _ = writeln!(s, "{header}");
    if let Some(d) = &r.description {
        let _ = writeln!(s, "{d}");
    }
    let _ = writeln!(s);

    kv(&mut s, "Bundle ID", &r.bundle_id);
    kv(&mut s, "Mode", &r.mode);
    if !r.locale.is_empty() {
        kv(&mut s, "Locale", &r.locale);
    }

    if !r.app_packs.is_empty() {
        let _ = writeln!(s, "\nApp packs ({})", r.app_packs.len());
        for p in &r.app_packs {
            render_pack(&mut s, p);
        }
    }
    if !r.extension_providers.is_empty() {
        let _ = writeln!(s, "\nExtension providers ({})", r.extension_providers.len());
        for p in &r.extension_providers {
            render_pack(&mut s, p);
        }
    }
    if !r.catalogs.is_empty() {
        let _ = writeln!(s, "\nCatalogs ({})", r.catalogs.len());
        for c in &r.catalogs {
            let _ = writeln!(s, "  {:<20} {} items", c.name, c.item_count);
        }
    }

    let _ = writeln!(
        s,
        "\nAccess ({} tenants, {} teams)",
        r.access.tenants, r.access.teams
    );
    for t in &r.access.targets {
        let teams_text = if t.team_count == 1 {
            "1 team".to_string()
        } else {
            format!("{} teams", t.team_count)
        };
        let _ = writeln!(
            s,
            "  {:<12} {:<8} ({})",
            t.tenant, teams_text, t.default_policy
        );
    }

    if !r.capabilities.is_empty() {
        kv_block(&mut s, "Capabilities", &r.capabilities.join(", "));
    }
    if !r.hooks.is_empty() {
        kv_block(&mut s, "Hooks", &r.hooks.join(", "));
    }
    if !r.subscriptions.is_empty() {
        kv_block(&mut s, "Subscriptions", &r.subscriptions.join(", "));
    }

    s
}

fn kv(s: &mut String, label: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let _ = writeln!(s, "{:<14} {}", label, value);
}

fn kv_block(s: &mut String, label: &str, value: &str) {
    // Like kv but prefixed with a blank-line separator, used for trailing rows.
    let _ = writeln!(s, "\n{:<14} {}", label, value);
}

fn render_pack(s: &mut String, p: &PackRef) {
    let digest_display = match &p.digest {
        Some(d) if d.len() > 24 => format!("{}…", &d[..24]),
        Some(d) => d.clone(),
        None => "(no digest)".into(),
    };
    let _ = writeln!(s, "  {:<18} {}", p.reference, digest_display);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> InfoReport {
        InfoReport {
            info_schema_version: 1,
            bundle_id: "acme".into(),
            name: "acme-demo".into(),
            version: None,
            description: Some("Demo bundle for ACME.".into()),
            mode: "production".into(),
            locale: "en".into(),
            app_packs: vec![
                PackRef {
                    reference: "hello-bot".into(),
                    version: None,
                    digest: Some("sha256:abcdef123456abcdef".into()),
                },
                PackRef {
                    reference: "support-bot".into(),
                    version: None,
                    digest: None,
                },
            ],
            extension_providers: vec![PackRef {
                reference: "slack-provider".into(),
                version: None,
                digest: Some("sha256:9ab0cd1234".into()),
            }],
            catalogs: vec![CatalogRef {
                name: "catalog.json".into(),
                item_count: 12,
            }],
            access: AccessSummary {
                tenants: 2,
                teams: 3,
                targets: vec![
                    AccessTarget {
                        tenant: "default".into(),
                        team_count: 2,
                        default_policy: "public".into(),
                    },
                    AccessTarget {
                        tenant: "acme".into(),
                        team_count: 1,
                        default_policy: "forbidden".into(),
                    },
                ],
            },
            capabilities: vec!["state.kv".into(), "secrets".into()],
            hooks: vec!["on_install".into()],
            subscriptions: vec![],
        }
    }

    #[test]
    fn renders_header_mode_and_description() {
        let out = render(&sample());
        assert!(out.contains("acme-demo · production"));
        assert!(out.contains("Demo bundle for ACME."));
    }

    #[test]
    fn renders_packs_with_digest_and_fallback() {
        let out = render(&sample());
        assert!(out.contains("hello-bot"));
        assert!(out.contains("sha256:abcdef123456abcde…")); // truncated at 20 chars + …
        assert!(out.contains("support-bot"));
        assert!(out.contains("(no digest)"));
    }

    #[test]
    fn renders_access_summary() {
        let out = render(&sample());
        assert!(out.contains("Access (2 tenants, 3 teams)"));
        assert!(out.contains("default"));
        assert!(out.contains("2 teams"));
        assert!(out.contains("(public)"));
        assert!(out.contains("acme"));
        assert!(out.contains("1 team"));
        assert!(out.contains("(forbidden)"));
    }

    #[test]
    fn omits_empty_sections() {
        let mut r = sample();
        r.app_packs.clear();
        r.extension_providers.clear();
        r.catalogs.clear();
        r.capabilities.clear();
        r.hooks.clear();
        r.subscriptions.clear();
        let out = render(&r);
        assert!(!out.contains("App packs"));
        assert!(!out.contains("Extension providers"));
        assert!(!out.contains("Catalogs"));
        assert!(!out.contains("Capabilities"));
        assert!(!out.contains("Hooks"));
        assert!(!out.contains("Subscriptions"));
        // Access section is always present, even with zero targets.
        assert!(out.contains("Access (2 tenants, 3 teams)"));
    }

    #[test]
    fn renders_with_version_in_header() {
        let mut r = sample();
        r.version = Some("0.3.0".into());
        let out = render(&r);
        assert!(out.contains("acme-demo 0.3.0 · production"));
    }
}
