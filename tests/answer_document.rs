use semver::Version;
use serde_json::json;

use greentic_bundle::answers::{AnswerDocument, migrate::migrate_document};

#[test]
fn answer_document_round_trips_deterministically() {
    let mut document = AnswerDocument::new("en-US");
    document
        .answers
        .insert("bundle_name".to_string(), json!("demo"));
    document
        .answers
        .insert("advanced".to_string(), json!(false));
    document
        .locks
        .insert("catalog".to_string(), json!("pending"));

    let rendered = document
        .to_pretty_json_string()
        .expect("render answer document");
    let reparsed = AnswerDocument::from_json_str(&rendered).expect("reparse answer document");

    assert_eq!(reparsed.locale, "en-US");
    assert_eq!(reparsed, document);
    assert!(rendered.contains("\"answers\": {"));
    assert!(
        rendered.find("\"advanced\"").expect("advanced field")
            < rendered.find("\"bundle_name\"").expect("bundle name field")
    );
}

#[test]
fn schema_version_uses_semver_and_migration_stub_updates_target() {
    let mut document = AnswerDocument::new("en");
    document.schema_version = Version::new(1, 0, 0);

    let migrated = migrate_document(document.clone(), &Version::new(1, 1, 0)).expect("migrate");
    assert!(migrated.migrated);
    assert_eq!(migrated.document.schema_version, Version::new(1, 1, 0));

    let unchanged = migrate_document(document, &Version::new(1, 0, 0)).expect("no-op migration");
    assert!(!unchanged.migrated);
}
