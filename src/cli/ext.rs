//! `greentic-bundle ext …` subcommand (feature-gated).

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ExtArgs {
    /// Override the install directory (defaults to `state/ext/`).
    #[arg(long = "extension-dir", value_name = "DIR", global = true)]
    pub extension_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: ExtCommand,
}

#[derive(Debug, Subcommand)]
pub enum ExtCommand {
    /// List all discovered extensions and their recipes.
    #[command(about = "cli.ext.list.about")]
    List,

    /// Print metadata for one extension.
    #[command(about = "cli.ext.info.about")]
    Info {
        /// Extension id (e.g. `greentic.bundle-standard`).
        extension_id: String,
    },

    /// Validate a config JSON against a recipe's schema.
    #[command(about = "cli.ext.validate.about")]
    Validate {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
    },

    /// Render a bundle artifact via the ext dispatcher (Mode A only in Phase A).
    #[command(about = "cli.ext.render.about")]
    Render {
        /// Extension id.
        extension_id: String,
        /// Recipe id.
        recipe_id: String,
        /// Path to a config JSON file.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,
        /// Path to a designer session JSON file.
        #[arg(long, value_name = "FILE")]
        session: PathBuf,
        /// Output file (default: stdout).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },

    /// Print the resolved install directory.
    #[command(about = "cli.ext.install_dir.about")]
    InstallDir,
}
