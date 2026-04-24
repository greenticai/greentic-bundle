//! Helpers shared by the `ext` subcommand handlers.
//!
//! Kept in a sibling module so `cli/mod.rs` stays under the 500-line guideline.

#![cfg(feature = "extensions")]

use crate::ext::errors::ExtensionError;

/// Read a JSON payload from a filesystem path or stdin (when `"-"`).
pub(crate) fn read_input(path_or_dash: &str) -> std::io::Result<String> {
    use std::io::Read;
    if path_or_dash == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path_or_dash)
    }
}

/// Prints a JSON error line to stdout when `json` is true, then returns the
/// error for propagation (stderr + non-zero exit).
pub(crate) fn fail_json(json: bool, code: &str, message: &str) -> anyhow::Error {
    if json {
        let line = serde_json::json!({
            "status": "error",
            "code": code,
            "message": message,
        });
        println!("{line}");
    }
    anyhow::anyhow!("{code}: {message}")
}

/// Stable machine-readable code for each `ExtensionError` variant. Exhaustive
/// so new variants force a compile error here.
pub(crate) fn extension_error_code(err: &ExtensionError) -> &'static str {
    use ExtensionError::*;
    match err {
        NotFound(_) => "extension-not-found",
        RecipeNotFound { .. } => "recipe-not-found",
        InvalidConfig(_) => "invalid-config",
        InvalidDescriptor(_) => "invalid-descriptor",
        Conflict(_) => "conflict",
        ModeBNotImplemented => "mode-b-not-implemented",
        Internal(_) => "internal-error",
        Io(_) => "io-error",
        Json(_) => "invalid-json",
    }
}
