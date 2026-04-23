use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoReport {
    pub info_schema_version: u32,
    pub bundle_id: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub mode: String,
    pub locale: String,
    pub app_packs: Vec<PackRef>,
    pub extension_providers: Vec<PackRef>,
    pub catalogs: Vec<CatalogRef>,
    pub access: AccessSummary,
    pub capabilities: Vec<String>,
    pub hooks: Vec<String>,
    pub subscriptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRef {
    pub reference: String,
    pub version: Option<String>,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogRef {
    pub name: String,
    pub item_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessSummary {
    pub tenants: u32,
    pub teams: u32,
    pub targets: Vec<AccessTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTarget {
    pub tenant: String,
    pub team_count: u32,
    pub default_policy: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_schema_version_one() {
        let report = InfoReport {
            info_schema_version: 1,
            bundle_id: "b".into(),
            name: "b".into(),
            version: None,
            description: None,
            mode: "production".into(),
            locale: "en".into(),
            app_packs: vec![],
            extension_providers: vec![],
            catalogs: vec![],
            access: AccessSummary { tenants: 0, teams: 0, targets: vec![] },
            capabilities: vec![],
            hooks: vec![],
            subscriptions: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(v["info_schema_version"], 1);
        assert_eq!(v["mode"], "production");
        assert_eq!(v["locale"], "en");
        assert_eq!(v["access"]["tenants"], 0);
    }
}
