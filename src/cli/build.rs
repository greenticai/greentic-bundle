use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct BuildArgs {
    #[arg(long, default_value = ".", help = "cli.build.root.option")]
    pub root: PathBuf,

    #[arg(long, value_name = "FILE", help = "cli.build.output.option")]
    pub output: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "cli.option.dry_run")]
    pub dry_run: bool,

    /// Embed a precompiled component cache (`.cache/v1/...`) into the bundle.
    /// Requires `greentic-start` on PATH; bigger artifact, faster cold start.
    #[arg(long, default_value_t = false)]
    pub warmup: bool,

    #[command(flatten)]
    pub signing: SigningArgs,
}

/// CLI flags for DSSE+Ed25519 artifact signing (C2). When `--signing-key` is
/// passed, a `<artifact>.sig` sidecar is written next to the `.gtbundle`.
#[derive(Debug, Default, Args, Clone)]
pub struct SigningArgs {
    /// Path to an Ed25519 PKCS#8 PEM private key. When set, signs the
    /// `.gtbundle` artifact and writes the DSSE envelope sidecar.
    #[arg(long, value_name = "FILE")]
    pub signing_key: Option<PathBuf>,

    /// Explicit DSSE `keyid`. Default: derived from a sibling `<key>.pub`
    /// SPKI PEM (hex of `SHA-256(raw 32-byte public key)[..16]`).
    #[arg(long, value_name = "HEX", requires = "signing_key")]
    pub key_id: Option<String>,

    /// SLSA `builder.id` recorded in the provenance predicate.
    /// Default: `greentic-bundle:<package version>`.
    #[arg(long, value_name = "ID", requires = "signing_key")]
    pub builder_id: Option<String>,

    /// Override of the signature sidecar path. Default: `<artifact>.sig`.
    #[arg(long, value_name = "FILE", requires = "signing_key")]
    pub signature_output: Option<PathBuf>,
}

impl SigningArgs {
    /// Build a `SigningConfig` when `--signing-key` was provided.
    pub fn to_config(&self) -> Option<crate::build::signing::SigningConfig> {
        self.signing_key
            .as_ref()
            .map(|path| crate::build::signing::SigningConfig {
                signing_key_path: path.clone(),
                key_id_override: self.key_id.clone(),
                builder_id: self.builder_id.clone(),
                signature_path_override: self.signature_output.clone(),
            })
    }
}

impl Default for BuildArgs {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            output: None,
            dry_run: false,
            warmup: false,
            signing: SigningArgs::default(),
        }
    }
}

pub fn run(args: BuildArgs) -> Result<()> {
    let signing = args.signing.to_config();
    let result = crate::build::build_workspace(
        &args.root,
        args.output.as_deref(),
        args.dry_run,
        args.warmup,
        signing.as_ref(),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
