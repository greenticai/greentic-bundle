use std::collections::BTreeMap;
use std::fs;
use std::io::IsTerminal;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use greentic_qa_lib::{I18nConfig, WizardDriver, WizardFrontend, WizardRunConfig};
use semver::Version;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::answers::{AnswerDocument, migrate::migrate_document};
use crate::cli::wizard::{WizardApplyArgs, WizardMode, WizardRunArgs, WizardValidateArgs};

pub mod i18n;

pub const WIZARD_ID: &str = "greentic-bundle.wizard.run";
pub const ANSWER_SCHEMA_ID: &str = "greentic-bundle.wizard.answers";
pub const DEFAULT_PROVIDER_REGISTRY: &str =
    "oci://ghcr.io/greenticai/greentic-bundle/providers:stable";
const DEPLOYER_AWS_REF: &str = "oci://ghcr.io/greenticai/packs/deployer/greentic.deploy.aws:stable";
const DEPLOYER_AZURE_REF: &str =
    "oci://ghcr.io/greenticai/packs/deployer/greentic.deploy.azure:stable";
const DEPLOYER_GCP_REF: &str = "oci://ghcr.io/greenticai/packs/deployer/greentic.deploy.gcp:stable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    DryRun,
    Execute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRequest {
    pub mode: WizardMode,
    pub locale: String,
    pub bundle_name: String,
    pub bundle_id: String,
    pub output_dir: PathBuf,
    pub local_reference_base_dir: Option<PathBuf>,
    pub app_pack_entries: Vec<AppPackEntry>,
    pub access_rules: Vec<AccessRuleInput>,
    pub extension_provider_entries: Vec<ExtensionProviderEntry>,
    pub advanced_setup: bool,
    pub app_packs: Vec<String>,
    pub extension_providers: Vec<String>,
    pub remote_catalogs: Vec<String>,
    pub setup_specs: BTreeMap<String, Value>,
    pub setup_answers: BTreeMap<String, Value>,
    pub setup_execution_intent: bool,
    pub export_intent: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct AppPackEntry {
    pub reference: String,
    pub detected_kind: String,
    pub pack_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub mapping: AppPackMappingInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct AppPackMappingInput {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct AccessRuleInput {
    pub rule_path: String,
    pub policy: String,
    pub tenant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ExtensionProviderEntry {
    pub reference: String,
    pub detected_kind: String,
    pub provider_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_catalog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewAction {
    BuildNow,
    DryRunOnly,
    SaveAnswersOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractiveChoice {
    Create,
    Update,
    Validate,
    Doctor,
    Inspect,
    Unbundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootMenuZeroAction {
    Exit,
    Back,
}

#[derive(Debug)]
struct InteractiveRequest {
    request: NormalizedRequest,
    review_action: ReviewAction,
}

enum InteractiveSelection {
    Request(Box<InteractiveRequest>),
    Handled,
}

enum BundleTarget {
    Workspace(PathBuf),
    Artifact(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WizardPlanEnvelope {
    pub metadata: PlanMetadata,
    pub target_root: String,
    pub requested_action: String,
    pub normalized_input_summary: BTreeMap<String, Value>,
    pub ordered_step_list: Vec<WizardPlanStep>,
    pub expected_file_writes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanMetadata {
    pub wizard_id: String,
    pub schema_id: String,
    pub schema_version: String,
    pub locale: String,
    pub execution: ExecutionMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WizardPlanStep {
    pub kind: StepKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    EnsureWorkspace,
    WriteBundleFile,
    UpdateAccessRules,
    ResolveRefs,
    WriteLock,
    BuildBundle,
    ExportBundle,
}

#[derive(Debug)]
pub struct WizardRunResult {
    pub plan: WizardPlanEnvelope,
    pub document: AnswerDocument,
    pub applied_files: Vec<PathBuf>,
}

struct LoadedRequest {
    request: NormalizedRequest,
    locks: BTreeMap<String, Value>,
    build_bundle_now: bool,
}

pub fn run_command(args: WizardRunArgs) -> Result<()> {
    let locale = crate::i18n::current_locale();
    let result = if let Some(path) = args.answers.as_ref() {
        let loaded = load_and_normalize_answers(
            path,
            args.mode,
            args.schema_version.as_deref(),
            args.migrate,
            &locale,
        )?;
        execute_request(
            loaded.request,
            execution_for_run(args.dry_run),
            loaded.build_bundle_now && !args.dry_run,
            args.schema_version.as_deref(),
            args.emit_answers.as_ref(),
            Some(loaded.locks),
            &args.env,
        )?
    } else {
        run_interactive(
            args.mode,
            args.emit_answers.as_ref(),
            args.schema_version.as_deref(),
            execution_for_run(args.dry_run),
            &args.env,
        )?
    };
    print_plan(&result.plan)?;
    Ok(())
}

pub fn answer_document_schema(
    mode: Option<WizardMode>,
    schema_version: Option<&str>,
) -> Result<Value> {
    let schema_version = requested_schema_version(schema_version)?;
    let selected_mode = mode.map(mode_name);
    let mode_schema = match selected_mode {
        Some(mode) => json!({
            "type": "string",
            "const": mode,
            "description": "Wizard mode. When omitted, the CLI can also supply --mode."
        }),
        None => json!({
            "type": "string",
            "enum": ["create", "update", "doctor"],
            "description": "Wizard mode. Defaults to create when omitted unless the CLI supplies --mode."
        }),
    };

    Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://greenticai.github.io/greentic-bundle/schemas/wizard.answers.schema.json",
        "title": "greentic-bundle wizard answers",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "wizard_id": {
                "type": "string",
                "const": WIZARD_ID
            },
            "schema_id": {
                "type": "string",
                "const": ANSWER_SCHEMA_ID
            },
            "schema_version": {
                "type": "string",
                "const": schema_version.to_string()
            },
            "locale": {
                "type": "string",
                "minLength": 1
            },
            "answers": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "mode": mode_schema,
                    "bundle_name": non_empty_string_schema("Human-friendly bundle name."),
                    "bundle_id": non_empty_string_schema("Stable bundle id."),
                    "output_dir": non_empty_string_schema("Workspace output directory."),
                    "advanced_setup": {
                        "type": "boolean"
                    },
                    "app_pack_entries": {
                        "type": "array",
                        "items": app_pack_entry_schema()
                    },
                    "access_rules": {
                        "type": "array",
                        "items": access_rule_schema()
                    },
                    "extension_provider_entries": {
                        "type": "array",
                        "items": extension_provider_entry_schema()
                    },
                    "app_packs": string_array_schema("App-pack references or local paths."),
                    "extension_providers": string_array_schema("Extension provider references or local paths."),
                    "cloud_deployer_target": {
                        "type": "string",
                        "enum": ["aws", "azure", "gcp"]
                    },
                    "remote_catalogs": string_array_schema("Additional remote catalog references."),
                    "setup_specs": {
                        "type": "object",
                        "additionalProperties": true
                    },
                    "setup_answers": {
                        "type": "object",
                        "additionalProperties": true
                    },
                    "setup_execution_intent": {
                        "type": "boolean"
                    },
                    "export_intent": {
                        "type": "boolean"
                    },
                    "capabilities": string_array_schema("Requested bundle capabilities.")
                },
                "required": ["bundle_name", "bundle_id"]
            },
            "locks": {
                "type": "object",
                "additionalProperties": true,
                "properties": {
                    "execution": {
                        "type": "string",
                        "enum": ["dry_run", "execute"]
                    }
                }
            }
        },
        "required": ["wizard_id", "schema_id", "schema_version", "locale", "answers"]
    }))
}

fn non_empty_string_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": description
    })
}

fn string_array_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "description": description,
        "items": {
            "type": "string",
            "minLength": 1
        }
    })
}

fn app_pack_entry_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "reference": non_empty_string_schema("Resolved reference or source path."),
            "detected_kind": non_empty_string_schema("Detected source kind."),
            "pack_id": non_empty_string_schema("Pack id."),
            "display_name": non_empty_string_schema("Pack display name."),
            "version": {
                "type": ["string", "null"]
            },
            "mapping": app_pack_mapping_schema()
        },
        "required": ["reference", "detected_kind", "pack_id", "display_name", "mapping"]
    })
}

fn app_pack_mapping_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "scope": {
                "type": "string",
                "enum": ["global", "tenant", "tenant_team"]
            },
            "tenant": {
                "type": ["string", "null"]
            },
            "team": {
                "type": ["string", "null"]
            }
        },
        "required": ["scope"]
    })
}

fn access_rule_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "rule_path": non_empty_string_schema("Resolved GMAP rule path."),
            "policy": {
                "type": "string",
                "enum": ["allow", "forbid"]
            },
            "tenant": non_empty_string_schema("Tenant id."),
            "team": {
                "type": ["string", "null"]
            }
        },
        "required": ["rule_path", "policy", "tenant"]
    })
}

fn extension_provider_entry_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "reference": non_empty_string_schema("Resolved reference or source path."),
            "detected_kind": non_empty_string_schema("Detected source kind."),
            "provider_id": non_empty_string_schema("Provider id."),
            "display_name": non_empty_string_schema("Provider display name."),
            "version": {
                "type": ["string", "null"]
            },
            "source_catalog": {
                "type": ["string", "null"]
            },
            "group": {
                "type": ["string", "null"]
            }
        },
        "required": ["reference", "detected_kind", "provider_id", "display_name"]
    })
}

pub fn validate_command(args: WizardValidateArgs) -> Result<()> {
    let locale = crate::i18n::current_locale();
    let loaded = load_and_normalize_answers(
        &args.answers,
        args.mode,
        args.schema_version.as_deref(),
        args.migrate,
        &locale,
    )?;
    let result = execute_request(
        loaded.request,
        ExecutionMode::DryRun,
        false,
        args.schema_version.as_deref(),
        args.emit_answers.as_ref(),
        Some(loaded.locks),
        &args.env,
    )?;
    print_plan(&result.plan)?;
    Ok(())
}

pub fn apply_command(args: WizardApplyArgs) -> Result<()> {
    let locale = crate::i18n::current_locale();
    let loaded = load_and_normalize_answers(
        &args.answers,
        args.mode,
        args.schema_version.as_deref(),
        args.migrate,
        &locale,
    )?;
    let execution = if args.dry_run {
        ExecutionMode::DryRun
    } else {
        ExecutionMode::Execute
    };
    let result = execute_request(
        loaded.request,
        execution,
        loaded.build_bundle_now && execution == ExecutionMode::Execute,
        args.schema_version.as_deref(),
        args.emit_answers.as_ref(),
        Some(loaded.locks),
        &args.env,
    )?;
    print_plan(&result.plan)?;
    Ok(())
}

pub fn run_interactive(
    initial_mode: Option<WizardMode>,
    emit_answers: Option<&PathBuf>,
    schema_version: Option<&str>,
    execution: ExecutionMode,
    env_id: &str,
) -> Result<WizardRunResult> {
    match run_interactive_with_zero_action(
        initial_mode,
        emit_answers,
        schema_version,
        execution,
        RootMenuZeroAction::Exit,
        env_id,
    )? {
        Some(result) => Ok(result),
        None => bail!("{}", crate::i18n::tr("wizard.exit.message")),
    }
}

pub fn run_interactive_with_zero_action(
    initial_mode: Option<WizardMode>,
    emit_answers: Option<&PathBuf>,
    schema_version: Option<&str>,
    execution: ExecutionMode,
    zero_action: RootMenuZeroAction,
    env_id: &str,
) -> Result<Option<WizardRunResult>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    loop {
        let Some(selection) = collect_guided_interactive_request(
            &mut input,
            &mut output,
            initial_mode,
            zero_action,
            env_id,
        )?
        else {
            return Ok(None);
        };
        let InteractiveSelection::Request(interactive) = selection else {
            if initial_mode.is_none() {
                continue;
            }
            return Ok(None);
        };
        let resolved_execution = match execution {
            ExecutionMode::DryRun => ExecutionMode::DryRun,
            ExecutionMode::Execute => match interactive.review_action {
                ReviewAction::BuildNow => ExecutionMode::Execute,
                ReviewAction::DryRunOnly | ReviewAction::SaveAnswersOnly => ExecutionMode::DryRun,
            },
        };
        return Ok(Some(execute_request(
            interactive.request,
            resolved_execution,
            matches!(interactive.review_action, ReviewAction::BuildNow)
                && resolved_execution == ExecutionMode::Execute,
            schema_version,
            emit_answers,
            None,
            env_id,
        )?));
    }
}

fn collect_guided_interactive_request<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    initial_mode: Option<WizardMode>,
    zero_action: RootMenuZeroAction,
    env_id: &str,
) -> Result<Option<InteractiveSelection>> {
    if let Some(mode) = initial_mode {
        let interactive = match mode {
            WizardMode::Create => collect_create_flow(input, output, env_id)?,
            WizardMode::Update => collect_update_flow(input, output, false, env_id)?,
            WizardMode::Doctor => collect_doctor_flow(input, output, env_id)?,
        };
        return Ok(Some(InteractiveSelection::Request(Box::new(interactive))));
    }

    let Some(choice) = choose_interactive_menu(input, output, zero_action)? else {
        return Ok(None);
    };

    match choice {
        InteractiveChoice::Create => Ok(Some(InteractiveSelection::Request(Box::new(
            collect_create_flow(input, output, env_id)?,
        )))),
        InteractiveChoice::Update => Ok(Some(InteractiveSelection::Request(Box::new(
            collect_update_flow(input, output, false, env_id)?,
        )))),
        InteractiveChoice::Validate => Ok(Some(InteractiveSelection::Request(Box::new(
            collect_update_flow(input, output, true, env_id)?,
        )))),
        InteractiveChoice::Doctor => {
            perform_doctor_action(input, output)?;
            Ok(Some(InteractiveSelection::Handled))
        }
        InteractiveChoice::Inspect => {
            perform_inspect_action(input, output)?;
            Ok(Some(InteractiveSelection::Handled))
        }
        InteractiveChoice::Unbundle => {
            perform_unbundle_action(input, output)?;
            Ok(Some(InteractiveSelection::Handled))
        }
    }
}

fn choose_interactive_menu<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    zero_action: RootMenuZeroAction,
) -> Result<Option<InteractiveChoice>> {
    writeln!(output, "{}", crate::i18n::tr("wizard.menu.title"))?;
    write_root_menu_option(
        output,
        "1",
        &crate::i18n::tr("wizard.mode.create"),
        &crate::i18n::tr("wizard.menu_desc.create"),
    )?;
    write_root_menu_option(
        output,
        "2",
        &crate::i18n::tr("wizard.mode.update"),
        &crate::i18n::tr("wizard.menu_desc.update"),
    )?;
    write_root_menu_option(
        output,
        "3",
        &crate::i18n::tr("wizard.mode.validate"),
        &crate::i18n::tr("wizard.menu_desc.validate"),
    )?;
    write_root_menu_option(
        output,
        "4",
        &crate::i18n::tr("wizard.mode.doctor"),
        &crate::i18n::tr("wizard.menu_desc.doctor"),
    )?;
    write_root_menu_option(
        output,
        "5",
        &crate::i18n::tr("wizard.mode.inspect"),
        &crate::i18n::tr("wizard.menu_desc.inspect"),
    )?;
    write_root_menu_option(
        output,
        "6",
        &crate::i18n::tr("wizard.mode.unbundle"),
        &crate::i18n::tr("wizard.menu_desc.unbundle"),
    )?;
    let zero_label = match zero_action {
        RootMenuZeroAction::Exit => crate::i18n::tr("wizard.menu.exit"),
        RootMenuZeroAction::Back => crate::i18n::tr("wizard.action.back"),
    };
    writeln!(output, "0. {zero_label}")?;
    loop {
        write!(output, "{} ", crate::i18n::tr("wizard.setup.enum_prompt"))?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
        }
        match line.trim() {
            "0" => match zero_action {
                RootMenuZeroAction::Exit => bail!("{}", crate::i18n::tr("wizard.exit.message")),
                RootMenuZeroAction::Back => return Ok(None),
            },
            "1" | "create" => return Ok(Some(InteractiveChoice::Create)),
            "2" | "update" | "open" => return Ok(Some(InteractiveChoice::Update)),
            "3" | "validate" => return Ok(Some(InteractiveChoice::Validate)),
            "4" | "doctor" => return Ok(Some(InteractiveChoice::Doctor)),
            "5" | "inspect" => return Ok(Some(InteractiveChoice::Inspect)),
            "6" | "unbundle" => return Ok(Some(InteractiveChoice::Unbundle)),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn write_root_menu_option<W: Write>(
    output: &mut W,
    number: &str,
    title: &str,
    description: &str,
) -> Result<()> {
    writeln!(output, "{number}. {title}")?;
    writeln!(output, "   {description}")?;
    Ok(())
}

fn collect_create_flow<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    _env_id: &str,
) -> Result<InteractiveRequest> {
    let locale = crate::i18n::current_locale();
    let bundle_name = prompt_required_string(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.bundle_name"),
        None,
    )?;
    let bundle_id = normalize_bundle_id(&prompt_required_string(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.bundle_id"),
        None,
    )?);
    let mut state = normalize_request(SeedRequest {
        mode: WizardMode::Create,
        locale,
        bundle_name,
        bundle_id: bundle_id.clone(),
        output_dir: PathBuf::from(prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.output_dir"),
            Some(&default_bundle_output_dir(&bundle_id).display().to_string()),
        )?),
        app_pack_entries: Vec::new(),
        access_rules: Vec::new(),
        extension_provider_entries: Vec::new(),
        cloud_deployer_target: None,
        advanced_setup: false,
        app_packs: Vec::new(),
        extension_providers: Vec::new(),
        remote_catalogs: Vec::new(),
        setup_specs: BTreeMap::new(),
        setup_answers: BTreeMap::new(),
        setup_execution_intent: false,
        export_intent: false,
        capabilities: Vec::new(),
    });
    state = edit_app_packs(input, output, state, false)?;
    state = edit_extension_providers(input, output, state, false)?;
    state = edit_cloud_deployer_target(input, output, state)?;
    state = edit_bundle_capabilities(input, output, state)?;
    let review_action = review_summary(input, output, &state, false)?;
    Ok(InteractiveRequest {
        request: state,
        review_action,
    })
}

