use std::ffi::OsString;

use anyhow::Result;
use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

pub mod access;
pub mod add;
pub mod build;
pub mod doctor;
pub mod export;
pub mod info;
pub mod init;
pub mod inspect;
pub mod remove;
pub mod unbundle;
pub mod wizard;

#[derive(Debug, Parser)]
#[command(
    name = "greentic-bundle",
    about = "cli.root.about",
    long_about = "cli.root.long_about",
    version,
    arg_required_else_help = true
)]
pub struct Cli {
    #[arg(
        long = "locale",
        value_name = "LOCALE",
        global = true,
        help = "cli.option.locale"
    )]
    locale: Option<String>,

    #[arg(
        long = "offline",
        global = true,
        default_value_t = false,
        help = "cli.option.offline"
    )]
    offline: bool,

    #[arg(
        long = "refresh",
        global = true,
        default_value_t = false,
        help = "cli.option.refresh"
    )]
    refresh: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "cli.wizard.about")]
    Wizard(wizard::WizardArgs),
    #[command(about = "cli.doctor.about")]
    Doctor(doctor::DoctorArgs),
    #[command(about = "cli.build.about", long_about = "cli.build.long_about")]
    Build(build::BuildArgs),
    #[command(about = "cli.export.about", long_about = "cli.export.long_about")]
    Export(export::ExportArgs),
    #[command(about = "cli.inspect.about")]
    Inspect(inspect::InspectArgs),
    #[command(about = "cli.info.about")]
    Info(info::InfoArgs),
    #[command(about = "cli.unbundle.about")]
    Unbundle(unbundle::UnbundleArgs),
    #[command(about = "cli.add.about")]
    Add(add::AddArgs),
    #[command(about = "cli.remove.about")]
    Remove(remove::RemoveArgs),
    #[command(about = "cli.access.about")]
    Access(access::AccessArgs),
    #[command(about = "cli.init.about")]
    Init(init::InitArgs),
}

pub fn run() -> Result<()> {
    let argv: Vec<OsString> = std::env::args_os().collect();
    crate::i18n::init(crate::i18n::cli_locale_from_argv(&argv));

    let mut command = localized_command(true);
    let matches = match command.try_get_matches_from_mut(argv) {
        Ok(matches) => matches,
        Err(err) => err.exit(),
    };
    let cli = Cli::from_arg_matches(&matches)?;
    crate::i18n::init(cli.locale.clone());
    crate::runtime::set_offline(cli.offline);
    crate::runtime::set_refresh(cli.refresh);
    cli.dispatch()
}

pub fn localized_command(is_root: bool) -> clap::Command {
    localize_help(Cli::command(), is_root)
}

impl Cli {
    fn dispatch(self) -> Result<()> {
        match self.command {
            Commands::Wizard(args) => wizard::run(args),
            Commands::Doctor(args) => doctor::run(args),
            Commands::Build(args) => build::run(args),
            Commands::Export(args) => export::run(args),
            Commands::Inspect(args) => inspect::run(args),
            Commands::Info(args) => info::run(args),
            Commands::Unbundle(args) => unbundle::run(args),
            Commands::Add(args) => add::run(args),
            Commands::Remove(args) => remove::run(args),
            Commands::Access(args) => access::run(args),
            Commands::Init(args) => init::run(args),
        }
    }
}

fn localize_help(mut command: clap::Command, is_root: bool) -> clap::Command {
    if let Some(about) = command.get_about().map(|s| s.to_string()) {
        command = command.about(crate::i18n::tr(&about));
    }
    if let Some(long_about) = command.get_long_about().map(|s| s.to_string()) {
        command = command.long_about(crate::i18n::tr(&long_about));
    }
    if let Some(before) = command.get_before_help().map(|s| s.to_string()) {
        command = command.before_help(crate::i18n::tr(&before));
    }
    if let Some(after) = command.get_after_help().map(|s| s.to_string()) {
        command = command.after_help(crate::i18n::tr(&after));
    }

    command = command
        .disable_help_subcommand(true)
        .disable_help_flag(true)
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .action(ArgAction::Help)
                .help(crate::i18n::tr("cli.help.flag")),
        );
    if is_root {
        command = command.disable_version_flag(true).arg(
            Arg::new("version")
                .short('V')
                .long("version")
                .action(ArgAction::Version)
                .help(crate::i18n::tr("cli.version.flag")),
        );
    }

    let arg_ids = command
        .get_arguments()
        .map(|arg| arg.get_id().clone())
        .collect::<Vec<_>>();
    for arg_id in arg_ids {
        command = command.mut_arg(arg_id, |arg| {
            let mut arg = arg;
            if let Some(help) = arg.get_help().map(ToString::to_string) {
                arg = arg.help(crate::i18n::tr(&help));
            }
            if let Some(long_help) = arg.get_long_help().map(ToString::to_string) {
                arg = arg.long_help(crate::i18n::tr(&long_help));
            }
            arg
        });
    }

    let sub_names = command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect::<Vec<_>>();
    for name in sub_names {
        command = command.mut_subcommand(name, |sub| localize_help(sub, false));
    }
    command
}


