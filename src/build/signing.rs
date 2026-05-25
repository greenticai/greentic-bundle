//! C2 — DSSE+Ed25519 artifact signer for `.gtbundle` outputs.
//!
//! Reuses the cryptographic primitive in
//! [`greentic_distributor_client::signing`] and emits a single-signature DSSE
//! envelope alongside the artifact as `<artifact>.sig`. The envelope wraps an
//! in-toto Statement v1 whose subject pins the artifact's SHA-256 digest and
//! whose predicate is a minimal SLSA-provenance v1 document.
//!
//! Phase B scope: signature *authenticity* only. KMS-backed keys, Rekor
//! transparency-log submission, and full provenance materials belong to the
//! Trust plan.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use greentic_distributor_client::signing::{
    InTotoStatement, SlsaProvenance, TrustRoot, TrustedKey, key_id_for_public_key_pem,
    sign_statement, verify_artifact_dsse,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Configuration for the bundle signer.
#[derive(Debug, Clone)]
pub struct SigningConfig {
    /// Path to the Ed25519 PKCS#8 PEM private key.
    pub signing_key_path: PathBuf,
    /// Optional explicit DSSE `keyid`. When set, it must match the canonical
    /// key id derived from the private key — a mismatch is rejected before any
    /// build output is written.
    pub key_id_override: Option<String>,
    /// SLSA `builder.id`. Defaults to `greentic-bundle:<package version>`.
    pub builder_id: Option<String>,
    /// Override of the output signature path. Default: `<artifact>.sig`.
    /// Rejected if it resolves to the artifact path.
    pub signature_path_override: Option<PathBuf>,
}

/// SLSA `build_type` discriminator for a `.gtbundle` artifact.
pub const BUNDLE_BUILD_TYPE: &str = "gtbundle";

const SIGNATURE_SUFFIX: &str = ".sig";
const PUBLIC_KEY_SUFFIX: &str = ".pub";
const STAGING_SUFFIX: &str = ".partial";
const HASH_CHUNK_BYTES: usize = 64 * 1024;

/// A validated signing context: a parsed private key and a resolved sidecar
/// path that has been proven distinct from the artifact. Constructed once via
/// [`PreparedSigner::prepare`] before any build output is written so that
/// signing-configuration errors abort the build *before* the `.gtbundle` lands
/// on disk (closes Codex finding #3).
///
/// `private_pem` is held in [`Zeroizing<String>`] so the key bytes are wiped
/// from heap memory on drop. The manual [`std::fmt::Debug`] impl redacts the
/// PEM so it can never leak via `{:?}` formatting (tracing, panic messages,
/// anyhow context chains).
pub struct PreparedSigner {
    pub sig_path: PathBuf,
    private_pem: Zeroizing<String>,
    canonical_key_id: String,
    canonical_public_pem: String,
    builder_id: String,
}

impl std::fmt::Debug for PreparedSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSigner")
            .field("sig_path", &self.sig_path)
            .field("private_pem", &"[REDACTED]")
            .field("canonical_key_id", &self.canonical_key_id)
            .field("canonical_public_pem_len", &self.canonical_public_pem.len())
            .field("builder_id", &self.builder_id)
            .finish()
    }
}

