use anyhow::Result;
use serde::Serialize;

use crate::access::{Policy, parse_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmapTarget {
    pub tenant: String,
    pub team: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GmapMutation {
    pub rule_path: String,
    pub policy: Policy,
}

impl GmapMutation {
    pub fn new(rule_path: &str, policy: Policy) -> Result<Self> {
        let parsed = parse_path(rule_path, 0)?;
        Ok(Self {
            rule_path: parsed.to_string(),
            policy,
        })
    }
}
