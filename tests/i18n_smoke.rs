use assert_cmd::Command;
use greentic_bundle::i18n;
use predicates::prelude::*;

fn bundle_bin() -> &'static str {
    env!("CARGO_BIN_EXE_greentic-bundle")
}

#[test]
fn locale_fallback_uses_embedded_english_catalog() {
    assert_eq!(
        greentic_bundle::i18n::tr_for("en-US", "cli.build.about"),
        "Build a deterministic .gtbundle artifact"
    );
    assert_eq!(
        greentic_bundle::i18n::tr_for("zz-ZZ", "cli.build.about"),
        "Build a deterministic .gtbundle artifact"
    );
}

#[test]
fn root_help_uses_localized_embedded_strings() {
    let mut cmd = Command::new(bundle_bin());
    cmd.arg("--locale").arg("en-US").arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Scaffold for bundle authoring commands, localized help, answer documents, and future .gtbundle build flows.",
        ))
        .stdout(predicate::str::contains("Bundle wizard helpers"))
        .stdout(predicate::str::contains(
            "Locale used for CLI and wizard messages",
        ));
}

#[test]
fn dutch_locale_changes_wizard_menu_strings() {
    let mut cmd = Command::new(bundle_bin());
    cmd.args(["--locale", "nl", "wizard", "run", "--dry-run"]);
    cmd.write_stdin("1\nDemo Bundle\ndemo-bundle\n/tmp/demo-bundle-nl\n1\npack-a\n1\n1\n4\n4\n2\n");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Bundle-wizard"))
        .stdout(predicate::str::contains("1. maken"))
        .stdout(predicate::str::contains("Kies nummer of waarde:"))
        .stdout(predicate::str::contains("Bundlenaam"));
}

#[test]
fn wizard_run_help_shows_replay_flags() {
    let mut cmd = Command::new(bundle_bin());
    cmd.args(["wizard", "run", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--answers <FILE>"))
        .stdout(predicate::str::contains("--emit-answers <FILE>"))
        .stdout(predicate::str::contains("--schema-version <VER>"))
        .stdout(predicate::str::contains("--migrate"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn embedded_catalogs_exist_for_required_smoke_locales() {
    for locale in ["en", "ar", "ja", "en-GB"] {
        let catalog = i18n::load_catalog(locale).expect("embedded locale");
        assert!(catalog.contains_key("cli.root.about"));
        assert!(catalog.contains_key("wizard.menu.title"));
    }
}

#[test]
fn locale_selection_prefers_cli_then_base_language_then_en() {
    let supported = i18n::supported_locales();
    assert_eq!(
        i18n::select_locale(Some("ar-SA".to_string()), &supported),
        "ar-SA".to_string()
    );
    assert_eq!(
        i18n::select_locale(Some("ja-JP".to_string()), &supported),
        "ja".to_string()
    );
    assert_eq!(
        i18n::select_locale(Some("zz-ZZ".to_string()), &supported),
        "en".to_string()
    );
}