fn collect_update_flow<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    validate_only: bool,
    _env_id: &str,
) -> Result<InteractiveRequest> {
    let (target, mut state) = prompt_request_from_bundle_target(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.current_bundle_root"),
        WizardMode::Update,
    )?;
    if matches!(target, BundleTarget::Artifact(_)) && !validate_only {
        state.output_dir = PathBuf::from(prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.output_dir"),
            Some(&state.output_dir.display().to_string()),
        )?);
    }
    state.bundle_name = prompt_required_string(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.bundle_name"),
        Some(&state.bundle_name),
    )?;
    state.bundle_id = normalize_bundle_id(&prompt_required_string(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.bundle_id"),
        Some(&state.bundle_id),
    )?);
    if !validate_only {
        state = edit_app_packs(input, output, state, true)?;
        state = edit_extension_providers(input, output, state, true)?;
        state = edit_cloud_deployer_target(input, output, state)?;
        state = edit_bundle_capabilities(input, output, state)?;
        let review_action = review_summary(input, output, &state, true)?;
        Ok(InteractiveRequest {
            request: state,
            review_action,
        })
    } else {
        Ok(InteractiveRequest {
            request: state,
            review_action: ReviewAction::DryRunOnly,
        })
    }
}

fn collect_doctor_flow<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    _env_id: &str,
) -> Result<InteractiveRequest> {
    Ok(InteractiveRequest {
        request: prompt_request_from_bundle_target(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.current_bundle_root"),
            WizardMode::Doctor,
        )?
        .1,
        review_action: ReviewAction::DryRunOnly,
    })
}

fn perform_doctor_action<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<()> {
    run_prompted_bundle_target_action(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.bundle_target"),
        |target| match target {
            BundleTarget::Workspace(root) => crate::build::doctor_target(Some(root), None),
            BundleTarget::Artifact(artifact) => crate::build::doctor_target(None, Some(artifact)),
        },
    )
}

fn perform_inspect_action<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<()> {
    loop {
        let target = prompt_bundle_target(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.bundle_target"),
        )?;
        match inspect_bundle_target(output, &target) {
            Ok(()) => return Ok(()),
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

fn perform_unbundle_action<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<()> {
    loop {
        let artifact = prompt_bundle_artifact_path(input, output)?;
        let out = prompt_optional_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.unbundle_output_dir"),
            Some("."),
        )?;
        match crate::build::unbundle_artifact(&artifact, Path::new(&out)) {
            Ok(result) => {
                writeln!(output, "{}", serde_json::to_string_pretty(&result)?)?;
                return Ok(());
            }
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

fn prompt_bundle_target<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
) -> Result<BundleTarget> {
    loop {
        let raw = prompt_required_string(input, output, title, None)?;
        match parse_bundle_target(PathBuf::from(raw)) {
            Ok(target) => return Ok(target),
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

fn prompt_bundle_artifact_path<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<PathBuf> {
    loop {
        let raw = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.bundle_artifact"),
            None,
        )?;
        let path = PathBuf::from(raw);
        if !is_bundle_artifact_path(&path) {
            writeln!(
                output,
                "{}",
                crate::i18n::tr("wizard.error.bundle_artifact_required")
            )?;
            continue;
        }
        if !path.exists() {
            writeln!(
                output,
                "{}: {}",
                crate::i18n::tr("wizard.error.bundle_target_missing"),
                path.display()
            )?;
            continue;
        }
        return Ok(path);
    }
}

fn prompt_request_from_bundle_target<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    mode: WizardMode,
) -> Result<(BundleTarget, NormalizedRequest)> {
    loop {
        let target = prompt_bundle_target(input, output, title)?;
        match request_from_bundle_target(&target, mode) {
            Ok(request) => return Ok((target, request)),
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

fn run_prompted_bundle_target_action<R: BufRead, W: Write, T, F>(
    input: &mut R,
    output: &mut W,
    title: &str,
    action: F,
) -> Result<()>
where
    T: Serialize,
    F: Fn(&BundleTarget) -> Result<T>,
{
    loop {
        let target = prompt_bundle_target(input, output, title)?;
        match action(&target) {
            Ok(report) => {
                writeln!(output, "{}", serde_json::to_string_pretty(&report)?)?;
                return Ok(());
            }
            Err(error) => writeln!(output, "{error}")?,
        }
    }
}

fn inspect_bundle_target<W: Write>(output: &mut W, target: &BundleTarget) -> Result<()> {
    let report = match target {
        BundleTarget::Workspace(root) => crate::build::inspect_target(Some(root), None)?,
        BundleTarget::Artifact(artifact) => crate::build::inspect_target(None, Some(artifact))?,
    };
    if report.kind == "artifact" {
        for entry in report.contents.as_deref().unwrap_or(&[]) {
            writeln!(output, "{entry}")?;
        }
    } else {
        writeln!(output, "{}", serde_json::to_string_pretty(&report)?)?;
    }
    Ok(())
}

fn parse_bundle_target(path: PathBuf) -> Result<BundleTarget> {
    if !path.exists() {
        bail!(
            "{}: {}",
            crate::i18n::tr("wizard.error.bundle_target_missing"),
            path.display()
        );
    }
    if is_bundle_artifact_path(&path) {
        Ok(BundleTarget::Artifact(path))
    } else {
        Ok(BundleTarget::Workspace(path))
    }
}

fn request_from_bundle_target(
    target: &BundleTarget,
    mode: WizardMode,
) -> Result<NormalizedRequest> {
    match target {
        BundleTarget::Workspace(root) => {
            let workspace = crate::project::read_bundle_workspace(root)
                .with_context(|| format!("read current bundle workspace {}", root.display()))?;
            Ok(request_from_workspace(&workspace, root, mode))
        }
        BundleTarget::Artifact(artifact) => {
            let staging = tempfile::tempdir().with_context(|| {
                format!("create temporary workspace for {}", artifact.display())
            })?;
            crate::build::unbundle_artifact(artifact, staging.path())?;
            let workspace =
                crate::project::read_bundle_workspace(staging.path()).with_context(|| {
                    format!("read unbundled bundle workspace {}", artifact.display())
                })?;
            let mut request = request_from_workspace(&workspace, staging.path(), mode);
            request.output_dir = default_workspace_dir_for_artifact(artifact);
            Ok(request)
        }
    }
}

fn default_workspace_dir_for_artifact(artifact: &Path) -> PathBuf {
    let stem = artifact
        .file_stem()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "bundle".into());
    artifact
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(stem)
}

fn is_bundle_artifact_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("gtbundle"))
}

fn execution_for_run(dry_run: bool) -> ExecutionMode {
    if dry_run {
        ExecutionMode::DryRun
    } else {
        ExecutionMode::Execute
    }
}

fn execute_request(
    request: NormalizedRequest,
    execution: ExecutionMode,
    build_bundle_now: bool,
    schema_version: Option<&str>,
    emit_answers: Option<&PathBuf>,
    source_locks: Option<BTreeMap<String, Value>>,
    env_id: &str,
) -> Result<WizardRunResult> {
    let target_version = requested_schema_version(schema_version)?;
    if !request.remote_catalogs.is_empty() {
        eprintln!(
            "[resolve] Resolving {} remote catalog(s)...",
            request.remote_catalogs.len()
        );
    }
    let catalog_resolution = crate::catalog::resolve::resolve_catalogs(
        &request.output_dir,
        &request.remote_catalogs,
        &crate::catalog::resolve::CatalogResolveOptions {
            offline: crate::runtime::offline(),
            write_cache: execution == ExecutionMode::Execute,
        },
    )?;
    if !request.remote_catalogs.is_empty() {
        eprintln!(
            "[resolve] Catalog resolution complete ({} entries)",
            catalog_resolution.entries.len()
        );
    }
    let request = discover_setup_specs(request, &catalog_resolution);
    let setup_writes = preview_setup_writes(&request, execution, env_id)?;
    let bundle_lock = build_bundle_lock(
        &request,
        execution,
        &catalog_resolution,
        &setup_writes,
        env_id,
    );
    let plan = build_plan(
        &request,
        execution,
        build_bundle_now,
        &target_version,
        &catalog_resolution.cache_writes,
        &setup_writes,
    );
    let mut document = answer_document_from_request(&request, Some(&target_version.to_string()))?;
    document.env_id = Some(env_id.to_string());
    let mut locks = source_locks.unwrap_or_default();
    locks.extend(bundle_lock_to_answer_locks(&bundle_lock));
    document.locks = locks;
    let applied_files = if execution == ExecutionMode::Execute {
        let mut applied_files = apply_plan(&request, &bundle_lock, env_id)?;
        if build_bundle_now {
            let build_result =
                crate::build::build_workspace(&request.output_dir, None, false, false, None)?;
            applied_files.push(PathBuf::from(build_result.artifact_path));
        }
        applied_files.sort();
        applied_files.dedup();
        applied_files
    } else {
        Vec::new()
    };
    if let Some(path) = emit_answers {
        write_answer_document(path, &document)?;
    }
    Ok(WizardRunResult {
        plan,
        document,
        applied_files,
    })
}

#[allow(dead_code)]
fn collect_interactive_request<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    initial_mode: Option<WizardMode>,
    last_compact_title: &mut Option<String>,
) -> Result<NormalizedRequest> {
    let mode = match initial_mode {
        Some(mode) => mode,
        None => choose_mode_via_qa(input, output, env_id, last_compact_title)?,
    };
    let request = match mode {
        WizardMode::Update => collect_update_request(input, output, env_id, last_compact_title)?,
        WizardMode::Create | WizardMode::Doctor => {
            let answers = run_qa_form(
                input,
                output,
                env_id,
                &wizard_request_form_spec_json(mode, None)?,
                None,
                "root wizard",
                last_compact_title,
            )?;
            normalized_request_from_qa_answers(answers, crate::i18n::current_locale(), mode)?
        }
    };
    collect_interactive_setup_answers(input, output, env_id, request, last_compact_title)
}

#[allow(dead_code)]
fn parse_csv_answers(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[allow(dead_code)]
fn choose_mode_via_qa<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    last_compact_title: &mut Option<String>,
) -> Result<WizardMode> {
    let config = WizardRunConfig {
        spec_json: json!({
            "id": "greentic-bundle-wizard-mode",
            "title": crate::i18n::tr("wizard.menu.title"),
            "version": "1.0.0",
            "presentation": {
                "default_locale": crate::i18n::current_locale()
            },
            "progress_policy": {
                "skip_answered": true,
                "autofill_defaults": false,
                "treat_default_as_answered": false
            },
            "questions": [{
                "id": "mode",
                "type": "enum",
                "title": crate::i18n::tr("wizard.prompt.main_choice"),
                "required": true,
                "choices": ["create", "update", "doctor"]
            }]
        })
        .to_string(),
        initial_answers_json: None,
        frontend: WizardFrontend::JsonUi,
        i18n: I18nConfig {
            locale: Some(crate::i18n::current_locale()),
            resolved: None,
            debug: false,
        },
        verbose: false,
        env_id: env_id.to_string(),
    };
    let mut driver =
        WizardDriver::new(config).context("initialize greentic-qa-lib wizard mode form")?;

    loop {
        driver
            .next_payload_json()
            .context("render greentic-qa-lib wizard mode payload")?;
        if driver.is_complete() {
            break;
        }

        let ui_raw = driver
            .last_ui_json()
            .ok_or_else(|| anyhow::anyhow!("greentic-qa-lib wizard mode missing UI state"))?;
        let ui: Value =
            serde_json::from_str(ui_raw).context("parse greentic-qa-lib wizard mode UI payload")?;
        let question = ui
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|questions| questions.first())
            .ok_or_else(|| anyhow::anyhow!("greentic-qa-lib wizard mode missing question"))?;

        let answer = prompt_wizard_mode_question(input, output, question)?;
        driver
            .submit_patch_json(&json!({ "mode": answer }).to_string())
            .context("submit greentic-qa-lib wizard mode answer")?;
    }
    *last_compact_title = Some(crate::i18n::tr("wizard.menu.title"));

    let answers = driver
        .finish()
        .context("finish greentic-qa-lib wizard mode")?
        .answer_set
        .answers;

    Ok(
        match answers
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("create")
        {
            "update" => WizardMode::Update,
            "doctor" => WizardMode::Doctor,
            _ => WizardMode::Create,
        },
    )
}

#[allow(dead_code)]
fn prompt_wizard_mode_question<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    question: &Value,
) -> Result<Value> {
    writeln!(output, "{}", crate::i18n::tr("wizard.menu.title"))?;
    let choices = question
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("wizard mode question missing choices"))?;
    for (index, choice) in choices.iter().enumerate() {
        let choice = choice
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("wizard mode choice must be a string"))?;
        writeln!(
            output,
            "{}. {}",
            index + 1,
            crate::i18n::tr(&format!("wizard.mode.{choice}"))
        )?;
    }
    prompt_compact_enum(
        input,
        output,
        question,
        true,
        question_default_value(question, "enum"),
    )
}

#[allow(dead_code)]
fn prompt_compact_enum<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    question: &Value,
    required: bool,
    default_value: Option<Value>,
) -> Result<Value> {
    let choices = question
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("qa enum question missing choices"))?
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    loop {
        write!(output, "{} ", crate::i18n::tr("wizard.setup.enum_prompt"))?;
        output.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(default) = &default_value {
                return Ok(default.clone());
            }
            if required {
                writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
                continue;
            }
            return Ok(Value::Null);
        }
        if let Ok(number) = trimmed.parse::<usize>()
            && number > 0
            && number <= choices.len()
        {
            return Ok(Value::String(choices[number - 1].clone()));
        }
        if choices.iter().any(|choice| choice == trimmed) {
            return Ok(Value::String(trimmed.to_string()));
        }
        writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?;
    }
}

