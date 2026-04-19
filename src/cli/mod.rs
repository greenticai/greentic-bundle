use std::ffi::OsString;

use anyhow::Result;
use clap::{Arg, ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

pub mod access;
pub mod add;
pub mod build;
pub mod doctor;
pub mod export;
#[cfg(feature = "extensions")]
pub mod ext;
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
    #[cfg(feature = "extensions")]
    #[command(about = "cli.ext.about")]
    Ext(ext::ExtArgs),
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
            Commands::Unbundle(args) => unbundle::run(args),
            Commands::Add(args) => add::run(args),
            Commands::Remove(args) => remove::run(args),
            Commands::Access(args) => access::run(args),
            Commands::Init(args) => init::run(args),
            #[cfg(feature = "extensions")]
            Commands::Ext(args) => run_ext(args),
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

#[cfg(feature = "extensions")]
#[allow(clippy::too_many_lines)]
fn run_ext(args: ext::ExtArgs) -> anyhow::Result<()> {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::ext::dispatcher::invoke_recipe;
    use crate::ext::loader::load_from_dir;
    use crate::ext::registry::ExtensionRegistry;

    let install_dir = args
        .extension_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("state").join("ext"));

    let mut registry = ExtensionRegistry::new();
    let discovered = load_from_dir(&install_dir)?;
    registry.register_discovered(discovered)?;

    match args.command {
        ext::ExtCommand::List => {
            for e in registry.list() {
                println!(
                    "{ext} {ver} recipe={recipe} kind={kind:?}",
                    ext = e.extension_id,
                    ver = e.extension_version,
                    recipe = e.recipe.id,
                    kind = e.execution,
                );
            }
        }
        ext::ExtCommand::Info { extension_id } => {
            let mut any = false;
            for e in registry.list().filter(|e| e.extension_id == extension_id) {
                any = true;
                println!(
                    "{ext} {ver}\n  recipe: {rid} — {display}\n  schema: {schema}\n  capabilities: {caps}",
                    ext = e.extension_id,
                    ver = e.extension_version,
                    rid = e.recipe.id,
                    display = e.recipe.display_name,
                    schema = e.recipe.config_schema,
                    caps = e.recipe.supported_capabilities.join(", "),
                );
            }
            if !any {
                return Err(anyhow::anyhow!(crate::i18n::trf(
                    "cli.ext.info.not_found",
                    &[("id", extension_id.as_str())]
                )));
            }
        }
        ext::ExtCommand::Validate {
            extension_id,
            recipe_id,
            config,
        } => {
            let entry = registry.resolve(&extension_id, &recipe_id)?;
            let schema_path = entry.descriptor_root.join(&entry.recipe.config_schema);
            let schema_raw = fs::read_to_string(&schema_path)?;
            let schema_json: serde_json::Value = serde_json::from_str(&schema_raw)?;
            let config_raw = fs::read_to_string(&config)?;
            let config_json: serde_json::Value = serde_json::from_str(&config_raw)?;
            let compiled = jsonschema::JSONSchema::compile(&schema_json)
                .map_err(|e| anyhow::anyhow!("schema load error: {e}"))?;
            match compiled.validate(&config_json) {
                Ok(()) => {
                    println!("{}", crate::i18n::tr("cli.ext.validate.ok"));
                }
                Err(errs) => {
                    for e in errs {
                        eprintln!("{}: {e}", e.instance_path);
                    }
                    return Err(anyhow::anyhow!(crate::i18n::tr("cli.ext.validate.failed")));
                }
            }
        }
        ext::ExtCommand::Render {
            extension_id,
            recipe_id,
            config,
            session,
            out,
            json,
        } => {
            if config == "-" && session == "-" {
                return Err(render_error(
                    json,
                    "invalid-args",
                    "only one of --config and --session may read from stdin",
                ));
            }

            let config_json = read_input(&config)
                .map_err(|e| render_error(json, "invalid-config", &e.to_string()))?;
            let session_json = read_input(&session)
                .map_err(|e| render_error(json, "invalid-session", &e.to_string()))?;

            let result = invoke_recipe(
                &registry,
                &extension_id,
                &recipe_id,
                &config_json,
                &session_json,
            );

            match (result, out) {
                (Ok(art), Some(path)) => {
                    fs::write(&path, &art.bytes)?;
                    if json {
                        let summary = serde_json::json!({
                            "status": "ok",
                            "filename": art.filename,
                            "sha256": art.sha256,
                            "bytesLen": art.bytes.len(),
                        });
                        println!("{summary}");
                    } else {
                        let path_str = path.display().to_string();
                        println!(
                            "{}",
                            crate::i18n::trf(
                                "cli.ext.render.wrote",
                                &[("file", path_str.as_str()), ("sha256", art.sha256.as_str()),],
                            )
                        );
                    }
                }
                (Ok(art), None) => {
                    // Binary passthrough; --json with no --out is invalid.
                    if json {
                        return Err(render_error(json, "invalid-args", "--json requires --out"));
                    }
                    std::io::stdout().write_all(&art.bytes)?;
                }
                (Err(e), _) => {
                    return Err(render_error(json, extension_error_code(&e), &e.to_string()));
                }
            }
        }
        ext::ExtCommand::InstallDir => {
            println!("{}", install_dir.display());
        }
    }
    Ok(())
}

#[cfg(feature = "extensions")]
fn read_input(path_or_dash: &str) -> std::io::Result<String> {
    use std::io::Read;
    if path_or_dash == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path_or_dash)
    }
}

#[cfg(feature = "extensions")]
fn render_error(json: bool, code: &str, message: &str) -> anyhow::Error {
    if json {
        let line = serde_json::json!({
            "status": "error",
            "code": code,
            "message": message,
        });
        println!("{line}");
    }
    anyhow::anyhow!("{code}: {message}")
}

#[cfg(feature = "extensions")]
fn extension_error_code(err: &crate::ext::errors::ExtensionError) -> &'static str {
    use crate::ext::errors::ExtensionError;
    match err {
        ExtensionError::NotFound(_) => "extension-not-found",
        ExtensionError::RecipeNotFound { .. } => "recipe-not-found",
        ExtensionError::InvalidConfig(_) => "invalid-config",
        ExtensionError::InvalidDescriptor(_) => "invalid-descriptor",
        ExtensionError::Conflict(_) => "conflict",
        ExtensionError::ModeBNotImplemented => "mode-b-not-implemented",
        ExtensionError::Io(_) => "io-error",
        ExtensionError::Json(_) => "invalid-json",
    }
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
