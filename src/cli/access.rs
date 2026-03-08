use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct AccessArgs {
    #[command(subcommand)]
    pub command: AccessCommand,
}

#[derive(Debug, Subcommand)]
pub enum AccessCommand {
    #[command(about = "cli.access.allow.about")]
    Allow(AccessMutationArgs),
    #[command(about = "cli.access.forbid.about")]
    Forbid(AccessMutationArgs),
}

#[derive(Debug, Args)]
pub struct AccessMutationArgs {
    #[arg(value_name = "SUBJECT")]
    pub subject: String,

    #[arg(
        long,
        value_name = "PATH",
        default_value = ".",
        help = "cli.access.root.option"
    )]
    pub root: std::path::PathBuf,

    #[arg(long, default_value = "default", help = "cli.access.tenant.option")]
    pub tenant: String,

    #[arg(long, help = "cli.access.team.option")]
    pub team: Option<String>,

    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,

    #[arg(long, default_value_t = false, help = "cli.option.execute")]
    pub execute: bool,
}

pub fn run(args: AccessArgs) -> Result<()> {
    match args.command {
        AccessCommand::Allow(args) => handle(args, crate::access::Policy::Public),
        AccessCommand::Forbid(args) => handle(args, crate::access::Policy::Forbidden),
    }
}

fn handle(args: AccessMutationArgs, policy: crate::access::Policy) -> Result<()> {
    let target = crate::access::GmapTarget {
        tenant: args.tenant,
        team: args.team,
    };
    let mutation = crate::access::GmapMutation::new(&args.subject, policy)?;
    let preview = crate::access::mutate_access(
        &args.root,
        &target,
        &mutation,
        args.dry_run || !args.execute,
    )?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}
