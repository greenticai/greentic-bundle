use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct UnbundleArgs {
    #[arg(value_name = "FILE")]
    pub artifact: PathBuf,

    #[arg(long, value_name = "DIR", help = "cli.unbundle.out.option")]
    pub out: Option<PathBuf>,
}

pub fn run(args: UnbundleArgs) -> Result<()> {
    let output_dir = args.out.unwrap_or_else(|| PathBuf::from("."));
    let result = crate::build::unbundle_artifact(&args.artifact, &output_dir)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
