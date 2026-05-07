use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[arg(long, value_name = "DIR", help = "cli.export.build_dir.option")]
    pub build_dir: PathBuf,

    #[arg(long, value_name = "FILE", help = "cli.export.output.option")]
    pub output: PathBuf,

    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,

    /// Embed a precompiled component cache (`.cache/v1/...`) into the bundle.
    /// Requires `greentic-start` on PATH.
    #[arg(long, default_value_t = false)]
    pub warmup: bool,
}

pub fn run(args: ExportArgs) -> Result<()> {
    let result =
        crate::build::export_build_dir(&args.build_dir, &args.output, args.dry_run, args.warmup)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
