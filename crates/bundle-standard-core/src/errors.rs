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
    }
}