#[allow(dead_code)]
fn collect_update_request<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    last_compact_title: &mut Option<String>,
) -> Result<NormalizedRequest> {
    let root_answers = run_qa_form(
        input,
        output,
        env_id,
        &json!({
            "id": "greentic-bundle-update-root",
            "title": crate::i18n::tr("wizard.menu.update"),
            "version": "1.0.0",
            "presentation": {
                "default_locale": crate::i18n::current_locale()
            },
            "progress_policy": {
                "skip_answered": true,
                "autofill_defaults": false,
                "treat_default_as_answered": false
            },
            "questions": [{
                "id": "output_dir",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.current_bundle_root"),
                "required": true
            }]
        })
        .to_string(),
        None,
        "update bundle root",
        last_compact_title,
    )?;
    let root = PathBuf::from(
        root_answers
            .get("output_dir")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("update wizard missing current bundle root"))?,
    );
    let workspace = crate::project::read_bundle_workspace(&root)
        .with_context(|| format!("read current bundle workspace {}", root.display()))?;
    let defaults = request_defaults_from_workspace(&workspace, &root);
    let answers = run_qa_form(
        input,
        output,
        env_id,
        &wizard_request_form_spec_json(WizardMode::Update, Some(&defaults))?,
        None,
        "update wizard",
        last_compact_title,
    )?;
    normalized_request_from_qa_answers(answers, crate::i18n::current_locale(), WizardMode::Update)
}

#[allow(dead_code)]
fn request_defaults_from_workspace(
    workspace: &crate::project::BundleWorkspaceDefinition,
    root: &Path,
) -> RequestDefaults {
    RequestDefaults {
        bundle_name: Some(workspace.bundle_name.clone()),
        bundle_id: Some(workspace.bundle_id.clone()),
        output_dir: Some(root.display().to_string()),
        advanced_setup: Some(workspace.advanced_setup.to_string()),
        app_packs: Some(workspace.app_packs.join(", ")),
        extension_providers: Some(workspace.extension_providers.join(", ")),
        cloud_deployer_target: detect_cloud_deployer_target(&workspace.extension_providers),
        remote_catalogs: Some(workspace.remote_catalogs.join(", ")),
        setup_execution_intent: Some(workspace.setup_execution_intent.to_string()),
        export_intent: Some(workspace.export_intent.to_string()),
    }
}

#[allow(dead_code)]
fn run_qa_form<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    spec_json: &str,
    initial_answers_json: Option<String>,
    context_label: &str,
    last_compact_title: &mut Option<String>,
) -> Result<Value> {
    let config = WizardRunConfig {
        spec_json: spec_json.to_string(),
        initial_answers_json,
        frontend: WizardFrontend::Text,
        i18n: I18nConfig {
            locale: Some(crate::i18n::current_locale()),
            resolved: None,
            debug: false,
        },
        verbose: false,
        env_id: env_id.to_string(),
    };
    let mut driver = WizardDriver::new(config)
        .with_context(|| format!("initialize greentic-qa-lib {context_label}"))?;
    loop {
        let payload_raw = driver
            .next_payload_json()
            .with_context(|| format!("render greentic-qa-lib {context_label} payload"))?;
        let payload: Value = serde_json::from_str(&payload_raw)
            .with_context(|| format!("parse greentic-qa-lib {context_label} payload"))?;
        if let Some(text) = payload.get("text").and_then(Value::as_str) {
            render_qa_driver_text(output, text, last_compact_title)?;
        }
        if driver.is_complete() {
            break;
        }

        let ui_raw = driver.last_ui_json().ok_or_else(|| {
            anyhow::anyhow!("greentic-qa-lib {context_label} payload missing UI state")
        })?;
        let ui: Value = serde_json::from_str(ui_raw)
            .with_context(|| format!("parse greentic-qa-lib {context_label} UI payload"))?;
        let question_id = ui
            .get("next_question_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!("greentic-qa-lib {context_label} missing next_question_id")
            })?
            .to_string();
        let question = ui
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|questions| {
                questions.iter().find(|question| {
                    question.get("id").and_then(Value::as_str) == Some(question_id.as_str())
                })
            })
            .ok_or_else(|| {
                anyhow::anyhow!("greentic-qa-lib {context_label} missing question {question_id}")
            })?;

        let answer = prompt_qa_question_answer(input, output, &question_id, question)?;
        driver
            .submit_patch_json(&json!({ question_id: answer }).to_string())
            .with_context(|| format!("submit greentic-qa-lib {context_label} answer"))?;
    }

    let result = driver
        .finish()
        .with_context(|| format!("finish greentic-qa-lib {context_label}"))?;
    Ok(result.answer_set.answers)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
struct RequestDefaults {
    bundle_name: Option<String>,
    bundle_id: Option<String>,
    output_dir: Option<String>,
    advanced_setup: Option<String>,
    app_packs: Option<String>,
    extension_providers: Option<String>,
    cloud_deployer_target: Option<String>,
    remote_catalogs: Option<String>,
    setup_execution_intent: Option<String>,
    export_intent: Option<String>,
}

#[allow(dead_code)]
fn wizard_request_form_spec_json(
    mode: WizardMode,
    defaults: Option<&RequestDefaults>,
) -> Result<String> {
    let defaults = defaults.cloned().unwrap_or_default();
    let output_dir_default = defaults.output_dir.clone().or_else(|| {
        if matches!(mode, WizardMode::Create) {
            defaults
                .bundle_id
                .as_deref()
                .map(default_bundle_output_dir)
                .map(|path| path.display().to_string())
        } else {
            None
        }
    });
    Ok(json!({
        "id": format!("greentic-bundle-root-wizard-{}", mode_name(mode)),
        "title": crate::i18n::tr("wizard.menu.title"),
        "version": "1.0.0",
        "presentation": {
            "default_locale": crate::i18n::current_locale()
        },
        "progress_policy": {
            "skip_answered": true,
            "autofill_defaults": false,
            "treat_default_as_answered": false
        },
        "questions": [
            {
                "id": "bundle_name",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.bundle_name"),
                "required": true,
                "default_value": defaults.bundle_name
            },
            {
                "id": "bundle_id",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.bundle_id"),
                "required": true,
                "default_value": defaults.bundle_id
            },
            {
                "id": "output_dir",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.output_dir"),
                "required": !matches!(mode, WizardMode::Create),
                "default_value": output_dir_default
            },
            {
                "id": "advanced_setup",
                "type": "boolean",
                "title": crate::i18n::tr("wizard.prompt.advanced_setup"),
                "required": true,
                "default_value": defaults.advanced_setup.unwrap_or_else(|| "false".to_string())
            },
            {
                "id": "app_packs",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.app_packs"),
                "required": false,
                "default_value": defaults.app_packs,
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            },
            {
                "id": "extension_providers",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.extension_providers"),
                "required": false,
                "default_value": defaults.extension_providers,
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            },
            {
                "id": "cloud_deployer_target",
                "type": "string",
                "title": "Cloud deployer target",
                "required": false,
                "default_value": defaults.cloud_deployer_target,
                "enum": ["", "aws", "azure", "gcp"],
                "enum_labels": ["None", "AWS", "Azure", "GCP"],
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            },
            {
                "id": "remote_catalogs",
                "type": "string",
                "title": crate::i18n::tr("wizard.prompt.remote_catalogs"),
                "required": false,
                "default_value": defaults.remote_catalogs,
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            },
            {
                "id": "setup_execution_intent",
                "type": "boolean",
                "title": crate::i18n::tr("wizard.prompt.setup_execution"),
                "required": true,
                "default_value": defaults
                    .setup_execution_intent
                    .unwrap_or_else(|| "false".to_string()),
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            },
            {
                "id": "export_intent",
                "type": "boolean",
                "title": crate::i18n::tr("wizard.prompt.export_intent"),
                "required": true,
                "default_value": defaults.export_intent.unwrap_or_else(|| "false".to_string()),
                "visible_if": { "op": "var", "path": "/advanced_setup" }
            }
        ]
    })
    .to_string())
}

#[derive(Debug)]
struct SeedRequest {
    mode: WizardMode,
    locale: String,
    bundle_name: String,
    bundle_id: String,
    output_dir: PathBuf,
    app_pack_entries: Vec<AppPackEntry>,
    access_rules: Vec<AccessRuleInput>,
    extension_provider_entries: Vec<ExtensionProviderEntry>,
    cloud_deployer_target: Option<String>,
    advanced_setup: bool,
    app_packs: Vec<String>,
    extension_providers: Vec<String>,
    remote_catalogs: Vec<String>,
    setup_specs: BTreeMap<String, Value>,
    setup_answers: BTreeMap<String, Value>,
    setup_execution_intent: bool,
    export_intent: bool,
    capabilities: Vec<String>,
}

fn normalize_request(seed: SeedRequest) -> NormalizedRequest {
    let bundle_id = normalize_bundle_id(&seed.bundle_id);
    let mut app_pack_entries = seed.app_pack_entries;
    if app_pack_entries.is_empty() {
        app_pack_entries = seed
            .app_packs
            .iter()
            .map(|reference| AppPackEntry {
                reference: reference.clone(),
                detected_kind: "legacy".to_string(),
                pack_id: inferred_reference_id(reference),
                display_name: inferred_display_name(reference),
                version: inferred_reference_version(reference),
                mapping: AppPackMappingInput {
                    scope: "global".to_string(),
                    tenant: None,
                    team: None,
                },
            })
            .collect();
    }
    app_pack_entries.sort_by(|left, right| left.reference.cmp(&right.reference));
    app_pack_entries.dedup_by(|left, right| {
        left.reference == right.reference
            && left.mapping.scope == right.mapping.scope
            && left.mapping.tenant == right.mapping.tenant
            && left.mapping.team == right.mapping.team
    });
    let mut app_packs = seed.app_packs;
    app_packs.extend(app_pack_entries.iter().map(|entry| entry.reference.clone()));

    let mut extension_provider_entries = seed.extension_provider_entries;
    if let Some(target) = seed.cloud_deployer_target.as_deref() {
        apply_cloud_deployer_target_to_entries(&mut extension_provider_entries, target);
    }
    if extension_provider_entries.is_empty() {
        extension_provider_entries = seed
            .extension_providers
            .iter()
            .map(|reference| ExtensionProviderEntry {
                reference: reference.clone(),
                detected_kind: "legacy".to_string(),
                provider_id: inferred_reference_id(reference),
                display_name: inferred_display_name(reference),
                version: inferred_reference_version(reference),
                source_catalog: None,
                group: None,
            })
            .collect();
    }
    extension_provider_entries.sort_by(|left, right| left.reference.cmp(&right.reference));
    extension_provider_entries.dedup_by(|left, right| left.reference == right.reference);
    let mut extension_providers = seed.extension_providers;
    extension_providers.extend(
        extension_provider_entries
            .iter()
            .map(|entry| entry.reference.clone()),
    );

    let mut remote_catalogs = seed.remote_catalogs;
    remote_catalogs.extend(
        extension_provider_entries
            .iter()
            .filter_map(|entry| entry.source_catalog.clone()),
    );

    let access_rules = if seed.access_rules.is_empty() {
        derive_access_rules_from_entries(&app_pack_entries)
    } else {
        normalize_access_rules(seed.access_rules)
    };

    NormalizedRequest {
        mode: seed.mode,
        locale: crate::i18n::normalize_locale(&seed.locale).unwrap_or_else(|| "en".to_string()),
        bundle_name: seed.bundle_name.trim().to_string(),
        bundle_id,
        output_dir: normalize_output_dir(seed.output_dir),
        local_reference_base_dir: None,
        app_pack_entries,
        access_rules,
        extension_provider_entries,
        advanced_setup: seed.advanced_setup,
        app_packs: sorted_unique(app_packs),
        extension_providers: sorted_unique(extension_providers),
        remote_catalogs: sorted_unique(remote_catalogs),
        setup_specs: seed.setup_specs,
        setup_answers: seed.setup_answers,
        setup_execution_intent: seed.setup_execution_intent,
        export_intent: seed.export_intent,
        capabilities: sorted_unique(seed.capabilities),
    }
}

fn normalize_access_rules(mut rules: Vec<AccessRuleInput>) -> Vec<AccessRuleInput> {
    rules.retain(|rule| !rule.rule_path.trim().is_empty() && !rule.tenant.trim().is_empty());
    rules.sort_by(|left, right| {
        left.tenant
            .cmp(&right.tenant)
            .then(left.team.cmp(&right.team))
            .then(left.rule_path.cmp(&right.rule_path))
            .then(left.policy.cmp(&right.policy))
    });
    rules.dedup_by(|left, right| {
        left.tenant == right.tenant
            && left.team == right.team
            && left.rule_path == right.rule_path
            && left.policy == right.policy
    });
    rules
}

fn request_from_workspace(
    workspace: &crate::project::BundleWorkspaceDefinition,
    root: &Path,
    mode: WizardMode,
) -> NormalizedRequest {
    let app_pack_entries = if workspace.app_pack_mappings.is_empty() {
        workspace
            .app_packs
            .iter()
            .map(|reference| AppPackEntry {
                pack_id: inferred_reference_id(reference),
                display_name: inferred_display_name(reference),
                version: inferred_reference_version(reference),
                detected_kind: detected_reference_kind(root, reference).to_string(),
                reference: reference.clone(),
                mapping: AppPackMappingInput {
                    scope: "global".to_string(),
                    tenant: None,
                    team: None,
                },
            })
            .collect::<Vec<_>>()
    } else {
        workspace
            .app_pack_mappings
            .iter()
            .map(|mapping| AppPackEntry {
                pack_id: inferred_reference_id(&mapping.reference),
                display_name: inferred_display_name(&mapping.reference),
                version: inferred_reference_version(&mapping.reference),
                detected_kind: detected_reference_kind(root, &mapping.reference).to_string(),
                reference: mapping.reference.clone(),
                mapping: AppPackMappingInput {
                    scope: match mapping.scope {
                        crate::project::MappingScope::Global => "global".to_string(),
                        crate::project::MappingScope::Tenant => "tenant".to_string(),
                        crate::project::MappingScope::Team => "tenant_team".to_string(),
                    },
                    tenant: mapping.tenant.clone(),
                    team: mapping.team.clone(),
                },
            })
            .collect::<Vec<_>>()
    };

    let access_rules = derive_access_rules_from_entries(&app_pack_entries);
    let extension_provider_entries = workspace
        .extension_providers
        .iter()
        .map(|reference| ExtensionProviderEntry {
            provider_id: inferred_reference_id(reference),
            display_name: inferred_display_name(reference),
            version: inferred_reference_version(reference),
            detected_kind: detected_reference_kind(root, reference).to_string(),
            reference: reference.clone(),
            source_catalog: workspace.remote_catalogs.first().cloned(),
            group: None,
        })
        .collect();

    normalize_request(SeedRequest {
        mode,
        locale: workspace.locale.clone(),
        bundle_name: workspace.bundle_name.clone(),
        bundle_id: workspace.bundle_id.clone(),
        output_dir: root.to_path_buf(),
        app_pack_entries,
        access_rules,
        extension_provider_entries,
        cloud_deployer_target: detect_cloud_deployer_target(&workspace.extension_providers),
        advanced_setup: false,
        app_packs: workspace.app_packs.clone(),
        extension_providers: workspace.extension_providers.clone(),
        remote_catalogs: workspace.remote_catalogs.clone(),
        setup_specs: BTreeMap::new(),
        setup_answers: BTreeMap::new(),
        setup_execution_intent: false,
        export_intent: false,
        capabilities: workspace.capabilities.clone(),
    })
}

fn prompt_required_string<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    default: Option<&str>,
) -> Result<String> {
    loop {
        let value = prompt_optional_string(input, output, title, default)?;
        if !value.trim().is_empty() {
            return Ok(value);
        }
        writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
    }
}

fn prompt_optional_string<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    default: Option<&str>,
) -> Result<String> {
    let default_value = default.map(|value| Value::String(value.to_string()));
    let value = prompt_qa_string_like(input, output, title, false, false, default_value)?;
    Ok(value.as_str().unwrap_or_default().to_string())
}

