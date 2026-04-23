//! Public build_pack orchestrator: synthesize → ZIP → hash → name.

use crate::errors::PackError;
use crate::types::{PackInputs, PackOutput};
use crate::workspace::synthesize_workspace;
use crate::zip_writer::zip_entries;
use sha2::{Digest, Sha256};

pub fn build_pack(inputs: &PackInputs<'_>) -> Result<PackOutput, PackError> {
    let entries = synthesize_workspace(inputs)?;
    let bytes = zip_entries(&entries)?;
    let sha256 = hex_sha256(&bytes);
    let filename = format!(
        "{}-{}.gtpack",
        inputs.config.metadata.name, inputs.config.metadata.version
    );
    Ok(PackOutput { filename, bytes, sha256 })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for b in out {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CardContentEntry, FlowEntry, I18nConfig, StandardConfig, StandardMetadata};
    use serde_json::json;

    fn min_inputs<'a>(
        cfg: &'a StandardConfig,
        flows: &'a [FlowEntry],
        cards: &'a [CardContentEntry],
    ) -> PackInputs<'a> {
        PackInputs {
            config: cfg,
            flows,
            cards,
            assets: &[],
            capabilities: &[],
        }
    }

    #[test]
    fn happy_path() {
        let cfg = StandardConfig {
            metadata: StandardMetadata {
                name: "demo".into(),
                version: "0.1.0".into(),
                author: None,
            },
            channels: vec!["webchat".into()],
            embed_ui: "webchat".into(),
            i18n: I18nConfig::default(),
            format: "gtpack-legacy".into(),
        };
        let flows = vec![FlowEntry {
            name: "main".into(),
            yaml: "schema_version: 2".into(),
        }];
        let cards = vec![CardContentEntry {
            id: "welcome".into(),
            json: json!({"type":"AdaptiveCard"}),
        }];
        let out = build_pack(&min_inputs(&cfg, &flows, &cards)).unwrap();
        assert_eq!(out.filename, "demo-0.1.0.gtpack");
        assert_eq!(out.sha256.len(), 64);
        assert!(!out.bytes.is_empty());
    }

    #[test]
    fn deterministic_sha() {
        let cfg = StandardConfig {
            metadata: StandardMetadata {
                name: "x".into(),
                version: "1".into(),
                author: None,
            },
            channels: vec![],
            embed_ui: "none".into(),
            i18n: I18nConfig::default(),
            format: "gtpack-legacy".into(),
        };
        let a = build_pack(&min_inputs(&cfg, &[], &[])).unwrap();
        let b = build_pack(&min_inputs(&cfg, &[], &[])).unwrap();
        assert_eq!(a.sha256, b.sha256);
    }
}
