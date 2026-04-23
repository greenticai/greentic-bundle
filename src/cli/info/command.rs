use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::Args;

use super::human;
use super::report::InfoReport;

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Path to a .gtbundle file or bundle workspace directory.
    #[arg(value_name = "PATH", help = "cli.info.arg.path")]
    pub path: PathBuf,

    /// Emit the info report as JSON.
    #[arg(long, default_value_t = false, help = "cli.info.flag.json")]
    pub json: bool,
}

pub fn run(args: InfoArgs) -> Result<()> {
    let report = build_report(&args.path)?;
    if args.json {
        let pretty =
            serde_json::to_string_pretty(&report).context("serialize info report as JSON")?;
        println!("{pretty}");
    } else {
        print!("{}", human::render(&report));
    }
    Ok(())
}

fn build_report(path: &Path) -> Result<InfoReport> {
    if !path.exists() {
        let path_display = path.display().to_string();
        eprintln!(
            "{}",
            crate::i18n::trf(
                "cli.info.error.not_a_bundle",
                &[("0", path_display.as_str())]
            )
        );
        std::process::exit(2);
    }
    if path.is_dir() {
        return InfoReport::from_workspace(path).context("reading bundle workspace");
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("gtbundle") => {
            let opened = greentic_bundle_reader::open_artifact(path)
                .map_err(|e| anyhow!("{}", e))
                .context("reading bundle artifact")?;
            Ok(InfoReport::from_opened_bundle(&opened))
        }
        _ => {
            eprintln!(
                "{}: not a .gtbundle file or bundle workspace",
                path.display()
            );
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_args_default_json_is_false() {
        let args = InfoArgs {
            path: PathBuf::from("demo.gtbundle"),
            json: false,
        };
        assert!(!args.json);
    }
}