fn edit_app_packs<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    mut state: NormalizedRequest,
    allow_back: bool,
) -> Result<NormalizedRequest> {
    loop {
        writeln!(output, "{}", crate::i18n::tr("wizard.stage.app_packs"))?;
        render_pack_entries(output, &state.app_pack_entries)?;
        writeln!(
            output,
            "1. {}",
            crate::i18n::tr("wizard.action.add_app_pack")
        )?;
        writeln!(
            output,
            "2. {}",
            crate::i18n::tr("wizard.action.edit_app_pack_mapping")
        )?;
        writeln!(
            output,
            "3. {}",
            crate::i18n::tr("wizard.action.remove_app_pack")
        )?;
        writeln!(output, "4. {}", crate::i18n::tr("wizard.action.continue"))?;
        if allow_back {
            writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
        }

        let answer = prompt_menu_value(input, output)?;
        match answer.as_str() {
            "1" => {
                if let Some(entry) = add_app_pack(input, output, &state)? {
                    state.app_pack_entries.push(entry);
                    state.access_rules = derive_access_rules_from_entries(&state.app_pack_entries);
                    state = rebuild_request(state);
                }
            }
            "2" => {
                if !state.app_pack_entries.is_empty() {
                    state = edit_pack_access(input, output, state, true)?;
                }
            }
            "3" => {
                remove_app_pack(input, output, &mut state)?;
                state.access_rules = derive_access_rules_from_entries(&state.app_pack_entries);
                state = rebuild_request(state);
            }
            "4" => {
                if state.app_pack_entries.is_empty() {
                    writeln!(
                        output,
                        "{}",
                        crate::i18n::tr("wizard.error.app_pack_required")
                    )?;
                    continue;
                }
                return Ok(state);
            }
            "0" if allow_back => return Ok(state),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn edit_pack_access<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    mut state: NormalizedRequest,
    allow_back: bool,
) -> Result<NormalizedRequest> {
    loop {
        writeln!(output, "{}", crate::i18n::tr("wizard.stage.pack_access"))?;
        render_pack_entries(output, &state.app_pack_entries)?;
        writeln!(
            output,
            "1. {}",
            crate::i18n::tr("wizard.action.change_scope")
        )?;
        writeln!(
            output,
            "2. {}",
            crate::i18n::tr("wizard.action.add_tenant_access")
        )?;
        writeln!(
            output,
            "3. {}",
            crate::i18n::tr("wizard.action.add_tenant_team_access")
        )?;
        writeln!(
            output,
            "4. {}",
            crate::i18n::tr("wizard.action.remove_scope")
        )?;
        writeln!(output, "5. {}", crate::i18n::tr("wizard.action.continue"))?;
        writeln!(
            output,
            "6. {}",
            crate::i18n::tr("wizard.action.advanced_access_rules")
        )?;
        if allow_back {
            writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
        }
        let answer = prompt_menu_value(input, output)?;
        match answer.as_str() {
            "1" => change_pack_scope(input, output, &mut state)?,
            "2" => add_pack_scope(input, output, &mut state, false)?,
            "3" => add_pack_scope(input, output, &mut state, true)?,
            "4" => remove_pack_scope(input, output, &mut state)?,
            "5" => return Ok(rebuild_request(state)),
            "6" => edit_advanced_access_rules(input, output, &mut state)?,
            "0" if allow_back => return Ok(rebuild_request(state)),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
        state.access_rules = derive_access_rules_from_entries(&state.app_pack_entries);
    }
}

fn edit_extension_providers<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    mut state: NormalizedRequest,
    allow_back: bool,
) -> Result<NormalizedRequest> {
    loop {
        writeln!(
            output,
            "{}",
            crate::i18n::tr("wizard.stage.extension_providers")
        )?;
        render_named_entries(
            output,
            &crate::i18n::tr("wizard.stage.current_extension_providers"),
            &state
                .extension_provider_entries
                .iter()
                .map(|entry| format!("{} [{}]", entry.display_name, entry.reference))
                .collect::<Vec<_>>(),
        )?;
        writeln!(
            output,
            "1. {}",
            crate::i18n::tr("wizard.action.add_common_extension_provider")
        )?;
        writeln!(
            output,
            "2. {}",
            crate::i18n::tr("wizard.action.add_custom_extension_provider")
        )?;
        writeln!(
            output,
            "3. {}",
            crate::i18n::tr("wizard.action.remove_extension_provider")
        )?;
        writeln!(output, "4. {}", crate::i18n::tr("wizard.action.continue"))?;
        if allow_back {
            writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
        }
        let answer = prompt_menu_value(input, output)?;
        match answer.as_str() {
            "1" => {
                if let Some(entry) = add_common_extension_provider(input, output, &state)? {
                    state.extension_provider_entries.push(entry);
                    state = rebuild_request(state);
                }
            }
            "2" => {
                if let Some(entry) = add_custom_extension_provider(input, output, &state)? {
                    state.extension_provider_entries.push(entry);
                    state = rebuild_request(state);
                }
            }
            "3" => {
                remove_extension_provider(input, output, &mut state)?;
                state = rebuild_request(state);
            }
            "4" => return Ok(state),
            "0" if allow_back => return Ok(state),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn review_summary<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &NormalizedRequest,
    include_edit_paths: bool,
) -> Result<ReviewAction> {
    loop {
        writeln!(output, "{}", crate::i18n::tr("wizard.stage.review"))?;
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.prompt.bundle_name"),
            state.bundle_name
        )?;
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.prompt.bundle_id"),
            state.bundle_id
        )?;
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.prompt.output_dir"),
            state.output_dir.display()
        )?;
        render_named_entries(
            output,
            &crate::i18n::tr("wizard.stage.current_app_packs"),
            &state
                .app_pack_entries
                .iter()
                .map(|entry| {
                    format!(
                        "{} [{} -> {}]",
                        entry.display_name,
                        entry.reference,
                        format_mapping(&entry.mapping)
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        render_named_entries(
            output,
            &crate::i18n::tr("wizard.stage.current_access_rules"),
            &state
                .access_rules
                .iter()
                .map(format_access_rule)
                .collect::<Vec<_>>(),
        )?;
        render_named_entries(
            output,
            &crate::i18n::tr("wizard.stage.current_extension_providers"),
            &state
                .extension_provider_entries
                .iter()
                .map(|entry| format!("{} [{}]", entry.display_name, entry.reference))
                .collect::<Vec<_>>(),
        )?;
        if !state.capabilities.is_empty() {
            render_named_entries(
                output,
                &crate::i18n::tr("wizard.stage.capabilities"),
                &state.capabilities,
            )?;
        }
        writeln!(
            output,
            "1. {}",
            crate::i18n::tr("wizard.action.build_bundle")
        )?;
        writeln!(
            output,
            "2. {}",
            crate::i18n::tr("wizard.action.dry_run_only")
        )?;
        writeln!(
            output,
            "3. {}",
            crate::i18n::tr("wizard.action.save_answers_only")
        )?;
        if include_edit_paths {
            writeln!(output, "4. {}", crate::i18n::tr("wizard.action.finish"))?;
        }
        writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
        let answer = prompt_menu_value(input, output)?;
        match answer.as_str() {
            "1" => return Ok(ReviewAction::BuildNow),
            "2" => return Ok(ReviewAction::DryRunOnly),
            "3" => return Ok(ReviewAction::SaveAnswersOnly),
            "4" if include_edit_paths => return Ok(ReviewAction::BuildNow),
            "0" => return Ok(ReviewAction::DryRunOnly),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn prompt_menu_value<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<String> {
    write!(output, "{} ", crate::i18n::tr("wizard.setup.enum_prompt"))?;
    output.flush()?;
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
    }
    Ok(line.trim().to_string())
}

fn render_named_entries<W: Write>(output: &mut W, title: &str, entries: &[String]) -> Result<()> {
    writeln!(output, "{title}:")?;
    if entries.is_empty() {
        writeln!(output, "- {}", crate::i18n::tr("wizard.list.none"))?;
    } else {
        for entry in entries {
            writeln!(output, "- {entry}")?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct PackGroup {
    reference: String,
    display_name: String,
    scopes: Vec<AppPackMappingInput>,
}

fn render_pack_entries<W: Write>(output: &mut W, entries: &[AppPackEntry]) -> Result<()> {
    writeln!(
        output,
        "{}",
        crate::i18n::tr("wizard.stage.current_app_packs")
    )?;
    let groups = group_pack_entries(entries);
    if groups.is_empty() {
        writeln!(output, "- {}", crate::i18n::tr("wizard.list.none"))?;
        return Ok(());
    }
    for (index, group) in groups.iter().enumerate() {
        writeln!(output, "{}) {}", index + 1, group.display_name)?;
        writeln!(
            output,
            "   {}: {}",
            crate::i18n::tr("wizard.label.source"),
            group.reference
        )?;
        writeln!(
            output,
            "   {}: {}",
            crate::i18n::tr("wizard.label.scope"),
            group
                .scopes
                .iter()
                .map(format_mapping)
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    Ok(())
}

fn group_pack_entries(entries: &[AppPackEntry]) -> Vec<PackGroup> {
    let mut groups = Vec::<PackGroup>::new();
    for entry in entries {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.reference == entry.reference)
        {
            group.scopes.push(entry.mapping.clone());
        } else {
            groups.push(PackGroup {
                reference: entry.reference.clone(),
                display_name: entry.display_name.clone(),
                scopes: vec![entry.mapping.clone()],
            });
        }
    }
    groups
}

fn edit_bundle_capabilities<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    mut state: NormalizedRequest,
) -> Result<NormalizedRequest> {
    let cap = crate::project::CAP_BUNDLE_ASSETS_READ_V1.to_string();
    let already_enabled = state.capabilities.contains(&cap);
    let default_value = if already_enabled {
        Some(Value::Bool(true))
    } else {
        Some(Value::Bool(false))
    };
    let answer = prompt_qa_boolean(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.enable_bundle_assets"),
        false,
        default_value,
    )?;
    let enable = answer.as_bool().unwrap_or(false);
    if enable && !state.capabilities.contains(&cap) {
        state.capabilities.push(cap);
    } else if !enable {
        state.capabilities.retain(|c| c != &cap);
    }
    Ok(state)
}

fn rebuild_request(request: NormalizedRequest) -> NormalizedRequest {
    normalize_request(SeedRequest {
        mode: request.mode,
        locale: request.locale,
        bundle_name: request.bundle_name,
        bundle_id: request.bundle_id,
        output_dir: request.output_dir,
        app_pack_entries: request.app_pack_entries,
        access_rules: request.access_rules,
        extension_provider_entries: request.extension_provider_entries,
        cloud_deployer_target: detect_cloud_deployer_target(&request.extension_providers),
        advanced_setup: false,
        app_packs: Vec::new(),
        extension_providers: Vec::new(),
        remote_catalogs: request.remote_catalogs,
        setup_specs: BTreeMap::new(),
        setup_answers: BTreeMap::new(),
        setup_execution_intent: false,
        export_intent: false,
        capabilities: request.capabilities,
    })
}

fn edit_cloud_deployer_target<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    mut state: NormalizedRequest,
) -> Result<NormalizedRequest> {
    let current = detect_cloud_deployer_target(&state.extension_providers);
    let labels = vec![
        "None".to_string(),
        "AWS".to_string(),
        "Azure".to_string(),
        "GCP".to_string(),
    ];
    let prompt = format!(
        "Choose cloud deployer target (current: {})",
        current.as_deref().unwrap_or("none")
    );
    let Some(index) = choose_named_index(input, output, &prompt, &labels)? else {
        return Ok(state);
    };
    let target = match index {
        1 => "aws",
        2 => "azure",
        3 => "gcp",
        _ => "",
    };
    apply_cloud_deployer_target_to_entries(&mut state.extension_provider_entries, target);
    Ok(rebuild_request(state))
}

fn apply_cloud_deployer_target_to_entries(entries: &mut Vec<ExtensionProviderEntry>, target: &str) {
    entries.retain(|entry| !is_cloud_deployer_entry(entry));
    if let Some(entry) = cloud_deployer_entry_for_target(target) {
        entries.push(entry);
    }
}

fn cloud_deployer_entry_for_target(target: &str) -> Option<ExtensionProviderEntry> {
    let (provider_id, display_name, reference) = match target.trim() {
        "aws" => ("deployer-aws", "AWS Deployer", DEPLOYER_AWS_REF),
        "azure" => ("deployer-azure", "Azure Deployer", DEPLOYER_AZURE_REF),
        "gcp" => ("deployer-gcp", "GCP Deployer", DEPLOYER_GCP_REF),
        _ => return None,
    };
    Some(ExtensionProviderEntry {
        reference: reference.to_string(),
        detected_kind: "catalog".to_string(),
        provider_id: provider_id.to_string(),
        display_name: display_name.to_string(),
        version: inferred_reference_version(reference),
        source_catalog: Some(DEFAULT_PROVIDER_REGISTRY.to_string()),
        group: Some("deployer".to_string()),
    })
}

fn is_cloud_deployer_entry(entry: &ExtensionProviderEntry) -> bool {
    matches!(
        entry.provider_id.as_str(),
        "deployer-aws" | "deployer-azure" | "deployer-gcp"
    ) || detect_cloud_deployer_target(std::slice::from_ref(&entry.reference)).is_some()
}

fn detect_cloud_deployer_target(references: &[String]) -> Option<String> {
    for reference in references {
        let normalized = reference.trim().to_ascii_lowercase();
        if normalized.contains("greentic.deploy.aws") || normalized.ends_with("/aws.gtpack") {
            return Some("aws".to_string());
        }
        if normalized.contains("greentic.deploy.azure") || normalized.ends_with("/azure.gtpack") {
            return Some("azure".to_string());
        }
        if normalized.contains("greentic.deploy.gcp") || normalized.ends_with("/gcp.gtpack") {
            return Some("gcp".to_string());
        }
    }
    None
}

fn format_mapping(mapping: &AppPackMappingInput) -> String {
    match mapping.scope.as_str() {
        "tenant" => format!("tenant:{}", mapping.tenant.clone().unwrap_or_default()),
        "tenant_team" => format!(
            "tenant/team:{}/{}",
            mapping.tenant.clone().unwrap_or_default(),
            mapping.team.clone().unwrap_or_default()
        ),
        _ => "global".to_string(),
    }
}

fn format_access_rule(rule: &AccessRuleInput) -> String {
    match &rule.team {
        Some(team) => format!(
            "{}/{team}: {} = {}",
            rule.tenant, rule.rule_path, rule.policy
        ),
        None => format!("{}: {} = {}", rule.tenant, rule.rule_path, rule.policy),
    }
}

fn derive_access_rules_from_entries(entries: &[AppPackEntry]) -> Vec<AccessRuleInput> {
    normalize_access_rules(
        entries
            .iter()
            .map(|entry| match entry.mapping.scope.as_str() {
                "tenant" => AccessRuleInput {
                    rule_path: entry.pack_id.clone(),
                    policy: "public".to_string(),
                    tenant: entry
                        .mapping
                        .tenant
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    team: None,
                },
                "tenant_team" => AccessRuleInput {
                    rule_path: entry.pack_id.clone(),
                    policy: "public".to_string(),
                    tenant: entry
                        .mapping
                        .tenant
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    team: entry.mapping.team.clone(),
                },
                _ => AccessRuleInput {
                    rule_path: entry.pack_id.clone(),
                    policy: "public".to_string(),
                    tenant: "default".to_string(),
                    team: None,
                },
            })
            .collect(),
    )
}

fn choose_pack_group_index<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    entries: &[AppPackEntry],
) -> Result<Option<usize>> {
    let groups = group_pack_entries(entries);
    choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_app_pack"),
        &groups
            .iter()
            .map(|group| format!("{} [{}]", group.display_name, group.reference))
            .collect::<Vec<_>>(),
    )
}

fn change_pack_scope<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    let Some(group_index) = choose_pack_group_index(input, output, &state.app_pack_entries)? else {
        return Ok(());
    };
    let groups = group_pack_entries(&state.app_pack_entries);
    let group = &groups[group_index];
    let template = state
        .app_pack_entries
        .iter()
        .find(|entry| entry.reference == group.reference)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing pack entry template"))?;
    let mapping = prompt_app_pack_mapping(input, output, &template.pack_id)?;
    state
        .app_pack_entries
        .retain(|entry| entry.reference != group.reference);
    let mut replacement = template;
    replacement.mapping = mapping;
    state.app_pack_entries.push(replacement);
    Ok(())
}

fn add_pack_scope<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
    include_team: bool,
) -> Result<()> {
    let Some(group_index) = choose_pack_group_index(input, output, &state.app_pack_entries)? else {
        return Ok(());
    };
    let groups = group_pack_entries(&state.app_pack_entries);
    let group = &groups[group_index];
    let template = state
        .app_pack_entries
        .iter()
        .find(|entry| entry.reference == group.reference)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing pack entry template"))?;
    let mapping = if include_team {
        let tenant = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.tenant_id"),
            Some("default"),
        )?;
        let team = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.team_id"),
            None,
        )?;
        AppPackMappingInput {
            scope: "tenant_team".to_string(),
            tenant: Some(tenant),
            team: Some(team),
        }
    } else {
        let tenant = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.tenant_id"),
            Some("default"),
        )?;
        AppPackMappingInput {
            scope: "tenant".to_string(),
            tenant: Some(tenant),
            team: None,
        }
    };
    if state
        .app_pack_entries
        .iter()
        .any(|entry| entry.reference == group.reference && entry.mapping == mapping)
    {
        return Ok(());
    }
    let mut addition = template;
    addition.mapping = mapping;
    state.app_pack_entries.push(addition);
    Ok(())
}

fn remove_pack_scope<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    let groups = group_pack_entries(&state.app_pack_entries);
    let Some(group_index) = choose_pack_group_index(input, output, &state.app_pack_entries)? else {
        return Ok(());
    };
    let group = &groups[group_index];
    let Some(scope_index) = choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_scope"),
        &group.scopes.iter().map(format_mapping).collect::<Vec<_>>(),
    )?
    else {
        return Ok(());
    };
    let target_scope = &group.scopes[scope_index];
    state
        .app_pack_entries
        .retain(|entry| !(entry.reference == group.reference && &entry.mapping == target_scope));
    Ok(())
}

