pub mod access;
pub mod answers;
pub mod build;
pub mod bundle_fs;
pub mod catalog;
pub mod cli;
pub mod i18n;
pub mod project;
pub mod runtime;
pub mod setup;
pub mod wizard;

pub fn main_entry() -> anyhow::Result<()> {
    cli::run()
}

/// Render a top-level error for stderr, cause chain included.
///
/// `{err}` prints ONLY the outermost `.context(..)` and silently drops every
/// cause under it, which turns a diagnosable failure into a label. The whole
/// of what a CI job saw when `wizard apply` could not reach a registry on
/// 2026-09-08 (greentic-e2e run 34189156283) was
///
/// ```text
/// resolve OCI pack ref oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest
/// ```
///
/// — no status code, no transport error, nothing to tell a registry hiccup
/// from a pack that no longer exists. The two need completely different
/// responses, and the job had to be diagnosed by pulling the ref by hand
/// afterwards.
///
/// `{err:#}` appends each cause after `: `, so the same failure names what
/// actually went wrong. Keep this the ONE place the top-level error is
/// formatted: a second `eprintln!("{err}")` reintroduces the blind spot.
pub fn render_cli_error(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

#[cfg(test)]
mod render_cli_error_tests {
    use anyhow::{Context, anyhow};

    // The failure shape that motivated this: two layers of context over a
    // transport error, which is what `project::resolve_remote_pack_path`
    // builds when a registry refuses a pull.
    #[test]
    fn every_cause_is_printed_not_just_the_outermost_context() {
        let err = Err::<(), _>(anyhow!("Not authorized: url https://ghcr.io/v2/..."))
            .context("failed to pull `ghcr.io/greenticai/packs/messaging/messaging-teams:latest`")
            .context("resolve OCI pack ref oci://ghcr.io/greenticai/packs/messaging/messaging-teams:latest")
            .expect_err("err");

        let rendered = super::render_cli_error(&err);

        assert!(
            rendered.contains("resolve OCI pack ref oci://"),
            "the outermost context must still lead: {rendered}"
        );
        assert!(
            rendered.contains("failed to pull `ghcr.io/"),
            "the intermediate context was dropped: {rendered}"
        );
        assert!(
            rendered.contains("Not authorized: url https://ghcr.io/v2/..."),
            "the root cause was dropped, which is the whole defect: {rendered}"
        );
    }

    // A single-layer error must not gain punctuation or a trailing separator —
    // the common case is still one line.
    #[test]
    fn an_error_with_no_cause_renders_as_itself() {
        let err = anyhow!("bundle already exists at /tmp/bundle");
        assert_eq!(
            super::render_cli_error(&err),
            "bundle already exists at /tmp/bundle"
        );
    }
}
