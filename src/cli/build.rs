use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct BuildArgs {
    #[arg(long, default_value = ".", help = "cli.build.root.option")]
    pub root: PathBuf,

    #[arg(long, value_name = "FILE", help = "cli.build.output.option")]
    pub output: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,
}

impl Default for BuildArgs {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            output: None,
            dry_run: false,
        }
    }
}

pub fn run(args: BuildArgs) -> Result<()> {
    let result = crate::build::build_workspace(&args.root, args.output.as_deref(), args.dry_run)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