fn edit_advanced_access_rules<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    writeln!(
        output,
        "{}",
        crate::i18n::tr("wizard.stage.advanced_access_rules")
    )?;
    render_named_entries(
        output,
        &crate::i18n::tr("wizard.stage.current_access_rules"),
        &state
            .access_rules
            .iter()
            .map(format_access_rule)
            .collect::<Vec<_>>(),
    )?;
    writeln!(
        output,
        "1. {}",
        crate::i18n::tr("wizard.action.add_allow_rule")
    )?;
    writeln!(
        output,
        "2. {}",
        crate::i18n::tr("wizard.action.remove_rule")
    )?;
    writeln!(
        output,
        "3. {}",
        crate::i18n::tr("wizard.action.return_simple_mode")
    )?;
    loop {
        match prompt_menu_value(input, output)?.as_str() {
            "1" => add_manual_access_rule(input, output, state, "public")?,
            "2" => remove_access_rule(input, output, state)?,
            "3" => return Ok(()),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
        state.access_rules = normalize_access_rules(state.access_rules.clone());
    }
}

fn add_app_pack<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &NormalizedRequest,
) -> Result<Option<AppPackEntry>> {
    loop {
        let raw = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.app_pack_reference"),
            None,
        )?;
        let resolved = match resolve_reference_metadata(&state.output_dir, &raw) {
            Ok(resolved) => resolved,
            Err(error) => {
                writeln!(output, "{error}")?;
                continue;
            }
        };
        writeln!(output, "{}", crate::i18n::tr("wizard.confirm.app_pack"))?;
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.label.pack_id"),
            resolved.id
        )?;
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.label.name"),
            resolved.display_name
        )?;
        if let Some(version) = &resolved.version {
            writeln!(
                output,
                "{}: {}",
                crate::i18n::tr("wizard.label.version"),
                version
            )?;
        }
        writeln!(
            output,
            "{}: {}",
            crate::i18n::tr("wizard.label.source"),
            resolved.reference
        )?;
        writeln!(
            output,
            "1. {}",
            crate::i18n::tr("wizard.action.add_this_app_pack")
        )?;
        writeln!(
            output,
            "2. {}",
            crate::i18n::tr("wizard.action.reenter_reference")
        )?;
        writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
        match prompt_menu_value(input, output)?.as_str() {
            "1" => {
                let mapping = prompt_app_pack_mapping(input, output, &resolved.id)?;
                return Ok(Some(AppPackEntry {
                    reference: resolved.reference,
                    detected_kind: resolved.detected_kind,
                    pack_id: resolved.id,
                    display_name: resolved.display_name,
                    version: resolved.version,
                    mapping,
                }));
            }
            "2" => continue,
            "0" => return Ok(None),
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn remove_app_pack<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    let Some(index) = choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_app_pack"),
        &state
            .app_pack_entries
            .iter()
            .map(|entry| format!("{} [{}]", entry.display_name, entry.reference))
            .collect::<Vec<_>>(),
    )?
    else {
        return Ok(());
    };
    state.app_pack_entries.remove(index);
    Ok(())
}

fn prompt_app_pack_mapping<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    pack_id: &str,
) -> Result<AppPackMappingInput> {
    writeln!(output, "{}", crate::i18n::tr("wizard.stage.map_app_pack"))?;
    writeln!(output, "{}", pack_id)?;
    writeln!(output, "1. {}", crate::i18n::tr("wizard.mapping.global"))?;
    writeln!(output, "2. {}", crate::i18n::tr("wizard.mapping.tenant"))?;
    writeln!(
        output,
        "3. {}",
        crate::i18n::tr("wizard.mapping.tenant_team")
    )?;
    writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
    loop {
        match prompt_menu_value(input, output)?.as_str() {
            "1" => {
                return Ok(AppPackMappingInput {
                    scope: "global".to_string(),
                    tenant: None,
                    team: None,
                });
            }
            "2" => {
                let tenant = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.tenant_id"),
                    Some("default"),
                )?;
                return Ok(AppPackMappingInput {
                    scope: "tenant".to_string(),
                    tenant: Some(tenant),
                    team: None,
                });
            }
            "3" => {
                let tenant = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.tenant_id"),
                    Some("default"),
                )?;
                let team = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.team_id"),
                    None,
                )?;
                return Ok(AppPackMappingInput {
                    scope: "tenant_team".to_string(),
                    tenant: Some(tenant),
                    team: Some(team),
                });
            }
            "0" => {
                return Ok(AppPackMappingInput {
                    scope: "global".to_string(),
                    tenant: None,
                    team: None,
                });
            }
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

fn add_manual_access_rule<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
    policy: &str,
) -> Result<()> {
    let target = prompt_access_target(input, output)?;
    let rule_path = prompt_required_string(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.rule_path"),
        None,
    )?;
    state.access_rules.push(AccessRuleInput {
        rule_path,
        policy: policy.to_string(),
        tenant: target.0,
        team: target.1,
    });
    Ok(())
}

fn remove_access_rule<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    let Some(index) = choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_access_rule"),
        &state
            .access_rules
            .iter()
            .map(format_access_rule)
            .collect::<Vec<_>>(),
    )?
    else {
        return Ok(());
    };
    state.access_rules.remove(index);
    Ok(())
}

fn prompt_access_target<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<(String, Option<String>)> {
    writeln!(output, "1. {}", crate::i18n::tr("wizard.mapping.global"))?;
    writeln!(output, "2. {}", crate::i18n::tr("wizard.mapping.tenant"))?;
    writeln!(
        output,
        "3. {}",
        crate::i18n::tr("wizard.mapping.tenant_team")
    )?;
    loop {
        match prompt_menu_value(input, output)?.as_str() {
            "1" => return Ok(("default".to_string(), None)),
            "2" => {
                let tenant = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.tenant_id"),
                    Some("default"),
                )?;
                return Ok((tenant, None));
            }
            "3" => {
                let tenant = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.tenant_id"),
                    Some("default"),
                )?;
                let team = prompt_required_string(
                    input,
                    output,
                    &crate::i18n::tr("wizard.prompt.team_id"),
                    None,
                )?;
                return Ok((tenant, Some(team)));
            }
            _ => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

/// Resolves extension provider catalog entries.
///
/// If `remote_catalogs` contains explicit references, they are used.
/// Otherwise, tries to fetch from OCI registry, falling back to bundled
/// provider registry if the fetch fails (network error, timeout, etc.).
///
/// Returns (should_persist_catalog_ref, catalog_ref, entries).
fn resolve_extension_provider_catalog(
    output_dir: &Path,
    remote_catalogs: &[String],
) -> Result<(
    bool,
    Option<String>,
    Vec<crate::catalog::registry::CatalogEntry>,
)> {
    // If explicit catalogs are provided, use them
    if let Some(catalog_ref) = remote_catalogs.first() {
        let resolution = crate::catalog::resolve::resolve_catalogs(
            output_dir,
            std::slice::from_ref(catalog_ref),
            &crate::catalog::resolve::CatalogResolveOptions {
                offline: crate::runtime::offline(),
                write_cache: false,
            },
        )?;
        return Ok((true, Some(catalog_ref.clone()), resolution.discovered_items));
    }

    // Check for bundled-only mode (used in tests and CI)
    let use_bundled_only = std::env::var("GREENTIC_BUNDLE_USE_BUNDLED_CATALOG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // No explicit catalogs - try OCI registry first, fall back to bundled
    if !crate::runtime::offline() && !use_bundled_only {
        let catalog_ref = DEFAULT_PROVIDER_REGISTRY.to_string();
        match crate::catalog::resolve::resolve_catalogs(
            output_dir,
            std::slice::from_ref(&catalog_ref),
            &crate::catalog::resolve::CatalogResolveOptions {
                offline: false,
                write_cache: false,
            },
        ) {
            Ok(resolution) if !resolution.discovered_items.is_empty() => {
                return Ok((true, Some(catalog_ref), resolution.discovered_items));
            }
            _ => {
                // OCI fetch failed or returned empty, fall back to bundled
            }
        }
    }

    // Fall back to bundled provider registry
    let entries = crate::catalog::registry::bundled_provider_registry_entries()?;
    Ok((false, None, entries))
}

fn add_common_extension_provider<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &NormalizedRequest,
) -> Result<Option<ExtensionProviderEntry>> {
    let (persist_catalog_ref, catalog_ref, entries) =
        resolve_extension_provider_catalog(&state.output_dir, &state.remote_catalogs)?;
    if entries.is_empty() {
        writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_catalog"))?;
        return Ok(None);
    }
    let grouped_entries = group_catalog_entries_by_category(&entries);
    let category_key = if grouped_entries.len() > 1 {
        let labels = grouped_entries
            .iter()
            .map(|(category_id, category_label, description, _)| {
                // Use category_label if available, otherwise fall back to category_id
                let display_name = category_label.as_deref().unwrap_or(category_id);
                format_extension_category_label(display_name, description.as_deref())
            })
            .collect::<Vec<_>>();
        let Some(index) = choose_named_index(input, output, "Choose extension category", &labels)?
        else {
            return Ok(None);
        };
        Some(grouped_entries[index].0.clone())
    } else {
        None
    };
    let selected_entries = category_key
        .as_deref()
        .map(|category| {
            entries
                .iter()
                .filter(|entry| entry.category.as_deref().unwrap_or("other") == category)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| entries.iter().collect::<Vec<_>>());
    let options = build_extension_provider_options(&selected_entries);
    let labels = options
        .iter()
        .map(|option| option.display_name.clone())
        .collect::<Vec<_>>();
    let Some(index) = choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_extension_provider"),
        &labels,
    )?
    else {
        return Ok(None);
    };
    let selected = &options[index];
    let entry = selected.entry;
    let reference = resolve_catalog_entry_reference(input, output, &entry.reference)?;
    Ok(Some(ExtensionProviderEntry {
        detected_kind: detected_reference_kind(&state.output_dir, &reference).to_string(),
        reference,
        provider_id: entry.id.clone(),
        display_name: selected.display_name.clone(),
        version: inferred_reference_version(&entry.reference),
        source_catalog: if persist_catalog_ref {
            catalog_ref
        } else {
            None
        },
        group: None,
    }))
}

fn build_extension_provider_options<'a>(
    entries: &'a [&'a crate::catalog::registry::CatalogEntry],
) -> Vec<ResolvedExtensionProviderOption<'a>> {
    let mut options = Vec::<ResolvedExtensionProviderOption<'a>>::new();
    for entry in entries {
        let display_name = clean_extension_provider_label(entry);
        if options
            .iter()
            .any(|existing| existing.display_name == display_name)
        {
            continue;
        }
        options.push(ResolvedExtensionProviderOption {
            entry,
            display_name,
        });
    }
    options
}

#[derive(Clone)]
struct ResolvedExtensionProviderOption<'a> {
    entry: &'a crate::catalog::registry::CatalogEntry,
    display_name: String,
}

fn clean_extension_provider_label(entry: &crate::catalog::registry::CatalogEntry) -> String {
    let raw = entry
        .label
        .clone()
        .unwrap_or_else(|| inferred_display_name(&entry.reference));
    let trimmed = raw.trim();
    for suffix in [" (latest)", " (Latest)", " (LATEST)"] {
        if let Some(base) = trimmed.strip_suffix(suffix) {
            return base.trim().to_string();
        }
    }
    trimmed.to_string()
}

/// (category_id, category_label, category_description, entry_indices)
type CategoryGroup = (String, Option<String>, Option<String>, Vec<usize>);

fn group_catalog_entries_by_category(
    entries: &[crate::catalog::registry::CatalogEntry],
) -> Vec<CategoryGroup> {
    let mut grouped = Vec::<CategoryGroup>::new();
    for (index, entry) in entries.iter().enumerate() {
        let category = entry
            .category
            .clone()
            .unwrap_or_else(|| "other".to_string());
        let label = entry.category_label.clone();
        let description = entry.category_description.clone();
        if let Some((_, existing_label, existing_description, indices)) =
            grouped.iter_mut().find(|(name, _, _, _)| name == &category)
        {
            if existing_label.is_none() {
                *existing_label = label.clone();
            }
            if existing_description.is_none() {
                *existing_description = description.clone();
            }
            indices.push(index);
        } else {
            grouped.push((category, label, description, vec![index]));
        }
    }
    grouped
}

fn format_extension_category_label(category: &str, description: Option<&str>) -> String {
    match description
        .map(str::trim)
        .filter(|description| !description.is_empty())
    {
        Some(description) => format!("{category} -> {description}"),
        None => category.to_string(),
    }
}

fn add_custom_extension_provider<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &NormalizedRequest,
) -> Result<Option<ExtensionProviderEntry>> {
    loop {
        let raw = prompt_required_string(
            input,
            output,
            &crate::i18n::tr("wizard.prompt.extension_provider_reference"),
            None,
        )?;
        let resolved = match resolve_reference_metadata(&state.output_dir, &raw) {
            Ok(resolved) => resolved,
            Err(error) => {
                writeln!(output, "{error}")?;
                continue;
            }
        };
        return Ok(Some(ExtensionProviderEntry {
            reference: resolved.reference,
            detected_kind: resolved.detected_kind,
            provider_id: resolved.id.clone(),
            display_name: resolved.display_name,
            version: resolved.version,
            source_catalog: None,
            group: None,
        }));
    }
}

fn remove_extension_provider<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    state: &mut NormalizedRequest,
) -> Result<()> {
    let Some(index) = choose_named_index(
        input,
        output,
        &crate::i18n::tr("wizard.prompt.choose_extension_provider"),
        &state
            .extension_provider_entries
            .iter()
            .map(|entry| format!("{} [{}]", entry.display_name, entry.reference))
            .collect::<Vec<_>>(),
    )?
    else {
        return Ok(());
    };
    state.extension_provider_entries.remove(index);
    Ok(())
}

fn choose_named_index<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    entries: &[String],
) -> Result<Option<usize>> {
    if entries.is_empty() {
        return Ok(None);
    }
    writeln!(output, "{title}:")?;
    for (index, entry) in entries.iter().enumerate() {
        writeln!(output, "{}. {}", index + 1, entry)?;
    }
    writeln!(output, "0. {}", crate::i18n::tr("wizard.action.back"))?;
    loop {
        let answer = prompt_menu_value(input, output)?;
        if answer == "0" {
            return Ok(None);
        }
        if let Ok(index) = answer.parse::<usize>()
            && index > 0
            && index <= entries.len()
        {
            return Ok(Some(index - 1));
        }
        writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?;
    }
}

struct ResolvedReferenceMetadata {
    reference: String,
    detected_kind: String,
    id: String,
    display_name: String,
    version: Option<String>,
}

fn resolve_reference_metadata(root: &Path, raw: &str) -> Result<ResolvedReferenceMetadata> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("{}", crate::i18n::tr("wizard.error.empty_answer"));
    }
    validate_reference_input(root, raw)?;
    let detected_kind = detected_reference_kind(root, raw).to_string();
    Ok(ResolvedReferenceMetadata {
        id: inferred_reference_id(raw),
        display_name: inferred_display_name(raw),
        version: inferred_reference_version(raw),
        reference: raw.to_string(),
        detected_kind,
    })
}

