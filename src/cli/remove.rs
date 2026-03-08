use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};
use serde::Serialize;

#[derive(Debug, Args)]
pub struct RemoveArgs {
    #[command(subcommand)]
    pub command: RemoveCommand,
}

#[derive(Debug, Subcommand)]
pub enum RemoveCommand {
    #[command(about = "cli.remove.app_pack.about")]
    AppPack(MutationArgs),
    #[command(about = "cli.remove.extension_provider.about")]
    ExtensionProvider(MutationArgs),
}

#[derive(Debug, Args)]
pub struct MutationArgs {
    #[arg(value_name = "ID")]
    pub id: String,

    #[arg(
        long,
        value_name = "PATH",
        default_value = ".",
        help = "cli.remove.root.option"
    )]
    pub root: PathBuf,

    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,

    #[arg(long, default_value_t = false, help = "cli.option.execute")]
    pub execute: bool,
}

#[derive(Debug, Serialize)]
struct MutationPreview {
    root: String,
    field: String,
    action: String,
    item: String,
    execute: bool,
    changed: bool,
    current_values: Vec<String>,
    next_values: Vec<String>,
    expected_file_writes: Vec<String>,
}

pub fn run(args: RemoveArgs) -> Result<()> {
    match args.command {
        RemoveCommand::AppPack(args) => mutate(args, crate::project::ReferenceField::AppPack),
        RemoveCommand::ExtensionProvider(args) => {
            mutate(args, crate::project::ReferenceField::ExtensionProvider)
        }
    }
}

fn mutate(args: MutationArgs, field: crate::project::ReferenceField) -> Result<()> {
    let mut workspace = crate::project::read_bundle_workspace(&args.root)?;
    let current_values = workspace.references(field).to_vec();
    workspace
        .references_mut(field)
        .retain(|value| value != &args.id);
    workspace.canonicalize();
    let next_values = workspace.references(field).to_vec();
    let changed = current_values != next_values;
    let execute = args.execute && !args.dry_run;
    if execute {
        crate::project::write_bundle_workspace(&args.root, &workspace)?;
        crate::project::sync_lock_with_workspace(&args.root, &workspace)?;
        crate::project::sync_project(&args.root)?;
    }
    let preview = MutationPreview {
        root: args.root.display().to_string(),
        field: field_name(field).to_string(),
        action: "remove".to_string(),
        item: args.id,
        execute,
        changed,
        current_values,
        next_values,
        expected_file_writes: if execute {
            vec![
                args.root
                    .join(crate::project::WORKSPACE_ROOT_FILE)
                    .display()
                    .to_string(),
                args.root
                    .join(crate::project::LOCK_FILE)
                    .display()
                    .to_string(),
                args.root
                    .join("resolved/default.yaml")
                    .display()
                    .to_string(),
                args.root
                    .join("state/resolved/default.yaml")
                    .display()
                    .to_string(),
            ]
        } else {
            vec![
                args.root
                    .join(crate::project::WORKSPACE_ROOT_FILE)
                    .display()
                    .to_string(),
            ]
        },
    };
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

fn field_name(field: crate::project::ReferenceField) -> &'static str {
    match field {
        crate::project::ReferenceField::AppPack => "app_packs",
        crate::project::ReferenceField::ExtensionProvider => "extension_providers",
    }
}
