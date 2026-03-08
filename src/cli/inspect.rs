use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(long, default_value = ".", help = "cli.inspect.root.option")]
    pub root: PathBuf,

    #[arg(long, value_name = "FILE", help = "cli.inspect.artifact.option")]
    pub artifact: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "cli.inspect.json.option")]
    pub json: bool,
}

impl Default for InspectArgs {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            artifact: None,
            json: false,
        }
    }
}

pub fn run(args: InspectArgs) -> Result<()> {
    let report = if let Some(artifact) = args.artifact.as_deref() {
        crate::build::inspect_target(None, Some(artifact))?
    } else {
        crate::build::inspect_target(Some(&args.root), None)?
    };
    let _ = args.json;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