fn resolve_catalog_entry_reference<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    raw: &str,
) -> Result<String> {
    if !raw.contains("<pr-version>") {
        return Ok(raw.to_string());
    }
    let version = prompt_required_string(input, output, "PR version or tag", None)?;
    Ok(raw.replace("<pr-version>", version.trim()))
}

fn validate_reference_input(root: &Path, raw: &str) -> Result<()> {
    if raw.contains("<pr-version>") {
        bail!("Reference contains an unresolved <pr-version> placeholder.");
    }
    if let Some(path) = parse_local_gtpack_reference(root, raw) {
        let metadata = fs::metadata(&path)
            .with_context(|| format!("read local .gtpack {}", path.display()))?;
        if !metadata.is_file() {
            bail!(
                "Local .gtpack reference must point to a file: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn parse_local_gtpack_reference(root: &Path, raw: &str) -> Option<PathBuf> {
    if let Some(path) = raw.strip_prefix("file://") {
        let path = PathBuf::from(path.trim());
        return Some(path);
    }
    if raw.contains("://") || !raw.ends_with(".gtpack") {
        return None;
    }
    let candidate = PathBuf::from(raw);
    Some(if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    })
}

fn detected_reference_kind(root: &Path, raw: &str) -> &'static str {
    if raw.starts_with("file://") {
        return "file_uri";
    }
    if raw.starts_with("https://") {
        return "https";
    }
    if raw.starts_with("oci://") {
        return "oci";
    }
    if raw.starts_with("repo://") {
        return "repo";
    }
    if raw.starts_with("store://") {
        return "store";
    }
    if raw.contains("://") {
        return "unknown";
    }
    let path = PathBuf::from(raw);
    let resolved = if path.is_absolute() {
        path
    } else {
        root.join(&path)
    };
    if resolved.is_dir() {
        "local_dir"
    } else {
        "local_file"
    }
}

fn inferred_reference_id(raw: &str) -> String {
    let cleaned = raw
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(raw)
        .split('@')
        .next()
        .unwrap_or(raw)
        .split(':')
        .next()
        .unwrap_or(raw)
        .trim_end_matches(".json")
        .trim_end_matches(".gtpack")
        .trim_end_matches(".yaml")
        .trim_end_matches(".yml");
    normalize_bundle_id(cleaned)
}

fn inferred_display_name(raw: &str) -> String {
    inferred_reference_id(raw)
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn inferred_reference_version(raw: &str) -> Option<String> {
    raw.split('@').nth(1).map(ToOwned::to_owned).or_else(|| {
        raw.rsplit_once(':')
            .and_then(|(_, version)| (!version.contains('/')).then(|| version.to_string()))
    })
}

fn load_and_normalize_answers(
    path: &Path,
    mode_override: Option<WizardMode>,
    schema_version: Option<&str>,
    migrate: bool,
    locale: &str,
) -> Result<LoadedRequest> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read answers file {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| {
        anyhow::anyhow!(crate::i18n::trf(
            "errors.answer_document.invalid_json",
            &[("path", &path.display().to_string())],
        ))
    })?;
    let document = parse_answer_document(value, schema_version, migrate, locale)?;
    let locks = document.locks.clone();
    let build_bundle_now = answer_document_requests_bundle_build(&document);
    let request = normalized_request_from_document(document, mode_override)?;
    let request = NormalizedRequest {
        local_reference_base_dir: Some(answer_reference_base_dir(path)?),
        ..request
    };
    Ok(LoadedRequest {
        request,
        locks,
        build_bundle_now,
    })
}

fn answer_reference_base_dir(path: &Path) -> Result<PathBuf> {
    let base = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    match base {
        Some(parent) => parent
            .canonicalize()
            .with_context(|| format!("canonicalize answers parent {}", parent.display())),
        None => std::env::current_dir().context("resolve current working directory for answers"),
    }
}

fn reference_roots_for_apply(request: &NormalizedRequest) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    if let Some(base_dir) = &request.local_reference_base_dir {
        roots.push(base_dir.clone());
    }
    let current_dir = std::env::current_dir().context("resolve current working directory")?;
    if !roots.iter().any(|root| root == &current_dir) {
        roots.push(current_dir);
    }
    Ok(roots)
}

fn answer_document_requests_bundle_build(document: &AnswerDocument) -> bool {
    matches!(
        document.locks.get("execution").and_then(Value::as_str),
        Some("execute")
    )
}

fn parse_answer_document(
    value: Value,
    schema_version: Option<&str>,
    migrate: bool,
    locale: &str,
) -> Result<AnswerDocument> {
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!(crate::i18n::tr("errors.answer_document.invalid_root")))?;

    let has_metadata = object.contains_key("wizard_id")
        || object.contains_key("schema_id")
        || object.contains_key("schema_version")
        || object.contains_key("locale");

    let document = if has_metadata {
        let document: AnswerDocument =
            serde_json::from_value(Value::Object(object)).map_err(|_| {
                anyhow::anyhow!(crate::i18n::tr("errors.answer_document.invalid_document"))
            })?;
        document.validate()?;
        document
    } else if migrate {
        let mut document = AnswerDocument::new(locale);
        if let Some(Value::Object(answers)) = object.get("answers") {
            document.answers = answers
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
        } else {
            document.answers = object
                .iter()
                .filter(|(key, _)| key.as_str() != "locks")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
        }
        if let Some(Value::Object(locks)) = object.get("locks") {
            document.locks = locks
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
        }
        document
    } else {
        bail!(
            "{}",
            crate::i18n::tr("errors.answer_document.metadata_missing")
        );
    };

    if document.schema_id != ANSWER_SCHEMA_ID {
        bail!(
            "{}",
            crate::i18n::tr("errors.answer_document.schema_id_mismatch")
        );
    }

    let target_version = requested_schema_version(schema_version)?;
    let migrated = migrate_document(document, &target_version)?;
    if migrated.migrated && !migrate {
        bail!(
            "{}",
            crate::i18n::tr("errors.answer_document.migrate_required")
        );
    }
    Ok(migrated.document)
}

fn normalized_request_from_document(
    document: AnswerDocument,
    mode_override: Option<WizardMode>,
) -> Result<NormalizedRequest> {
    let answers = normalized_answers_from_document(&document.answers)?;
    let mode = match mode_override {
        Some(mode) => mode,
        None => mode_from_answers(&answers)?,
    };
    let bundle_name = required_string(&answers, "bundle_name")?;
    let bundle_id = required_string(&answers, "bundle_id")?;
    let normalized_bundle_id = normalize_bundle_id(&bundle_id);
    let output_dir = answers
        .get("output_dir")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_bundle_output_dir(&normalized_bundle_id));
    let request = normalize_request(SeedRequest {
        mode,
        locale: document.locale,
        bundle_name,
        bundle_id,
        output_dir,
        app_pack_entries: optional_app_pack_entries(&answers, "app_pack_entries")?,
        access_rules: optional_access_rules(&answers, "access_rules")?,
        extension_provider_entries: optional_extension_provider_entries(
            &answers,
            "extension_provider_entries",
        )?,
        cloud_deployer_target: optional_string(&answers, "cloud_deployer_target")?,
        advanced_setup: optional_bool(&answers, "advanced_setup")?,
        app_packs: optional_string_list(&answers, "app_packs")?,
        extension_providers: optional_string_list(&answers, "extension_providers")?,
        remote_catalogs: optional_string_list(&answers, "remote_catalogs")?,
        setup_specs: optional_object_map(&answers, "setup_specs")?,
        setup_answers: optional_object_map(&answers, "setup_answers")?,
        setup_execution_intent: optional_bool(&answers, "setup_execution_intent")?,
        export_intent: optional_bool(&answers, "export_intent")?,
        capabilities: optional_string_list(&answers, "capabilities")?,
    });
    validate_normalized_answer_request(&request)?;
    Ok(request)
}

fn normalized_answers_from_document(
    answers: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>> {
    let mut normalized = answers.clone();

    if let Some(Value::Object(bundle)) = answers.get("bundle") {
        copy_nested_string(bundle, "name", &mut normalized, "bundle_name")?;
        copy_nested_string(bundle, "id", &mut normalized, "bundle_id")?;
        copy_nested_string(bundle, "output_dir", &mut normalized, "output_dir")?;
    }

    if !normalized.contains_key("mode")
        && let Some(Value::String(action)) = answers.get("selected_action")
    {
        if action == "bundle" {
            normalized.insert("mode".to_string(), Value::String("create".to_string()));
        } else {
            return Err(invalid_answer_field("selected_action"));
        }
    }

    if !normalized.contains_key("app_packs")
        && !normalized.contains_key("app_pack_entries")
        && let Some(Value::Array(apps)) = answers.get("apps")
    {
        normalized.insert(
            "app_packs".to_string(),
            Value::Array(
                apps.iter()
                    .map(app_reference_from_launcher_entry)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }

    if !normalized.contains_key("extension_providers")
        && !normalized.contains_key("extension_provider_entries")
        && let Some(Value::Object(providers)) = answers.get("providers")
    {
        let mut refs = Vec::new();
        for key in ["messaging", "events"] {
            if let Some(value) = providers.get(key) {
                refs.extend(string_list_from_launcher_value(value, "providers")?);
            }
        }
        normalized.insert(
            "extension_providers".to_string(),
            Value::Array(refs.into_iter().map(Value::String).collect()),
        );
    }

    Ok(normalized)
}

fn copy_nested_string(
    object: &Map<String, Value>,
    source_key: &str,
    normalized: &mut BTreeMap<String, Value>,
    target_key: &str,
) -> Result<()> {
    if normalized.contains_key(target_key) {
        return Ok(());
    }
    match object.get(source_key) {
        None => Ok(()),
        Some(Value::String(value)) => {
            normalized.insert(target_key.to_string(), Value::String(value.clone()));
            Ok(())
        }
        Some(_) => Err(invalid_answer_field(target_key)),
    }
}

fn app_reference_from_launcher_entry(value: &Value) -> Result<String> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_answer_field("apps"))?;
    object
        .get("source")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| invalid_answer_field("apps"))
}

fn string_list_from_launcher_value(value: &Value, key: &str) -> Result<Vec<String>> {
    match value {
        Value::Array(entries) => entries
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| invalid_answer_field(key))
            })
            .collect(),
        _ => Err(invalid_answer_field(key)),
    }
}

