use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long, default_value = ".", help = "cli.doctor.root.option")]
    pub root: PathBuf,

    #[arg(long, value_name = "FILE", help = "cli.doctor.artifact.option")]
    pub artifact: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "cli.doctor.json.option")]
    pub json: bool,
}

impl Default for DoctorArgs {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            artifact: None,
            json: false,
        }
    }
}

pub fn run(args: DoctorArgs) -> Result<()> {
    let report = if let Some(artifact) = args.artifact.as_deref() {
        crate::build::doctor_target(None, Some(artifact))?
    } else {
        crate::build::doctor_target(Some(&args.root), None)?
    };
    let _ = args.json;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.ok {
        // Phase 0 P0.2: non-zero exit so CI can rely on `doctor` as a gate.
        // Existing JSON output above is the single source of truth for what
        // failed; this message is just an exit-code anchor.
        bail!(
            "doctor reported failing checks for {}; see report above",
            report.target
        );
    }
    Ok(())
}