#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands};

    #[test]
    fn parses_global_locale_and_wizard_flags() {
        let cli = Cli::try_parse_from([
            "greentic-bundle",
            "--locale",
            "en-US",
            "wizard",
            "run",
            "--schema",
            "--answers",
            "answers.json",
            "--emit-answers",
            "out.json",
            "--schema-version",
            "1.2.3",
            "--migrate",
            "--dry-run",
        ])
        .expect("cli parses");

        assert_eq!(cli.locale.as_deref(), Some("en-US"));
        match cli.command {
            Commands::Wizard(args) => {
                assert!(args.schema);
                match args.command.expect("wizard subcommand") {
                    super::wizard::WizardCommand::Run(run) => {
                        assert_eq!(
                            run.answers.as_deref(),
                            Some(std::path::Path::new("answers.json"))
                        );
                        assert_eq!(
                            run.emit_answers.as_deref(),
                            Some(std::path::Path::new("out.json"))
                        );
                        assert_eq!(run.schema_version.as_deref(), Some("1.2.3"));
                        assert!(run.migrate);
                        assert!(run.dry_run);
                    }
                    _ => panic!("expected run"),
                }
            }
            _ => panic!("expected wizard"),
        }
    }

    #[test]
    fn parses_access_allow_execute_flag() {
        let cli = Cli::try_parse_from([
            "greentic-bundle",
            "access",
            "allow",
            "tenant-a",
            "--execute",
        ])
        .expect("cli parses");

        match cli.command {
            Commands::Access(args) => match args.command {
                super::access::AccessCommand::Allow(allow) => {
                    assert_eq!(allow.subject, "tenant-a");
                    assert!(allow.execute);
                    assert!(!allow.dry_run);
                }
                _ => panic!("expected access allow"),
            },
            _ => panic!("expected access"),
        }
    }

    #[test]
    fn parses_build_export_doctor_and_inspect_flags() {
        let build = Cli::try_parse_from([
            "greentic-bundle",
            "build",
            "--root",
            "bundle",
            "--output",
            "out.gtbundle",
            "--dry-run",
        ])
        .expect("build parses");
        match build.command {
            Commands::Build(args) => {
                assert_eq!(args.root, std::path::PathBuf::from("bundle"));
                assert_eq!(args.output, Some(std::path::PathBuf::from("out.gtbundle")));
                assert!(args.dry_run);
            }
            _ => panic!("expected build"),
        }

        let doctor = Cli::try_parse_from([
            "greentic-bundle",
            "doctor",
            "--artifact",
            "demo.gtbundle",
            "--json",
        ])
        .expect("doctor parses");
        match doctor.command {
            Commands::Doctor(args) => {
                assert_eq!(
                    args.artifact,
                    Some(std::path::PathBuf::from("demo.gtbundle"))
                );
                assert!(args.json);
            }
            _ => panic!("expected doctor"),
        }

        let export = Cli::try_parse_from([
            "greentic-bundle",
            "export",
            "--build-dir",
            "state/build/demo/normalized",
            "--output",
            "demo.gtbundle",
            "--dry-run",
        ])
        .expect("export parses");
        match export.command {
            Commands::Export(args) => {
                assert_eq!(
                    args.build_dir,
                    std::path::PathBuf::from("state/build/demo/normalized")
                );
                assert_eq!(args.output, std::path::PathBuf::from("demo.gtbundle"));
                assert!(args.dry_run);
            }
            _ => panic!("expected export"),
        }

        let inspect = Cli::try_parse_from(["greentic-bundle", "inspect", "bundle", "--json"])
            .expect("inspect parses");
        match inspect.command {
            Commands::Inspect(args) => {
                assert_eq!(args.target, Some(std::path::PathBuf::from("bundle")));
                assert!(args.json);
            }
            _ => panic!("expected inspect"),
        }
    }

    #[test]
    fn command_defaults_use_current_directory() {
        assert_eq!(
            super::build::BuildArgs::default().root,
            std::path::PathBuf::from(".")
        );
        assert_eq!(
            super::doctor::DoctorArgs::default().root,
            std::path::PathBuf::from(".")
        );
        assert_eq!(
            super::inspect::InspectArgs::default().root,
            std::path::PathBuf::from(".")
        );
    }
}