#[allow(dead_code)]
fn normalized_request_from_qa_answers(
    answers: Value,
    locale: String,
    mode: WizardMode,
) -> Result<NormalizedRequest> {
    let object = answers
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("wizard answers must be a JSON object"))?;
    let bundle_name = object
        .get("bundle_name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("wizard answer missing bundle_name"))?
        .to_string();
    let bundle_id = normalize_bundle_id(
        object
            .get("bundle_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("wizard answer missing bundle_id"))?,
    );
    let output_dir = object
        .get("output_dir")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_bundle_output_dir(&bundle_id));

    Ok(normalize_request(SeedRequest {
        mode,
        locale,
        bundle_name,
        bundle_id,
        output_dir,
        app_pack_entries: Vec::new(),
        access_rules: Vec::new(),
        extension_provider_entries: Vec::new(),
        cloud_deployer_target: object
            .get("cloud_deployer_target")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        advanced_setup: object
            .get("advanced_setup")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        app_packs: parse_csv_answers(
            object
                .get("app_packs")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        extension_providers: parse_csv_answers(
            object
                .get("extension_providers")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        remote_catalogs: parse_csv_answers(
            object
                .get("remote_catalogs")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        setup_specs: BTreeMap::new(),
        setup_answers: BTreeMap::new(),
        setup_execution_intent: object
            .get("setup_execution_intent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        export_intent: object
            .get("export_intent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        capabilities: object
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

fn mode_from_answers(answers: &BTreeMap<String, Value>) -> Result<WizardMode> {
    match answers.get("mode") {
        None => Ok(WizardMode::Create),
        Some(Value::String(value)) => match value.to_ascii_lowercase().as_str() {
            "create" => Ok(WizardMode::Create),
            "update" => Ok(WizardMode::Update),
            "doctor" => Ok(WizardMode::Doctor),
            _ => Err(invalid_answer_field("mode")),
        },
        Some(_) => Err(invalid_answer_field("mode")),
    }
}

fn required_string(answers: &BTreeMap<String, Value>, key: &str) -> Result<String> {
    let value = answers.get(key).ok_or_else(|| missing_answer_field(key))?;
    let text = value.as_str().ok_or_else(|| invalid_answer_field(key))?;
    if text.trim().is_empty() {
        return Err(invalid_answer_field(key));
    }
    Ok(text.to_string())
}

fn optional_bool(answers: &BTreeMap<String, Value>, key: &str) -> Result<bool> {
    match answers.get(key) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(invalid_answer_field(key)),
    }
}

fn optional_string(answers: &BTreeMap<String, Value>, key: &str) -> Result<Option<String>> {
    match answers.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(_) => Err(invalid_answer_field(key)),
    }
}

fn optional_string_list(answers: &BTreeMap<String, Value>, key: &str) -> Result<Vec<String>> {
    match answers.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(entries)) => entries
            .iter()
            .map(|entry| {
                entry
                    .as_str()
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| invalid_answer_field(key))
            })
            .collect(),
        Some(_) => Err(invalid_answer_field(key)),
    }
}

fn optional_object_map(
    answers: &BTreeMap<String, Value>,
    key: &str,
) -> Result<BTreeMap<String, Value>> {
    match answers.get(key) {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(entries)) => Ok(entries
            .iter()
            .map(|(entry_key, entry_value)| (entry_key.clone(), entry_value.clone()))
            .collect()),
        Some(_) => Err(invalid_answer_field(key)),
    }
}

fn optional_app_pack_entries(
    answers: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Vec<AppPackEntry>> {
    answers
        .get(key)
        .cloned()
        .map(|value| serde_json::from_value(value).map_err(|_| invalid_answer_field(key)))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn optional_access_rules(
    answers: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Vec<AccessRuleInput>> {
    answers
        .get(key)
        .cloned()
        .map(|value| serde_json::from_value(value).map_err(|_| invalid_answer_field(key)))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn optional_extension_provider_entries(
    answers: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Vec<ExtensionProviderEntry>> {
    answers
        .get(key)
        .cloned()
        .map(|value| serde_json::from_value(value).map_err(|_| invalid_answer_field(key)))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn missing_answer_field(key: &str) -> anyhow::Error {
    anyhow::anyhow!(crate::i18n::trf(
        "errors.answer_document.answer_missing",
        &[("field", key)],
    ))
}

fn invalid_answer_field(key: &str) -> anyhow::Error {
    anyhow::anyhow!(crate::i18n::trf(
        "errors.answer_document.answer_invalid",
        &[("field", key)],
    ))
}

fn validate_normalized_answer_request(request: &NormalizedRequest) -> Result<()> {
    if request.bundle_name.trim().is_empty() {
        bail!(
            "{}",
            crate::i18n::trf(
                "errors.answer_document.answer_invalid",
                &[("field", "bundle_name")]
            )
        );
    }
    if request.bundle_id.trim().is_empty() {
        bail!(
            "{}",
            crate::i18n::trf(
                "errors.answer_document.answer_invalid",
                &[("field", "bundle_id")]
            )
        );
    }
    Ok(())
}

fn requested_schema_version(schema_version: Option<&str>) -> Result<Version> {
    let raw = schema_version.unwrap_or("1.0.0");
    Version::parse(raw).with_context(|| format!("invalid schema version {raw}"))
}

fn answer_document_from_request(
    request: &NormalizedRequest,
    schema_version: Option<&str>,
) -> Result<AnswerDocument> {
    let mut document = AnswerDocument::new(&request.locale);
    document.schema_version = requested_schema_version(schema_version)?;
    document.answers = BTreeMap::from([
        (
            "mode".to_string(),
            Value::String(mode_name(request.mode).to_string()),
        ),
        (
            "bundle_name".to_string(),
            Value::String(request.bundle_name.clone()),
        ),
        (
            "bundle_id".to_string(),
            Value::String(request.bundle_id.clone()),
        ),
        (
            "output_dir".to_string(),
            Value::String(request.output_dir.display().to_string()),
        ),
        (
            "advanced_setup".to_string(),
            Value::Bool(request.advanced_setup),
        ),
        (
            "app_pack_entries".to_string(),
            serde_json::to_value(&request.app_pack_entries)?,
        ),
        (
            "app_packs".to_string(),
            Value::Array(
                request
                    .app_packs
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "extension_providers".to_string(),
            Value::Array(
                request
                    .extension_providers
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "extension_provider_entries".to_string(),
            serde_json::to_value(&request.extension_provider_entries)?,
        ),
        (
            "remote_catalogs".to_string(),
            Value::Array(
                request
                    .remote_catalogs
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "setup_execution_intent".to_string(),
            Value::Bool(request.setup_execution_intent),
        ),
        (
            "setup_specs".to_string(),
            Value::Object(request.setup_specs.clone().into_iter().collect()),
        ),
        (
            "access_rules".to_string(),
            serde_json::to_value(&request.access_rules)?,
        ),
        (
            "setup_answers".to_string(),
            Value::Object(request.setup_answers.clone().into_iter().collect()),
        ),
        (
            "export_intent".to_string(),
            Value::Bool(request.export_intent),
        ),
        (
            "capabilities".to_string(),
            Value::Array(
                request
                    .capabilities
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
    ]);
    Ok(document)
}

pub fn build_plan(
    request: &NormalizedRequest,
    execution: ExecutionMode,
    build_bundle_now: bool,
    schema_version: &Version,
    cache_writes: &[String],
    setup_writes: &[String],
) -> WizardPlanEnvelope {
    let mut expected_file_writes = vec![
        request
            .output_dir
            .join(crate::project::WORKSPACE_ROOT_FILE)
            .display()
            .to_string(),
        request
            .output_dir
            .join("tenants/default/tenant.gmap")
            .display()
            .to_string(),
        request
            .output_dir
            .join(crate::project::LOCK_FILE)
            .display()
            .to_string(),
    ];
    expected_file_writes.extend(
        cache_writes
            .iter()
            .map(|path| request.output_dir.join(path).display().to_string()),
    );
    expected_file_writes.extend(
        setup_writes
            .iter()
            .map(|path| request.output_dir.join(path).display().to_string()),
    );
    if build_bundle_now && execution == ExecutionMode::Execute {
        expected_file_writes.push(
            crate::build::default_artifact_path(&request.output_dir, &request.bundle_id)
                .display()
                .to_string(),
        );
    }
    expected_file_writes.sort();
    expected_file_writes.dedup();
    let mut warnings = Vec::new();
    if request.advanced_setup
        && request.app_packs.is_empty()
        && request.extension_providers.is_empty()
    {
        warnings.push(crate::i18n::tr("wizard.warning.advanced_without_refs"));
    }

    WizardPlanEnvelope {
        metadata: PlanMetadata {
            wizard_id: WIZARD_ID.to_string(),
            schema_id: ANSWER_SCHEMA_ID.to_string(),
            schema_version: schema_version.to_string(),
            locale: request.locale.clone(),
            execution,
        },
        target_root: request.output_dir.display().to_string(),
        requested_action: mode_name(request.mode).to_string(),
        normalized_input_summary: normalized_summary(request),
        ordered_step_list: plan_steps(request, build_bundle_now),
        expected_file_writes,
        warnings,
    }
}

fn normalized_summary(request: &NormalizedRequest) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "mode".to_string(),
            Value::String(mode_name(request.mode).to_string()),
        ),
        (
            "bundle_name".to_string(),
            Value::String(request.bundle_name.clone()),
        ),
        (
            "bundle_id".to_string(),
            Value::String(request.bundle_id.clone()),
        ),
        (
            "output_dir".to_string(),
            Value::String(request.output_dir.display().to_string()),
        ),
        (
            "advanced_setup".to_string(),
            Value::Bool(request.advanced_setup),
        ),
        (
            "app_pack_entries".to_string(),
            serde_json::to_value(&request.app_pack_entries).unwrap_or(Value::Null),
        ),
        (
            "app_packs".to_string(),
            Value::Array(
                request
                    .app_packs
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "extension_providers".to_string(),
            Value::Array(
                request
                    .extension_providers
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "extension_provider_entries".to_string(),
            serde_json::to_value(&request.extension_provider_entries).unwrap_or(Value::Null),
        ),
        (
            "remote_catalogs".to_string(),
            Value::Array(
                request
                    .remote_catalogs
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "setup_execution_intent".to_string(),
            Value::Bool(request.setup_execution_intent),
        ),
        (
            "access_rules".to_string(),
            serde_json::to_value(&request.access_rules).unwrap_or(Value::Null),
        ),
        (
            "setup_spec_providers".to_string(),
            Value::Array(
                request
                    .setup_specs
                    .keys()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
        (
            "export_intent".to_string(),
            Value::Bool(request.export_intent),
        ),
    ])
}

fn plan_steps(request: &NormalizedRequest, build_bundle_now: bool) -> Vec<WizardPlanStep> {
    let mut steps = vec![
        WizardPlanStep {
            kind: StepKind::EnsureWorkspace,
            description: crate::i18n::tr("wizard.plan.ensure_workspace"),
        },
        WizardPlanStep {
            kind: StepKind::WriteBundleFile,
            description: crate::i18n::tr("wizard.plan.write_bundle_file"),
        },
        WizardPlanStep {
            kind: StepKind::UpdateAccessRules,
            description: crate::i18n::tr("wizard.plan.update_access_rules"),
        },
        WizardPlanStep {
            kind: StepKind::ResolveRefs,
            description: crate::i18n::tr("wizard.plan.resolve_refs"),
        },
        WizardPlanStep {
            kind: StepKind::WriteLock,
            description: crate::i18n::tr("wizard.plan.write_lock"),
        },
    ];
    if build_bundle_now || matches!(request.mode, WizardMode::Doctor) {
        steps.push(WizardPlanStep {
            kind: StepKind::BuildBundle,
            description: crate::i18n::tr("wizard.plan.build_bundle"),
        });
    }
    if request.export_intent {
        steps.push(WizardPlanStep {
            kind: StepKind::ExportBundle,
            description: crate::i18n::tr("wizard.plan.export_bundle"),
        });
    }
    steps
}

fn apply_plan(
    request: &NormalizedRequest,
    bundle_lock: &crate::project::BundleLock,
    env_id: &str,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(&request.output_dir)
        .with_context(|| format!("create output dir {}", request.output_dir.display()))?;
    let bundle_yaml = request.output_dir.join(crate::project::WORKSPACE_ROOT_FILE);
    let tenant_gmap = request.output_dir.join("tenants/default/tenant.gmap");
    let lock_file = request.output_dir.join(crate::project::LOCK_FILE);

    let workspace = workspace_definition_from_request(request);
    let mut writes = crate::project::init_bundle_workspace(&request.output_dir, &workspace)?;

    for entry in &request.app_pack_entries {
        if let Some(tenant) = &entry.mapping.tenant {
            if let Some(team) = &entry.mapping.team {
                crate::project::ensure_team(&request.output_dir, tenant, team)?;
            } else {
                crate::project::ensure_tenant(&request.output_dir, tenant)?;
            }
        }
    }

    for rule in &request.access_rules {
        let preview = crate::access::mutate_access(
            &request.output_dir,
            &crate::access::GmapTarget {
                tenant: rule.tenant.clone(),
                team: rule.team.clone(),
            },
            &crate::access::GmapMutation {
                rule_path: rule.rule_path.clone(),
                policy: match rule.policy.as_str() {
                    "forbidden" => crate::access::Policy::Forbidden,
                    _ => crate::access::Policy::Public,
                },
            },
            false,
        )?;
        writes.extend(
            preview
                .writes
                .into_iter()
                .map(|path| request.output_dir.join(path)),
        );
    }

    let setup_result = persist_setup_state(request, ExecutionMode::Execute, env_id)?;
    crate::project::write_bundle_lock(&request.output_dir, bundle_lock)
        .with_context(|| format!("write {}", lock_file.display()))?;
    crate::project::sync_project_with_reference_roots(
        &request.output_dir,
        &reference_roots_for_apply(request)?,
    )?;

    if request
        .capabilities
        .iter()
        .any(|c| c == crate::project::CAP_BUNDLE_ASSETS_READ_V1)
    {
        let scaffolded = crate::project::scaffold_assets_from_packs(&request.output_dir)?;
        writes.extend(scaffolded);
    }

    writes.push(bundle_yaml);
    writes.push(tenant_gmap);
    writes.push(lock_file);
    writes.extend(
        setup_result
            .writes
            .into_iter()
            .map(|path| request.output_dir.join(path)),
    );
    writes.sort();
    writes.dedup();
    Ok(writes)
}

fn workspace_definition_from_request(
    request: &NormalizedRequest,
) -> crate::project::BundleWorkspaceDefinition {
    let mut workspace = crate::project::BundleWorkspaceDefinition::new(
        request.bundle_name.clone(),
        request.bundle_id.clone(),
        request.locale.clone(),
        mode_name(request.mode).to_string(),
    );
    workspace.advanced_setup = request.advanced_setup;
    workspace.app_pack_mappings = request
        .app_pack_entries
        .iter()
        .map(|entry| crate::project::AppPackMapping {
            reference: entry.reference.clone(),
            scope: match entry.mapping.scope.as_str() {
                "tenant" => crate::project::MappingScope::Tenant,
                "tenant_team" => crate::project::MappingScope::Team,
                _ => crate::project::MappingScope::Global,
            },
            tenant: entry.mapping.tenant.clone(),
            team: entry.mapping.team.clone(),
        })
        .collect();
    workspace.app_packs = request.app_packs.clone();
    workspace.extension_providers = request.extension_providers.clone();
    workspace.remote_catalogs = request.remote_catalogs.clone();
    workspace.capabilities = request.capabilities.clone();
    workspace.setup_execution_intent = false;
    workspace.export_intent = false;
    workspace.canonicalize();
    workspace
}

fn write_answer_document(path: &Path, document: &AnswerDocument) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create answers parent {}", parent.display()))?;
    }
    fs::write(path, document.to_pretty_json_string()?)
        .with_context(|| format!("write answers file {}", path.display()))
}

fn normalize_bundle_id(raw: &str) -> String {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    normalized.trim_matches('-').to_string()
}

fn normalize_output_dir(path: PathBuf) -> PathBuf {
    if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path
    }
}

fn default_bundle_output_dir(bundle_id: &str) -> PathBuf {
    let normalized = normalize_bundle_id(bundle_id);
    if normalized.is_empty() {
        PathBuf::from("./bundle")
    } else {
        PathBuf::from(format!("./{normalized}-bundle"))
    }
}

fn sorted_unique(entries: Vec<String>) -> Vec<String> {
    let mut entries = entries
        .into_iter()
        .filter(|entry| !entry.trim().is_empty())
        .collect::<Vec<_>>();
    entries.sort();
    entries.dedup();
    entries
}

fn mode_name(mode: WizardMode) -> &'static str {
    match mode {
        WizardMode::Create => "create",
        WizardMode::Update => "update",
        WizardMode::Doctor => "doctor",
    }
}

pub fn print_plan(plan: &WizardPlanEnvelope) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(plan)?);
    Ok(())
}

fn build_bundle_lock(
    request: &NormalizedRequest,
    execution: ExecutionMode,
    catalog_resolution: &crate::catalog::resolve::CatalogResolution,
    setup_writes: &[String],
    env_id: &str,
) -> crate::project::BundleLock {
    crate::project::BundleLock {
        schema_version: crate::project::LOCK_SCHEMA_VERSION,
        bundle_id: request.bundle_id.clone(),
        env_id: Some(env_id.to_string()),
        requested_mode: mode_name(request.mode).to_string(),
        execution: match execution {
            ExecutionMode::DryRun => "dry_run",
            ExecutionMode::Execute => "execute",
        }
        .to_string(),
        cache_policy: crate::catalog::DEFAULT_CACHE_POLICY.to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        build_format_version: "bundle-lock-v1".to_string(),
        workspace_root: crate::project::WORKSPACE_ROOT_FILE.to_string(),
        lock_file: crate::project::LOCK_FILE.to_string(),
        catalogs: catalog_resolution.entries.clone(),
        app_packs: request
            .app_packs
            .iter()
            .cloned()
            .map(|reference| crate::project::DependencyLock {
                reference,
                digest: None,
            })
            .collect(),
        extension_providers: request
            .extension_providers
            .iter()
            .cloned()
            .map(|reference| crate::project::DependencyLock {
                reference,
                digest: None,
            })
            .collect(),
        setup_state_files: setup_writes.to_vec(),
    }
}

fn bundle_lock_to_answer_locks(lock: &crate::project::BundleLock) -> BTreeMap<String, Value> {
    let catalogs = lock
        .catalogs
        .iter()
        .map(|entry| {
            serde_json::json!({
                "requested_ref": entry.requested_ref,
                "resolved_ref": entry.resolved_ref,
                "digest": entry.digest,
                "source": entry.source,
                "item_count": entry.item_count,
                "item_ids": entry.item_ids,
                "cache_path": entry.cache_path,
            })
        })
        .collect::<Vec<_>>();

    BTreeMap::from([
        (
            "cache_policy".to_string(),
            Value::String(lock.cache_policy.clone()),
        ),
        (
            "workspace_root".to_string(),
            Value::String(lock.workspace_root.clone()),
        ),
        (
            "lock_file".to_string(),
            Value::String(lock.lock_file.clone()),
        ),
        (
            "requested_mode".to_string(),
            Value::String(lock.requested_mode.clone()),
        ),
        (
            "execution".to_string(),
            Value::String(lock.execution.clone()),
        ),
        ("catalogs".to_string(), Value::Array(catalogs)),
        (
            "setup_state_files".to_string(),
            Value::Array(
                lock.setup_state_files
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        ),
    ])
}

fn preview_setup_writes(
    request: &NormalizedRequest,
    execution: ExecutionMode,
    env_id: &str,
) -> Result<Vec<String>> {
    let _ = execution;
    let instructions = collect_setup_instructions(request)?;
    if instructions.is_empty() {
        return Ok(Vec::new());
    }
    let scope = crate::setup::persist::SetupScope {
        env_id,
        bundle_id: &request.bundle_id,
    };
    Ok(crate::setup::persist::persist_setup(
        &request.output_dir,
        &instructions,
        &crate::setup::backend::NoopSetupBackend,
        &scope,
    )?
    .writes)
}

fn persist_setup_state(
    request: &NormalizedRequest,
    execution: ExecutionMode,
    env_id: &str,
) -> Result<crate::setup::persist::SetupPersistenceResult> {
    let instructions = collect_setup_instructions(request)?;
    if instructions.is_empty() {
        return Ok(crate::setup::persist::SetupPersistenceResult {
            states: Vec::new(),
            writes: Vec::new(),
        });
    }

    let backend: Box<dyn crate::setup::backend::SetupBackend> = match execution {
        ExecutionMode::Execute => Box::new(crate::setup::backend::FileSetupBackend::new(
            &request.output_dir,
        )),
        ExecutionMode::DryRun => Box::new(crate::setup::backend::NoopSetupBackend),
    };
    let scope = crate::setup::persist::SetupScope {
        env_id,
        bundle_id: &request.bundle_id,
    };
    crate::setup::persist::persist_setup(
        &request.output_dir,
        &instructions,
        backend.as_ref(),
        &scope,
    )
}

fn collect_setup_instructions(
    request: &NormalizedRequest,
) -> Result<Vec<crate::setup::persist::SetupInstruction>> {
    if !request.setup_execution_intent {
        return Ok(Vec::new());
    }
    crate::setup::persist::collect_setup_instructions(&request.setup_specs, &request.setup_answers)
}

#[allow(dead_code)]
fn collect_interactive_setup_answers<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    request: NormalizedRequest,
    last_compact_title: &mut Option<String>,
) -> Result<NormalizedRequest> {
    if !request.setup_execution_intent {
        return Ok(request);
    }

    let catalog_resolution = crate::catalog::resolve::resolve_catalogs(
        &request.output_dir,
        &request.remote_catalogs,
        &crate::catalog::resolve::CatalogResolveOptions {
            offline: crate::runtime::offline(),
            write_cache: false,
        },
    )?;
    let mut request = discover_setup_specs(request, &catalog_resolution);
    let provider_ids = request.setup_specs.keys().cloned().collect::<Vec<_>>();
    for provider_id in provider_ids {
        let needs_answers = request
            .setup_answers
            .get(&provider_id)
            .and_then(Value::as_object)
            .map(|answers| answers.is_empty())
            .unwrap_or(true);
        if !needs_answers {
            continue;
        }

        let spec_input = request
            .setup_specs
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing setup spec for {provider_id}"))?;
        let parsed = serde_json::from_value::<crate::setup::SetupSpecInput>(spec_input)?;
        let (_, form) = crate::setup::form_spec_from_input(&parsed, &provider_id)?;
        let answers = prompt_setup_form_answers(
            input,
            output,
            env_id,
            &provider_id,
            &form,
            last_compact_title,
        )?;
        request
            .setup_answers
            .insert(provider_id, Value::Object(answers.into_iter().collect()));
    }

    Ok(request)
}

#[allow(dead_code)]
fn prompt_setup_form_answers<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    env_id: &str,
    provider_id: &str,
    form: &crate::setup::FormSpec,
    last_compact_title: &mut Option<String>,
) -> Result<BTreeMap<String, Value>> {
    writeln!(
        output,
        "{} {} ({provider_id})",
        crate::i18n::tr("wizard.setup.form_prefix"),
        form.title
    )?;
    let spec_json = serde_json::to_string(&qa_form_spec_from_setup_form(form)?)?;
    let config = WizardRunConfig {
        spec_json,
        initial_answers_json: None,
        frontend: WizardFrontend::Text,
        i18n: I18nConfig {
            locale: Some(crate::i18n::current_locale()),
            resolved: None,
            debug: false,
        },
        verbose: false,
        env_id: env_id.to_string(),
    };
    let mut driver =
        WizardDriver::new(config).context("initialize greentic-qa-lib setup wizard")?;
    loop {
        let payload_raw = driver
            .next_payload_json()
            .context("render greentic-qa-lib setup payload")?;
        let payload: Value =
            serde_json::from_str(&payload_raw).context("parse greentic-qa-lib setup payload")?;

        if let Some(text) = payload.get("text").and_then(Value::as_str) {
            render_qa_driver_text(output, text, last_compact_title)?;
        }

        if driver.is_complete() {
            break;
        }

        let ui_raw = driver
            .last_ui_json()
            .ok_or_else(|| anyhow::anyhow!("greentic-qa-lib setup payload missing UI state"))?;
        let ui: Value = serde_json::from_str(ui_raw).context("parse greentic-qa-lib UI payload")?;
        let question_id = ui
            .get("next_question_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("greentic-qa-lib UI payload missing next_question_id"))?
            .to_string();
        let question = ui
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|questions| {
                questions.iter().find(|question| {
                    question.get("id").and_then(Value::as_str) == Some(question_id.as_str())
                })
            })
            .ok_or_else(|| {
                anyhow::anyhow!("greentic-qa-lib UI payload missing question {question_id}")
            })?;

        let answer = prompt_qa_question_answer(input, output, &question_id, question)?;
        driver
            .submit_patch_json(&json!({ question_id: answer }).to_string())
            .context("submit greentic-qa-lib setup answer")?;
    }

    let result = driver
        .finish()
        .context("finish greentic-qa-lib setup wizard")?;
    let answers = result
        .answer_set
        .answers
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new);
    Ok(answers.into_iter().collect())
}

#[allow(dead_code)]
fn qa_form_spec_from_setup_form(form: &crate::setup::FormSpec) -> Result<Value> {
    let questions = form
        .questions
        .iter()
        .map(|question| {
            let mut value = json!({
                "id": question.id,
                "type": qa_question_type_name(question.kind),
                "title": question.title,
                "required": question.required,
                "secret": question.secret,
            });
            if let Some(description) = &question.description {
                value["description"] = Value::String(description.clone());
            }
            if !question.choices.is_empty() {
                value["choices"] = Value::Array(
                    question
                        .choices
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                );
            }
            if let Some(default) = &question.default_value
                && let Some(default_value) = qa_default_value(default)
            {
                value["default_value"] = Value::String(default_value);
            }
            value
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "id": form.id,
        "title": form.title,
        "version": form.version,
        "description": form.description,
        "presentation": {
            "default_locale": crate::i18n::current_locale()
        },
        "progress_policy": {
            "skip_answered": true,
            "autofill_defaults": false,
            "treat_default_as_answered": false
        },
        "questions": questions
    }))
}

#[allow(dead_code)]
fn qa_question_type_name(kind: crate::setup::QuestionKind) -> &'static str {
    match kind {
        crate::setup::QuestionKind::String => "string",
        crate::setup::QuestionKind::Number => "number",
        crate::setup::QuestionKind::Boolean => "boolean",
        crate::setup::QuestionKind::Enum => "enum",
    }
}

#[allow(dead_code)]
fn qa_default_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[allow(dead_code)]
fn render_qa_driver_text<W: Write>(
    output: &mut W,
    text: &str,
    last_compact_title: &mut Option<String>,
) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    if let Some(title) = compact_form_title(text) {
        if last_compact_title.as_deref() != Some(title) {
            writeln!(output, "{title}")?;
            output.flush()?;
            *last_compact_title = Some(title.to_string());
        }
        return Ok(());
    }
    *last_compact_title = None;
    for line in text.lines() {
        writeln!(output, "{line}")?;
    }
    if !text.ends_with('\n') {
        output.flush()?;
    }
    Ok(())
}

#[allow(dead_code)]
fn compact_form_title(text: &str) -> Option<&str> {
    let first_line = text.lines().next()?;
    let form = first_line.strip_prefix("Form: ")?;
    let (title, form_id) = form.rsplit_once(" (")?;
    if form_id
        .strip_suffix(')')
        .is_some_and(|id| id.starts_with("greentic-bundle-root-wizard-"))
    {
        return Some(title);
    }
    None
}

#[allow(dead_code)]
fn prompt_qa_question_answer<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    question_id: &str,
    question: &Value,
) -> Result<Value> {
    let title = question
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(question_id);
    let required = question
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let kind = question
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string");
    let secret = question
        .get("secret")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let default_value = question_default_value(question, kind);

    match kind {
        "boolean" => prompt_qa_boolean(input, output, title, required, default_value),
        "enum" => prompt_qa_enum(input, output, title, required, question, default_value),
        _ => prompt_qa_string_like(input, output, title, required, secret, default_value),
    }
}

fn prompt_qa_string_like<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    required: bool,
    secret: bool,
    default_value: Option<Value>,
) -> Result<Value> {
    loop {
        if secret && io::stdin().is_terminal() && io::stdout().is_terminal() {
            let prompt = format!("{title}{}: ", default_suffix(default_value.as_ref()));
            let secret_value =
                rpassword::prompt_password(prompt).context("read secret wizard input")?;
            if secret_value.trim().is_empty() {
                if let Some(default) = &default_value {
                    return Ok(default.clone());
                }
                if required {
                    writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
                    continue;
                }
                return Ok(Value::Null);
            }
            return Ok(Value::String(secret_value));
        }

        write!(
            output,
            "{title}{}: ",
            default_suffix(default_value.as_ref())
        )?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(default) = &default_value {
                return Ok(default.clone());
            }
            if required {
                writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
                continue;
            }
            return Ok(Value::Null);
        }
        return Ok(Value::String(trimmed.to_string()));
    }
}

#[allow(dead_code)]
fn prompt_qa_boolean<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    required: bool,
    default_value: Option<Value>,
) -> Result<Value> {
    loop {
        write!(
            output,
            "{title}{}: ",
            default_suffix(default_value.as_ref())
        )?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
        }
        let trimmed = line.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            if let Some(default) = &default_value {
                return Ok(default.clone());
            }
            if required {
                writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
                continue;
            }
            return Ok(Value::Null);
        }
        match parse_localized_boolean(&trimmed) {
            Some(value) => return Ok(Value::Bool(value)),
            None => writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?,
        }
    }
}

