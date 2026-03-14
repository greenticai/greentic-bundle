use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct WizardArgs {
    #[command(subcommand)]
    pub command: Option<WizardCommand>,
}

#[derive(Debug, Subcommand)]
pub enum WizardCommand {
    #[command(about = "cli.wizard.run.about")]
    Run(WizardRunArgs),
    #[command(about = "cli.wizard.validate.about")]
    Validate(WizardValidateArgs),
    #[command(about = "cli.wizard.apply.about")]
    Apply(WizardApplyArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum WizardMode {
    Create,
    Update,
    Doctor,
}

#[derive(Debug, Args, Default)]
pub struct WizardRunArgs {
    #[arg(long, value_name = "FILE", help = "cli.option.answers")]
    pub answers: Option<PathBuf>,
    #[arg(
        long = "emit-answers",
        value_name = "FILE",
        help = "cli.option.emit_answers"
    )]
    pub emit_answers: Option<PathBuf>,
    #[arg(
        long = "schema-version",
        value_name = "VER",
        help = "cli.option.schema_version"
    )]
    pub schema_version: Option<String>,
    #[arg(long, default_value_t = false, help = "cli.option.migrate")]
    pub migrate: bool,
    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,
    #[arg(long, value_enum, help = "cli.wizard.mode.option")]
    pub mode: Option<WizardMode>,
}

#[derive(Debug, Args)]
pub struct WizardValidateArgs {
    #[arg(long, value_name = "FILE", help = "cli.option.answers")]
    pub answers: PathBuf,
    #[arg(
        long = "emit-answers",
        value_name = "FILE",
        help = "cli.option.emit_answers"
    )]
    pub emit_answers: Option<PathBuf>,
    #[arg(
        long = "schema-version",
        value_name = "VER",
        help = "cli.option.schema_version"
    )]
    pub schema_version: Option<String>,
    #[arg(long, default_value_t = false, help = "cli.option.migrate")]
    pub migrate: bool,
    #[arg(long, value_enum, help = "cli.wizard.mode.option")]
    pub mode: Option<WizardMode>,
}

#[derive(Debug, Args)]
pub struct WizardApplyArgs {
    #[arg(long, value_name = "FILE", help = "cli.option.answers")]
    pub answers: PathBuf,
    #[arg(
        long = "emit-answers",
        value_name = "FILE",
        help = "cli.option.emit_answers"
    )]
    pub emit_answers: Option<PathBuf>,
    #[arg(
        long = "schema-version",
        value_name = "VER",
        help = "cli.option.schema_version"
    )]
    pub schema_version: Option<String>,
    #[arg(long, default_value_t = false, help = "cli.option.migrate")]
    pub migrate: bool,
    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,
    #[arg(long, value_enum, help = "cli.wizard.mode.option")]
    pub mode: Option<WizardMode>,
}

pub fn run(args: WizardArgs) -> Result<()> {
    match args.command {
        None => {
            let zero_action = embedded_root_zero_action();
            let Some(result) = crate::wizard::run_interactive_with_zero_action(
                None,
                None,
                None,
                crate::wizard::ExecutionMode::Execute,
                zero_action,
            )?
            else {
                return Ok(());
            };
            crate::wizard::print_plan(&result.plan)
        }
        Some(WizardCommand::Run(args)) => crate::wizard::run_command(args),
        Some(WizardCommand::Validate(args)) => crate::wizard::validate_command(args),
        Some(WizardCommand::Apply(args)) => crate::wizard::apply_command(args),
    }
}

fn embedded_root_zero_action() -> crate::wizard::RootMenuZeroAction {
    match std::env::var("GREENTIC_WIZARD_ROOT_ZERO_ACTION")
        .ok()
        .as_deref()
    {
        Some("back") => crate::wizard::RootMenuZeroAction::Back,
        _ => crate::wizard::RootMenuZeroAction::Exit,
    }
}
