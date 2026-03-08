use anyhow::{Result, bail};
use semver::Version;

use crate::answers::document::AnswerDocument;

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub document: AnswerDocument,
    pub migrated: bool,
}

pub fn migrate_document(
    mut document: AnswerDocument,
    target_version: &Version,
) -> Result<MigrationResult> {
    if &document.schema_version == target_version {
        return Ok(MigrationResult {
            document,
            migrated: false,
        });
    }
    if document.schema_version > *target_version {
        bail!("{}", crate::i18n::tr("errors.answer_document.downgrade"));
    }

    document.schema_version = target_version.clone();
    Ok(MigrationResult {
        document,
        migrated: true,
    })
}