impl PreparedSigner {
    /// Validate the signing config against the intended artifact path. Performs:
    /// 1. Reject non-UTF-8 artifact paths — DSSE subject names must round-trip
    ///    losslessly through `to_str()` (closes review finding #8).
    /// 2. Read + parse the PKCS#8 PEM, derive the canonical `(key_id, public_pem)`
    ///    from the private key's verifying key — the only authoritative source.
    /// 3. If `--key-id` override is given, require it to match the canonical id
    ///    (case-insensitive hex).
    /// 4. If a `<key>.pub` sibling exists, require its derived id to match the
    ///    canonical id — a stale sibling after key rotation is a hard error.
    ///    Read-without-exists to avoid a TOCTOU race (closes review finding #7).
    /// 5. Resolve the signature output path and reject collisions with the
    ///    artifact (lexical, lexically-normalized-absolute, parent-canonicalize,
    ///    and full-canonicalize layers — closes review findings #1 and #2).
    pub fn prepare(artifact: &Path, cfg: &SigningConfig) -> Result<Self> {
        if artifact.to_str().is_none() {
            bail!(
                "artifact path {} is not valid UTF-8; DSSE subject names must round-trip losslessly",
                artifact.display()
            );
        }

        let private_pem =
            Zeroizing::new(fs::read_to_string(&cfg.signing_key_path).with_context(|| {
                format!("read signing key: {}", cfg.signing_key_path.display())
            })?);
        let signing_key = SigningKey::from_pkcs8_pem(&private_pem).with_context(|| {
            format!(
                "parse PKCS#8 PEM private key: {}",
                cfg.signing_key_path.display()
            )
        })?;
        let public_pem = signing_key
            .verifying_key()
            .to_public_key_pem(LineEnding::LF)
            .context("encode SPKI public key PEM")?;
        // SigningKey carries an in-memory copy of the private scalar; drop it
        // here so only the Zeroizing<String> PEM lingers until signing runs.
        drop(signing_key);
        let canonical_key_id = key_id_for_public_key_pem(&public_pem)
            .map_err(|e| anyhow!("derive canonical key id: {e}"))?;

        if let Some(override_) = cfg.key_id_override.as_deref() {
            if override_.is_empty() {
                bail!("--key-id may not be empty");
            }
            if !override_.eq_ignore_ascii_case(&canonical_key_id) {
                bail!(
                    "--key-id {override_} does not match the private key (canonical id: {canonical_key_id})"
                );
            }
        }

        let pub_sibling = append_suffix(&cfg.signing_key_path, PUBLIC_KEY_SUFFIX);
        match fs::read_to_string(&pub_sibling) {
            Ok(sibling_pem) => {
                let sibling_id = key_id_for_public_key_pem(&sibling_pem)
                    .map_err(|e| anyhow!("derive id from {}: {e}", pub_sibling.display()))?;
                if !sibling_id.eq_ignore_ascii_case(&canonical_key_id) {
                    bail!(
                        "public key sibling {} does not match the private key (pub id={sibling_id} priv id={canonical_key_id}); a stale .pub after key rotation will silently break verification",
                        pub_sibling.display()
                    );
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // No sibling — canonical key id derived from private key is
                // authoritative.
            }
            Err(e) => {
                return Err(anyhow::Error::new(e).context(format!(
                    "read public key sibling: {}",
                    pub_sibling.display()
                )));
            }
        }

        let sig_path = cfg
            .signature_path_override
            .clone()
            .unwrap_or_else(|| default_signature_path(artifact));
        reject_signature_artifact_collision(&sig_path, artifact)?;

        Ok(Self {
            sig_path,
            private_pem,
            canonical_key_id,
            canonical_public_pem: public_pem,
            builder_id: cfg.builder_id.clone().unwrap_or_else(default_builder_id),
        })
    }

    fn build_envelope_json(&self, digest_hex: &str, artifact_name: &str) -> Result<Vec<u8>> {
        let predicate = SlsaProvenance {
            builder_id: self.builder_id.clone(),
            build_type: BUNDLE_BUILD_TYPE.to_string(),
            built_at: None,
            tlog_entry_id: None,
        };
        let statement = InTotoStatement::provenance(artifact_name, digest_hex, predicate);
        let envelope = sign_statement(&statement, &self.private_pem, &self.canonical_key_id)
            .map_err(|e| anyhow!("sign in-toto statement: {e}"))?;
        let envelope_json =
            serde_json::to_vec_pretty(&envelope).context("serialize DSSE envelope")?;

        // Self-verify before publishing — defense in depth against a
        // PKCS#8/key-id binding bug (closes Codex finding #2 recommendation).
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: self.canonical_key_id.clone(),
            public_key_pem: self.canonical_public_pem.clone(),
        }]);
        verify_artifact_dsse(&envelope_json, digest_hex, &trust)
            .map_err(|e| anyhow!("self-verify of emitted envelope failed: {e}"))?;

        Ok(envelope_json)
    }
}

