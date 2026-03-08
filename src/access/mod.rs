pub mod edit;
pub mod eval;
pub mod gmap;
pub mod parse;

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

pub use edit::upsert_policy;
pub use eval::{MatchDecision, eval_policy, eval_with_overlay};
pub use gmap::{GmapMutation, GmapTarget};
pub use parse::{GmapPath, GmapRule, Policy, parse_file, parse_path, parse_rule_line, parse_str};

#[derive(Debug, Serialize)]
pub struct AccessMutationPreview {
    pub gmap_path: String,
    pub rule_path: String,
    pub policy: String,
    pub writes: Vec<String>,
}

pub fn mutate_access(
    root: &Path,
    target: &GmapTarget,
    mutation: &GmapMutation,
    dry_run: bool,
) -> Result<AccessMutationPreview> {
    let gmap_path = crate::project::gmap_path(root, target);
    let resolved_writes =
        crate::project::resolved_output_paths(root, &target.tenant, target.team.as_deref());

    let preview = AccessMutationPreview {
        gmap_path: relative_display(root, &gmap_path),
        rule_path: mutation.rule_path.clone(),
        policy: mutation.policy.to_string(),
        writes: std::iter::once(relative_display(root, &gmap_path))
            .chain(
                resolved_writes
                    .iter()
                    .map(|path| relative_display(root, path)),
            )
            .collect(),
    };

    if dry_run {
        return Ok(preview);
    }

    crate::project::ensure_layout(root)?;
    if let Some(team) = &target.team {
        crate::project::ensure_team(root, &target.tenant, team)?;
    } else {
        crate::project::ensure_tenant(root, &target.tenant)?;
    }
    upsert_policy(&gmap_path, &mutation.rule_path, mutation.policy.clone())
        .with_context(|| format!("update gmap {}", gmap_path.display()))?;
    crate::project::sync_project(root)?;
    Ok(preview)
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub const ACCESS_LAYOUT_HINT: &str = "tenants/<tenant>/tenant.gmap";
