//! Typed errors with stable codes.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("E_INVALID_FORMAT: format '{0}' not supported (only 'gtpack-legacy' in Phase A)")]
    InvalidFormat(String),
    #[error("E_INVALID_CONFIG: {0}")]
    InvalidConfig(String),
    #[error("E_ZIP: {0}")]
    Zip(String),
    #[error("E_SERDE: {0}")]
    Serde(String),
    #[error("E_MANIFEST_CBOR: {0}")]
    ManifestCbor(String),
}

impl PackError {
    pub fn code(&self) -> &'static str {
        match self {
            PackError::InvalidFormat(_) => "E_INVALID_FORMAT",
            PackError::InvalidConfig(_) => "E_INVALID_CONFIG",
            PackError::Zip(_) => "E_ZIP",
            PackError::Serde(_) => "E_SERDE",
            PackError::ManifestCbor(_) => "E_MANIFEST_CBOR",
        }
    }
}

impl From<serde_json::Error> for PackError {
    fn from(e: serde_json::Error) -> Self {
        PackError::Serde(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_stable() {
        assert_eq!(
            PackError::InvalidFormat("x".into()).code(),
            "E_INVALID_FORMAT"
        );
        assert_eq!(
            PackError::InvalidConfig("bad".into()).code(),
            "E_INVALID_CONFIG"
        );
        assert_eq!(PackError::Zip("zip oops".into()).code(), "E_ZIP");
        assert_eq!(PackError::Serde("decode".into()).code(), "E_SERDE");
    }

    #[test]
    fn display_includes_code_prefix() {
        assert_eq!(
            PackError::InvalidFormat("custom".into()).to_string(),
            "E_INVALID_FORMAT: format 'custom' not supported (only 'gtpack-legacy' in Phase A)"
        );
        assert_eq!(
            PackError::InvalidConfig("missing".into()).to_string(),
            "E_INVALID_CONFIG: missing"
        );
        assert_eq!(PackError::Zip("entry".into()).to_string(), "E_ZIP: entry");
        assert_eq!(
            PackError::Serde("decode".into()).to_string(),
            "E_SERDE: decode"
        );
    }

    #[test]
    fn from_serde_json_error() {
        let bad: Result<serde_json::Value, _> = serde_json::from_str("{not json");
        let err: PackError = bad.unwrap_err().into();
        assert_eq!(err.code(), "E_SERDE");
        assert!(matches!(err, PackError::Serde(_)));
    }
}