/// Atomically write a signed bundle: stage the artifact + sidecar adjacent to
/// their final paths, self-verify, then rename both into place. On any error,
/// best-effort remove both staged files so a release job collecting dist
/// outputs after a failed step never sees an unsigned `.gtbundle` masquerading
/// as a candidate (closes Codex finding #3).
///
/// **Failure semantics (fail-closed):** if the artifact rename succeeds but
/// the sidecar rename fails, the just-published artifact is removed so the
/// "never publish unsigned" invariant holds. The user must rebuild — the prior
/// valid build is unrecoverable. This is intentional: a downstream automation
/// that picks up `.gtbundle` files and never checks for `.sig` would otherwise
/// ship unsigned outputs.
///
/// `write_artifact` is called with the *staged* artifact path; callers pass
/// `bundle_fs::write_bundle(build_dir, ...)` here.
pub fn stage_sign_and_publish<F>(
    artifact: &Path,
    signer: &PreparedSigner,
    write_artifact: F,
) -> Result<PathBuf>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let staged_artifact = append_suffix(artifact, STAGING_SUFFIX);
    let staged_sig = append_suffix(&signer.sig_path, STAGING_SUFFIX);

    // Pre-clean any prior staging cruft so a partial leftover from an earlier
    // crash doesn't masquerade as the current run's output.
    let _ = fs::remove_file(&staged_artifact);
    let _ = fs::remove_file(&staged_sig);

    let outcome = (|| -> Result<()> {
        if let Some(parent) = signer.sig_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("create signature parent dir: {}", parent.display()))?;
        }
        write_artifact(&staged_artifact)?;
        let digest_hex = hash_file(&staged_artifact)?;
        let artifact_name = artifact
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                anyhow!(
                    "artifact path {} has no UTF-8 file name component",
                    artifact.display()
                )
            })?;
        let envelope_json = signer.build_envelope_json(&digest_hex, &artifact_name)?;
        fs::write(&staged_sig, &envelope_json)
            .with_context(|| format!("write staged sidecar: {}", staged_sig.display()))?;
        Ok(())
    })();

    if let Err(e) = outcome {
        let _ = fs::remove_file(&staged_artifact);
        let _ = fs::remove_file(&staged_sig);
        return Err(e);
    }

    // Same filesystem, sibling paths → POSIX rename is atomic.
    fs::rename(&staged_artifact, artifact).with_context(|| {
        format!(
            "rename staged artifact {} -> {}",
            staged_artifact.display(),
            artifact.display()
        )
    })?;
    if let Err(e) = fs::rename(&staged_sig, &signer.sig_path) {
        // Sidecar rename failed AFTER the artifact rename — under the
        // fail-closed invariant we remove the artifact too so no unsigned
        // `.gtbundle` escapes downstream. The successful build is lost; the
        // user must rebuild. We surface this explicitly so the choice is
        // visible in the error chain rather than silent data loss.
        let _ = fs::remove_file(artifact);
        let _ = fs::remove_file(&staged_sig);
        return Err(anyhow::Error::new(e).context(format!(
            "publish staged sidecar to {} failed; under the fail-closed invariant the artifact at {} was also removed — rerun build to publish a signed bundle",
            signer.sig_path.display(),
            artifact.display()
        )));
    }
    Ok(signer.sig_path.clone())
}

