use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,

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
            target: None,
            root: PathBuf::from("."),
            artifact: None,
            json: false,
        }
    }
}

pub fn run(args: InspectArgs) -> Result<()> {
    let report = if let Some(artifact) = args.artifact.as_deref().or_else(|| {
        args.target
            .as_deref()
            .filter(|path| is_bundle_artifact(path))
    }) {
        crate::build::inspect_target(None, Some(artifact))?
    } else if let Some(root) = args.target.as_deref() {
        crate::build::inspect_target(Some(root), None)?
    } else {
        crate::build::inspect_target(Some(&args.root), None)?
    };
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.kind == "artifact" {
        for entry in report.contents.as_deref().unwrap_or(&[]) {
            println!("{entry}");
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(())
}

fn is_bundle_artifact(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("gtbundle"))
}
