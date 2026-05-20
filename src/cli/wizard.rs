use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct WizardArgs {
    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "cli.option.schema",
        long_help = "cli.option.schema.long"
    )]
    pub schema: bool,
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
    #[arg(long = "env", short = 'e', default_value = "local")]
    pub env: String,
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
    if args.schema {
        let schema =
            crate::wizard::answer_document_schema(args.schema_mode(), args.schema_version())?;
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    match args.command {
        None => {
            let zero_action = embedded_root_zero_action();
            loop {
                let result = match crate::wizard::run_interactive_with_zero_action(
                    None,
                    None,
                    None,
                    crate::wizard::ExecutionMode::Execute,
                    zero_action,
                    "local",
                ) {
                    Ok(result) => result,
                    Err(error) if error.to_string() == crate::i18n::tr("wizard.exit.message") => {
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                };
                let Some(result) = result else {
                    return Ok(());
                };
                crate::wizard::print_plan(&result.plan)?;
            }
        }
        Some(WizardCommand::Run(args)) => crate::wizard::run_command(args),
        Some(WizardCommand::Validate(args)) => crate::wizard::validate_command(args),
        Some(WizardCommand::Apply(args)) => crate::wizard::apply_command(args),
    }
}

impl WizardArgs {
    fn schema_mode(&self) -> Option<WizardMode> {
        match self.command.as_ref() {
            Some(WizardCommand::Run(args)) => args.mode,
            Some(WizardCommand::Validate(args)) => args.mode,
            Some(WizardCommand::Apply(args)) => args.mode,
            None => None,
        }
    }

    fn schema_version(&self) -> Option<&str> {
        match self.command.as_ref() {
            Some(WizardCommand::Run(args)) => args.schema_version.as_deref(),
            Some(WizardCommand::Validate(args)) => args.schema_version.as_deref(),
            Some(WizardCommand::Apply(args)) => args.schema_version.as_deref(),
            None => None,
        }
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
