//! Error type for the `ext` module.

use std::io;

#[derive(thiserror::Error, Debug)]
pub enum ExtensionError {
    #[error("extension not found: {0}")]
    NotFound(String),

    #[error("recipe not found: {ext}/{recipe}")]
    RecipeNotFound { ext: String, recipe: String },

    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("invalid descriptor: {0}")]
    InvalidDescriptor(String),

    #[error("conflict: recipe id `{0}` offered by multiple extensions")]
    Conflict(String),

    #[error("Mode B (full WASM) not implemented in Phase A")]
    ModeBNotImplemented,

    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_not_found() {
        let e = ExtensionError::NotFound("greentic.bundle-standard".into());
        assert_eq!(e.to_string(), "extension not found: greentic.bundle-standard");
    }

    #[test]
    fn display_recipe_not_found() {
        let e = ExtensionError::RecipeNotFound {
            ext: "greentic.bundle-standard".into(),
            recipe: "unknown".into(),
        };
        assert_eq!(
            e.to_string(),
            "recipe not found: greentic.bundle-standard/unknown",
        );
    }

    #[test]
    fn mode_b_variant_distinct() {
        let e = ExtensionError::ModeBNotImplemented;
        assert_eq!(e.to_string(), "Mode B (full WASM) not implemented in Phase A");
    }

    #[test]
    fn from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let e: ExtensionError = io_err.into();
        assert!(matches!(e, ExtensionError::Io(_)));
    }

    #[test]
    fn from_json_error() {
        let bad: Result<serde_json::Value, _> = serde_json::from_str("{bad json");
        let e: ExtensionError = bad.unwrap_err().into();
        assert!(matches!(e, ExtensionError::Json(_)));
    }
}
