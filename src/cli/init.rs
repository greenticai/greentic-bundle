use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

#[derive(Debug, Args, Default)]
pub struct InitArgs {
    #[arg(value_name = "PATH", help = "cli.init.path.option")]
    pub path: Option<PathBuf>,

    #[arg(long, value_name = "NAME", help = "cli.init.bundle_name.option")]
    pub bundle_name: Option<String>,

    #[arg(long, value_name = "ID", help = "cli.init.bundle_id.option")]
    pub bundle_id: Option<String>,

    #[arg(
        long,
        value_name = "MODE",
        default_value = "create",
        help = "cli.init.mode.option"
    )]
    pub mode: String,

    #[arg(
        long,
        value_name = "LOCALE",
        default_value = "en",
        help = "cli.init.locale.option"
    )]
    pub locale: String,

    #[arg(long, default_value_t = false, help = "cli.option.execute")]
    pub execute: bool,
}

#[derive(Debug, Serialize)]
struct InitPreview {
    root: String,
    execute: bool,
    bundle_id: String,
    bundle_name: String,
    mode: String,
    locale: String,
    expected_file_writes: Vec<String>,
}

pub fn run(args: InitArgs) -> Result<()> {
    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    let bundle_name = args
        .bundle_name
        .unwrap_or_else(|| default_bundle_name(&root));
    let bundle_id = args
        .bundle_id
        .unwrap_or_else(|| normalize_bundle_id(&bundle_name));
    let workspace = crate::project::BundleWorkspaceDefinition::new(
        bundle_name.clone(),
        bundle_id.clone(),
        args.locale.clone(),
        args.mode.clone(),
    );
    let preview = InitPreview {
        root: root.display().to_string(),
        execute: args.execute,
        bundle_id,
        bundle_name,
        mode: args.mode,
        locale: args.locale,
        expected_file_writes: vec![
            root.join(crate::project::WORKSPACE_ROOT_FILE)
                .display()
                .to_string(),
            root.join(crate::project::LOCK_FILE).display().to_string(),
            root.join("tenants/default/tenant.gmap")
                .display()
                .to_string(),
            root.join("resolved/default.yaml").display().to_string(),
            root.join("state/resolved/default.yaml")
                .display()
                .to_string(),
        ],
    };
    if args.execute {
        crate::project::init_bundle_workspace(&root, &workspace)?;
    }
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

fn default_bundle_name(root: &std::path::Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != ".")
        .map(|name| name.replace('-', " "))
        .unwrap_or_else(|| "bundle".to_string())
}

fn normalize_bundle_id(raw: &str) -> String {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let trimmed = normalized.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "bundle".to_string()
    } else {
        trimmed
    }
}