#[allow(dead_code)]
fn parse_localized_boolean(input: &str) -> Option<bool> {
    let trimmed = input.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }

    let locale = crate::i18n::current_locale();
    let mut truthy = vec!["true", "t", "yes", "y", "1"];
    let mut falsy = vec!["false", "f", "no", "n", "0"];

    match crate::i18n::base_language(&locale).as_deref() {
        Some("nl") => {
            truthy.extend(["ja", "j"]);
            falsy.extend(["nee"]);
        }
        Some("de") => {
            truthy.extend(["ja", "j"]);
            falsy.extend(["nein"]);
        }
        Some("fr") => {
            truthy.extend(["oui", "o"]);
            falsy.extend(["non"]);
        }
        Some("es") | Some("pt") | Some("it") => {
            truthy.extend(["si", "s"]);
            falsy.extend(["no"]);
        }
        _ => {}
    }

    if truthy.iter().any(|value| *value == trimmed) {
        return Some(true);
    }
    if falsy.iter().any(|value| *value == trimmed) {
        return Some(false);
    }
    None
}

#[allow(dead_code)]
fn prompt_qa_enum<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    title: &str,
    required: bool,
    question: &Value,
    default_value: Option<Value>,
) -> Result<Value> {
    let choices = question
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("qa enum question missing choices"))?
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    loop {
        if !title.is_empty() {
            writeln!(output, "{title}:")?;
        }
        for (index, choice) in choices.iter().enumerate() {
            if title.is_empty() {
                writeln!(output, "{}. {}", index + 1, choice)?;
            } else {
                writeln!(output, "  {}. {}", index + 1, choice)?;
            }
        }
        write!(output, "{} ", crate::i18n::tr("wizard.setup.enum_prompt"))?;
        output.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            bail!("{}", crate::i18n::tr("wizard.error.input_ended"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(default) = &default_value {
                return Ok(default.clone());
            }
            if required {
                writeln!(output, "{}", crate::i18n::tr("wizard.error.empty_answer"))?;
                continue;
            }
            return Ok(Value::Null);
        }
        if let Ok(number) = trimmed.parse::<usize>()
            && number > 0
            && number <= choices.len()
        {
            return Ok(Value::String(choices[number - 1].clone()));
        }
        if choices.iter().any(|choice| choice == trimmed) {
            return Ok(Value::String(trimmed.to_string()));
        }
        writeln!(output, "{}", crate::i18n::tr("wizard.error.invalid_choice"))?;
    }
}

#[allow(dead_code)]
fn question_default_value(question: &Value, kind: &str) -> Option<Value> {
    let raw = question
        .get("current_value")
        .cloned()
        .or_else(|| question.get("default").cloned())?;
    match raw {
        Value::String(text) => match kind {
            "boolean" => match text.as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            "number" => serde_json::from_str::<serde_json::Number>(&text)
                .ok()
                .map(Value::Number),
            _ => Some(Value::String(text)),
        },
        Value::Bool(flag) if kind == "boolean" => Some(Value::Bool(flag)),
        Value::Number(number) if kind == "number" => Some(Value::Number(number)),
        Value::Null => None,
        other => Some(other),
    }
}

fn default_suffix(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) if !text.is_empty() => format!(" [{}]", text),
        Some(Value::Bool(flag)) => format!(" [{}]", flag),
        Some(Value::Number(number)) => format!(" [{}]", number),
        _ => String::new(),
    }
}

fn discover_setup_specs(
    mut request: NormalizedRequest,
    catalog_resolution: &crate::catalog::resolve::CatalogResolution,
) -> NormalizedRequest {
    if !request.setup_execution_intent {
        return request;
    }

    for reference in request
        .extension_providers
        .iter()
        .chain(request.app_packs.iter())
    {
        if request.setup_specs.contains_key(reference) {
            continue;
        }
        if let Some(entry) = catalog_resolution
            .discovered_items
            .iter()
            .find(|entry| entry.id == *reference || entry.reference == *reference)
            && let Some(setup) = &entry.setup
        {
            request
                .setup_specs
                .entry(entry.id.clone())
                .or_insert_with(|| serde_json::to_value(setup).expect("serialize setup metadata"));

            if let Some(answer_value) = request.setup_answers.remove(reference) {
                request
                    .setup_answers
                    .entry(entry.id.clone())
                    .or_insert(answer_value);
            }
        }
    }

    request
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::catalog::registry::CatalogEntry;

    use super::{
        RootMenuZeroAction, apply_cloud_deployer_target_to_entries,
        build_extension_provider_options, choose_interactive_menu, clean_extension_provider_label,
        detect_cloud_deployer_target, detected_reference_kind,
    };

    #[test]
    fn root_menu_shows_back_and_returns_none_for_embedded_wizards() {
        crate::i18n::init(Some("en".to_string()));
        let mut input = Cursor::new(b"0\n");
        let mut output = Vec::new();

        let choice = choose_interactive_menu(&mut input, &mut output, RootMenuZeroAction::Back)
            .expect("menu should render");

        assert_eq!(choice, None);
        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("0. Back"));
        assert!(!rendered.contains("0. Exit"));
    }

    #[test]
    fn extension_provider_options_dedupe_by_display_name() {
        let pinned = CatalogEntry {
            id: "greentic.secrets.aws-sm.v0-4-25".to_string(),
            category: Some("secrets".to_string()),
            category_label: None,
            category_description: None,
            label: Some("Greentic Secrets AWS SM".to_string()),
            reference:
                "oci://ghcr.io/greenticai/packs/secret/greentic.secrets.aws-sm.gtpack:0.4.25"
                    .to_string(),
            setup: None,
        };
        let latest = CatalogEntry {
            id: "greentic.secrets.aws-sm.latest".to_string(),
            category: Some("secrets".to_string()),
            category_label: None,
            category_description: None,
            label: Some("Greentic Secrets AWS SM".to_string()),
            reference:
                "oci://ghcr.io/greenticai/packs/secret/greentic.secrets.aws-sm.gtpack:stable"
                    .to_string(),
            setup: None,
        };
        let entries = vec![&pinned, &latest];
        let options = build_extension_provider_options(&entries);

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].display_name, "Greentic Secrets AWS SM");
        assert_eq!(options[0].entry.id, "greentic.secrets.aws-sm.v0-4-25");
        assert_eq!(
            options[0].entry.reference,
            "oci://ghcr.io/greenticai/packs/secret/greentic.secrets.aws-sm.gtpack:0.4.25"
        );
    }

    #[test]
    fn clean_extension_provider_label_removes_latest_suffix_only() {
        let latest = CatalogEntry {
            id: "x.latest".to_string(),
            category: None,
            category_label: None,
            category_description: None,
            label: Some("Greentic Secrets AWS SM (latest)".to_string()),
            reference: "oci://ghcr.io/example/secrets:stable".to_string(),
            setup: None,
        };
        let semver = CatalogEntry {
            id: "x.0.4.25".to_string(),
            category: None,
            category_label: None,
            category_description: None,
            label: Some("Greentic Secrets AWS SM (0.4.25)".to_string()),
            reference: "oci://ghcr.io/example/secrets:0.4.25".to_string(),
            setup: None,
        };
        let pr = CatalogEntry {
            id: "x.pr".to_string(),
            category: None,
            category_label: None,
            category_description: None,
            label: Some("Greentic Messaging Dummy (PR version)".to_string()),
            reference: "oci://ghcr.io/example/messaging:<pr-version>".to_string(),
            setup: None,
        };

        assert_eq!(
            clean_extension_provider_label(&latest),
            "Greentic Secrets AWS SM"
        );
        assert_eq!(
            clean_extension_provider_label(&semver),
            "Greentic Secrets AWS SM (0.4.25)"
        );
        assert_eq!(
            clean_extension_provider_label(&pr),
            "Greentic Messaging Dummy (PR version)"
        );
    }

    #[test]
    fn detected_reference_kind_classifies_https_refs() {
        let root = std::path::Path::new(".");
        assert_eq!(
            detected_reference_kind(root, "https://example.com/packs/cards-demo.gtpack"),
            "https"
        );
    }

    #[test]
    fn cloud_deployer_target_adds_provider_entry() {
        let mut entries = Vec::new();
        apply_cloud_deployer_target_to_entries(&mut entries, "aws");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider_id, "deployer-aws");
        assert_eq!(
            entries[0].reference,
            "oci://ghcr.io/greenticai/packs/deployer/greentic.deploy.aws:stable"
        );
    }

    #[test]
    fn cloud_deployer_target_replaces_existing_cloud_deployer_entry() {
        let mut entries = Vec::new();
        apply_cloud_deployer_target_to_entries(&mut entries, "aws");
        apply_cloud_deployer_target_to_entries(&mut entries, "gcp");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider_id, "deployer-gcp");
        assert_eq!(
            detect_cloud_deployer_target(&[entries[0].reference.clone()]),
            Some("gcp".to_string())
        );
    }
}