/// Streaming SHA-256 of `path` — chunked read so large bundles (multi-GB with
/// warmup cache) don't allocate a whole-file `Vec<u8>` for hashing (closes
/// review finding #4). Returns the lowercase hex digest.
fn hash_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("open for hashing: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("read for hashing: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Default sidecar path: append `.sig` to the artifact path.
pub fn default_signature_path(artifact: &Path) -> PathBuf {
    append_suffix(artifact, SIGNATURE_SUFFIX)
}

/// Reject a signature output path that resolves to the artifact path. Closes
/// review findings #1 and #2 in addition to the original Codex finding #1.
/// Four layers, applied in order:
/// 1. Lexical equality (cheap path).
/// 2. `std::path::absolute` + lexical-normalize (collapses `.`/`..` components
///    — catches `--signature-output /out/sub/../foo.gtbundle` aliasing the
///    artifact).
/// 3. Parent-canonicalize + file_name equality (catches symlinked parent
///    directories even when the files themselves don't exist at prepare-time).
/// 4. Full canonicalize when both files exist (catches symlinked files
///    pointing at each other).
fn reject_signature_artifact_collision(sig_path: &Path, artifact: &Path) -> Result<()> {
    fn bail_collision(sig: &Path, art: &Path, layer: &str) -> Result<()> {
        bail!(
            "signature output path {} {} the artifact path {}; refusing to overwrite the bundle with its own envelope",
            sig.display(),
            layer,
            art.display(),
        )
    }

    if sig_path == artifact {
        return bail_collision(sig_path, artifact, "equals");
    }

    let sig_abs = std::path::absolute(sig_path).unwrap_or_else(|_| sig_path.to_path_buf());
    let art_abs = std::path::absolute(artifact).unwrap_or_else(|_| artifact.to_path_buf());
    if normalize_lexically(&sig_abs) == normalize_lexically(&art_abs) {
        return bail_collision(sig_path, artifact, "resolves to");
    }

    if let (Some(sp), Some(ap), Some(sn), Some(an)) = (
        sig_path.parent(),
        artifact.parent(),
        sig_path.file_name(),
        artifact.file_name(),
    ) && sn == an
    {
        let parent_match = match (sp.canonicalize(), ap.canonicalize()) {
            (Ok(s), Ok(a)) => s == a,
            _ => false,
        };
        if parent_match {
            return bail_collision(sig_path, artifact, "shares a canonical parent dir with");
        }
    }

    if sig_path.exists()
        && artifact.exists()
        && let (Ok(s), Ok(a)) = (sig_path.canonicalize(), artifact.canonicalize())
        && s == a
    {
        return bail_collision(sig_path, artifact, "canonicalizes to (symlink collision)");
    }

    Ok(())
}

/// Collapse `.` and `..` components from an absolute path without touching the
/// filesystem. Used by [`reject_signature_artifact_collision`] to compare two
/// paths that may differ only in redundant `..`/`.` segments.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// SLSA `builder.id` default. Captures the *library* version
/// (`greentic-bundle:<version>`) at compile time, not the calling CLI's
/// identity (e.g. `gtc`). Top-level binaries that embed this signer should
/// pass `--builder-id` to record their own identity into the provenance
/// predicate.
fn default_builder_id() -> String {
    format!("greentic-bundle:{}", env!("CARGO_PKG_VERSION"))
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    debug_assert!(
        !path.as_os_str().is_empty(),
        "append_suffix called on empty path"
    );
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use tempfile::tempdir;

    fn ephemeral_keypair(seed: u8) -> (String, String) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let vk = sk.verifying_key();
        let priv_pem = sk.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
        let pub_pem = vk.to_public_key_pem(LineEnding::LF).unwrap();
        (priv_pem, pub_pem)
    }

    fn write_key(dir: &Path, name: &str, pem: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, pem).unwrap();
        p
    }

    #[test]
    fn default_signature_path_appends_sig() {
        let p = Path::new("/tmp/dist/example.gtbundle");
        assert_eq!(
            default_signature_path(p),
            PathBuf::from("/tmp/dist/example.gtbundle.sig")
        );
    }

    #[test]
    fn prepare_derives_key_id_from_private_pem_without_pub_sibling() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(31);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();
        assert_eq!(
            signer.canonical_key_id,
            key_id_for_public_key_pem(&pub_pem).unwrap()
        );
        assert_eq!(signer.sig_path, default_signature_path(&artifact));
    }

    #[test]
    fn prepare_rejects_key_id_override_that_doesnt_match_private_key() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(32);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some("deadbeefdeadbeefdeadbeefdeadbeef".into()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("mismatched --key-id must be rejected");
        assert!(
            format!("{err:#}").contains("does not match"),
            "got: {err:#}"
        );
    }

    #[test]
    fn prepare_rejects_stale_pub_sibling_after_key_rotation() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(33);
        let (_priv_other, pub_other) = ephemeral_keypair(34);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        fs::write(append_suffix(&key_path, PUBLIC_KEY_SUFFIX), &pub_other).unwrap();
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("stale .pub must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("stale .pub") || msg.contains("does not match"),
            "got: {msg}"
        );
    }

    #[test]
    fn prepare_rejects_empty_key_id_override() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(35);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(String::new()),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("empty override must be rejected");
        assert!(format!("{err:#}").contains("--key-id"));
    }

    #[test]
    fn prepare_rejects_signature_path_equal_to_artifact() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(36);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(artifact.clone()),
            },
        )
        .expect_err("identical paths must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("refusing to overwrite"), "got: {msg}");
    }

    #[test]
    fn prepare_rejects_signature_path_resolving_to_artifact_via_relative() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(37);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let alias = dir.path().join(".").join("a.gtbundle");
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(alias),
            },
        )
        .expect_err("path alias to artifact must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    // Review finding #1: `..` components must be lexically collapsed so a
    // path like `/out/sub/../demo.gtbundle` is recognised as aliasing
    // `/out/demo.gtbundle`. Neither file exists at prepare-time so the
    // canonicalize layer is skipped; the normalize_lexically layer catches
    // it.
    #[test]
    fn prepare_rejects_signature_path_via_parent_dir_dotdot() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("dist")).unwrap();
        let artifact = dir.path().join("dist").join("demo.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(50);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        // Sibling subdir need not exist on disk — lexical normalize collapses
        // `sub/..` regardless.
        let aliased = dir
            .path()
            .join("dist")
            .join("sub")
            .join("..")
            .join("demo.gtbundle");
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(aliased),
            },
        )
        .expect_err("..-alias must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    // Review finding #2: when --signature-output's parent canonicalizes to
    // the artifact's parent (symlink), and both file_names match, this is a
    // collision even when neither file exists yet.
    #[cfg(unix)]
    #[test]
    fn prepare_rejects_signature_path_via_symlinked_parent_dir() {
        let dir = tempdir().unwrap();
        let real_parent = dir.path().join("real-dist");
        fs::create_dir_all(&real_parent).unwrap();
        let link_parent = dir.path().join("current-dist");
        std::os::unix::fs::symlink(&real_parent, &link_parent).unwrap();

        let artifact = real_parent.join("demo.gtbundle");
        let aliased_sig = link_parent.join("demo.gtbundle");
        // Neither file exists yet.
        assert!(!artifact.exists());
        assert!(!aliased_sig.exists());

        let (priv_pem, _pub_pem) = ephemeral_keypair(51);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(aliased_sig),
            },
        )
        .expect_err("symlinked-parent alias must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    #[cfg(unix)]
    #[test]
    fn prepare_rejects_signature_path_via_symlink_to_artifact() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        fs::write(&artifact, b"existing-bundle").unwrap();
        let (priv_pem, _pub_pem) = ephemeral_keypair(38);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let link = dir.path().join("alias.sig");
        std::os::unix::fs::symlink(&artifact, &link).unwrap();
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(link),
            },
        )
        .expect_err("symlink to artifact must be rejected");
        assert!(format!("{err:#}").contains("refusing to overwrite"));
    }

    // Review finding #3: Debug output must not leak the private key PEM.
    #[test]
    fn debug_format_redacts_private_pem() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(52);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();
        let dbg = format!("{signer:?}");
        assert!(
            !dbg.contains("BEGIN PRIVATE KEY") && !dbg.contains("BEGIN ED25519"),
            "Debug leaked PEM: {dbg}"
        );
        assert!(dbg.contains("[REDACTED]"));
        assert!(dbg.contains(&signer.canonical_key_id));
    }

    // Review finding #4: hashing must stream, not slurp the whole artifact
    // into memory.
    #[test]
    fn hash_file_streams_chunks_and_matches_oneshot_digest() {
        let dir = tempdir().unwrap();
        // Just over two chunks of HASH_CHUNK_BYTES to exercise the read loop.
        let bytes = vec![0xABu8; HASH_CHUNK_BYTES * 2 + 7];
        let path = dir.path().join("big.bin");
        fs::write(&path, &bytes).unwrap();
        let streamed = hash_file(&path).unwrap();
        let oneshot = hex::encode(Sha256::digest(&bytes));
        assert_eq!(streamed, oneshot);
    }

    // Review finding #8: non-UTF-8 artifact paths are rejected at prepare()
    // time so they never reach the DSSE statement subject.
    #[cfg(unix)]
    #[test]
    fn prepare_rejects_non_utf8_artifact_path() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let dir = tempdir().unwrap();
        let invalid_name = OsStr::from_bytes(b"bad-\xff-name.gtbundle");
        let artifact = dir.path().join(invalid_name);
        let (priv_pem, _pub_pem) = ephemeral_keypair(53);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let err = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect_err("non-UTF-8 artifact path must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("not valid UTF-8"), "got: {msg}");
    }

    #[test]
    fn stage_sign_and_publish_emits_verifiable_sidecar() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(39);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        fs::write(append_suffix(&key_path, PUBLIC_KEY_SUFFIX), &pub_pem).unwrap();
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: Some("greentic-bundle:test".into()),
                signature_path_override: None,
            },
        )
        .unwrap();

        let sig_path = stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"squashfs-bytes")?;
            Ok(())
        })
        .expect("stage+sign");
        assert_eq!(sig_path, default_signature_path(&artifact));
        assert!(artifact.exists());
        assert!(sig_path.exists());
        assert!(!append_suffix(&artifact, STAGING_SUFFIX).exists());
        assert!(!append_suffix(&sig_path, STAGING_SUFFIX).exists());

        let envelope_bytes = fs::read(&sig_path).unwrap();
        let key_id = key_id_for_public_key_pem(&pub_pem).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: key_id.clone(),
            public_key_pem: pub_pem,
        }]);
        let expected_digest = hash_file(&artifact).unwrap();
        let verified = verify_artifact_dsse(&envelope_bytes, &expected_digest, &trust).unwrap();
        assert_eq!(verified.verified_key_ids, vec![key_id]);
    }

    // Review finding #5: when the sidecar rename fails AFTER the artifact has
    // been published, the artifact is removed under the fail-closed
    // invariant. The error chain must say so explicitly so the build's
    // disappearance is not silent.
    //
    // Forcing only the SECOND rename to fail is tricky on POSIX — read-only
    // directories trip the staged-sig write earlier. We instead pre-create
    // sig_path as a non-empty directory: the staged sidecar write goes to
    // `sig_path.partial` (a sibling file), which succeeds; the final
    // `fs::rename(file, dir)` then fails with EISDIR.
    #[cfg(unix)]
    #[test]
    fn stage_sign_and_publish_surfaces_fail_closed_artifact_removal() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let sig_dir = dir.path().join("sigs");
        fs::create_dir_all(&sig_dir).unwrap();
        let sig_path = sig_dir.join("hello.gtbundle.sig");
        // Make sig_path a non-empty directory so rename(file, dir) fails.
        fs::create_dir_all(&sig_path).unwrap();
        fs::write(sig_path.join("placeholder"), b"x").unwrap();

        let (priv_pem, _pub_pem) = ephemeral_keypair(54);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(sig_path.clone()),
            },
        )
        .unwrap();

        let err = stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"bytes")?;
            Ok(())
        })
        .expect_err("sig rename should fail because sig_path is a non-empty directory");

        let msg = format!("{err:#}");
        assert!(
            msg.contains("fail-closed") && msg.contains("was also removed"),
            "error must surface fail-closed removal, got: {msg}"
        );
        assert!(
            !artifact.exists(),
            "artifact must be removed under fail-closed"
        );
        // sig_path (the directory) still exists — we only remove_file the
        // staged sidecar, not the pre-existing directory.
        assert!(sig_path.is_dir());
    }

    #[test]
    fn stage_sign_and_publish_leaves_no_artifact_when_write_step_fails() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(40);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();

        let err = stage_sign_and_publish(&artifact, &signer, |_staged| {
            anyhow::bail!("simulated write_bundle failure")
        })
        .expect_err("must propagate write failure");
        assert!(format!("{err:#}").contains("simulated"));
        assert!(!artifact.exists());
        assert!(!signer.sig_path.exists());
        assert!(!append_suffix(&artifact, STAGING_SUFFIX).exists());
        assert!(!append_suffix(&signer.sig_path, STAGING_SUFFIX).exists());
    }

    #[test]
    fn stage_sign_and_publish_cleans_up_prior_partial_leftovers() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("hello.gtbundle");
        let (priv_pem, _pub_pem) = ephemeral_keypair(41);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: None,
            },
        )
        .unwrap();
        fs::write(append_suffix(&artifact, STAGING_SUFFIX), b"stale-art").unwrap();
        fs::write(
            append_suffix(&signer.sig_path, STAGING_SUFFIX),
            b"stale-sig",
        )
        .unwrap();
        stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"fresh-bytes")?;
            Ok(())
        })
        .expect("stage+sign");
        assert_eq!(fs::read(&artifact).unwrap(), b"fresh-bytes");
    }

    // Replacement for the dropped happy-path coverage of --signature-output:
    // a non-default, non-colliding custom sig path is honored, parent dir is
    // created if absent, and the envelope verifies.
    #[test]
    fn custom_signature_output_path_creates_parent_dir() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(55);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let custom = dir.path().join("sigs").join("custom.json");
        assert!(!custom.parent().unwrap().exists());

        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: None,
                builder_id: None,
                signature_path_override: Some(custom.clone()),
            },
        )
        .unwrap();
        stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"x")?;
            Ok(())
        })
        .expect("sign");

        assert!(custom.exists());
        assert!(custom.parent().unwrap().exists());
        let envelope_bytes = fs::read(&custom).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: key_id_for_public_key_pem(&pub_pem).unwrap(),
            public_key_pem: pub_pem,
        }]);
        let digest = hash_file(&artifact).unwrap();
        verify_artifact_dsse(&envelope_bytes, &digest, &trust).expect("verify");
    }

    // Replacement for the dropped --key-id happy-path coverage: a matching
    // (uppercase) override is accepted and produces a verifiable envelope.
    #[test]
    fn key_id_override_matching_uppercase_is_accepted() {
        let dir = tempdir().unwrap();
        let artifact = dir.path().join("a.gtbundle");
        let (priv_pem, pub_pem) = ephemeral_keypair(56);
        let key_path = write_key(dir.path(), "k.pem", &priv_pem);
        let derived = key_id_for_public_key_pem(&pub_pem).unwrap();
        let upper = derived.to_ascii_uppercase();

        let signer = PreparedSigner::prepare(
            &artifact,
            &SigningConfig {
                signing_key_path: key_path,
                key_id_override: Some(upper),
                builder_id: None,
                signature_path_override: None,
            },
        )
        .expect("uppercase matching --key-id must be accepted");
        stage_sign_and_publish(&artifact, &signer, |staged| {
            fs::write(staged, b"x")?;
            Ok(())
        })
        .expect("sign");
        let envelope_bytes = fs::read(&signer.sig_path).unwrap();
        let trust = TrustRoot::new(vec![TrustedKey {
            key_id: derived,
            public_key_pem: pub_pem,
        }]);
        let digest = hash_file(&artifact).unwrap();
        verify_artifact_dsse(&envelope_bytes, &digest, &trust).expect("verify");
    }
}
