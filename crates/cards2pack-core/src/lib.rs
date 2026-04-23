//! Pure-Rust cards-to-flow YGTc converter.
//!
//! Designed to cross-compile cleanly to `wasm32-wasip2`. NO `tokio`, NO subprocess,
//! NO native filesystem mutation. Inputs are deserialized JSON; outputs are strings
//! and structured warnings.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

mod convert;
mod emit;
mod entry;
mod errors;
mod http_inject;
mod parse;
mod routing;
mod types;

pub use convert::convert;
pub use errors::ConvertError;
pub use http_inject::HttpNode;
pub use parse::parse_cards;
pub use routing::{RouteEdge, RoutingGraph, build_routing};
pub use types::{
    CardEntry, CardKind, ConvertOptions, ConvertResult, Diagnostic, DiagnosticKind, HttpConfig,
};
