//! Typed errors with stable string codes for cross-boundary identification.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("E_NO_CARDS: no cards provided")]
    NoCards,
    #[error("E_NO_ENTRY: cannot detect entry card from {count} cards")]
    NoEntryCard { count: usize },
    #[error("E_DANGLING_ROUTE: card '{from}' routes to unknown card '{to}'")]
    DanglingRoute { from: String, to: String },
    #[error("E_INVALID_CARD: card '{id}' is not a valid AdaptiveCard JSON: {msg}")]
    InvalidCard { id: String, msg: String },
    #[error("E_INVALID_HTTP: card '{id}' has invalid HTTP config: {msg}")]
    InvalidHttp { id: String, msg: String },
    #[error("E_PARSE: cannot parse cards JSON: {0}")]
    Parse(String),
    #[error("E_EMIT: cannot serialize flow YAML: {0}")]
    Emit(String),
}

impl ConvertError {
    pub fn code(&self) -> &'static str {
        match self {
            ConvertError::NoCards => "E_NO_CARDS",
            ConvertError::NoEntryCard { .. } => "E_NO_ENTRY",
            ConvertError::DanglingRoute { .. } => "E_DANGLING_ROUTE",
            ConvertError::InvalidCard { .. } => "E_INVALID_CARD",
            ConvertError::InvalidHttp { .. } => "E_INVALID_HTTP",
            ConvertError::Parse(_) => "E_PARSE",
            ConvertError::Emit(_) => "E_EMIT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_round_trip() {
        assert_eq!(ConvertError::NoCards.code(), "E_NO_CARDS");
        assert_eq!(ConvertError::NoEntryCard { count: 0 }.code(), "E_NO_ENTRY");
        assert_eq!(
            ConvertError::DanglingRoute {
                from: "a".into(),
                to: "b".into()
            }
            .code(),
            "E_DANGLING_ROUTE"
        );
    }
}
