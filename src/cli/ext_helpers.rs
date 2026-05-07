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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn read_input_returns_file_contents() {
        let mut tmp = tempfile::NamedTempFile::new().expect("temp file");
        tmp.write_all(b"hello-payload").expect("write");
        let path = tmp.path().to_string_lossy().into_owned();
        let got = read_input(&path).expect("read");
        assert_eq!(got, "hello-payload");
    }

    #[test]
    fn read_input_propagates_missing_file() {
        let err = read_input("/definitely/not/a/real/path-for-ext-helpers-test")
            .expect_err("missing path must error");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn fail_json_returns_anyhow_with_code_prefix() {
        let err = fail_json(false, "bad-thing", "details here");
        assert_eq!(err.to_string(), "bad-thing: details here");
    }

    #[test]
    fn extension_error_code_covers_each_variant() {
        let recipe = ExtensionError::RecipeNotFound {
            ext: "x".into(),
            recipe: "y".into(),
        };
        let json_err: ExtensionError = serde_json::from_str::<serde_json::Value>("{nope")
            .unwrap_err()
            .into();
        let io_err: ExtensionError = std::io::Error::other("boom").into();
        let cases: &[(ExtensionError, &str)] = &[
            (
                ExtensionError::NotFound("ext".into()),
                "extension-not-found",
            ),
            (recipe, "recipe-not-found"),
            (ExtensionError::InvalidConfig("c".into()), "invalid-config"),
            (
                ExtensionError::InvalidDescriptor("d".into()),
                "invalid-descriptor",
            ),
            (ExtensionError::Conflict("r".into()), "conflict"),
            (
                ExtensionError::ModeBNotImplemented,
                "mode-b-not-implemented",
            ),
            (ExtensionError::Internal("i".into()), "internal-error"),
            (io_err, "io-error"),
            (json_err, "invalid-json"),
        ];
        for (err, expected) in cases {
            assert_eq!(extension_error_code(err), *expected, "{err:?}");
        }
    }
}
