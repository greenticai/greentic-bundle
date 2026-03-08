use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: String,
    pub bundle_id: String,
    pub bundle_name: String,
    pub requested_mode: String,
    pub locale: String,
    pub artifact_extension: String,
    #[serde(default)]
    pub generated_resolved_files: Vec<String>,
    #[serde(default)]
    pub generated_setup_files: Vec<String>,
    #[serde(default)]
    pub app_packs: Vec<String>,
    #[serde(default)]
    pub extension_providers: Vec<String>,
    #[serde(default)]
    pub catalogs: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub resolved_targets: Vec<ResolvedTargetSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedTargetSummary {
    pub path: String,
    pub tenant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    pub default_policy: String,
    pub tenant_gmap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_gmap: Option<String>,
    #[serde(default)]
    pub app_pack_policies: Vec<ResolvedReferencePolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedReferencePolicy {
    pub reference: String,
    pub policy: String,
}
