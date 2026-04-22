pub mod access;
pub mod answers;
pub mod build;
pub mod catalog;
pub mod cli;
pub mod i18n;
pub mod project;
pub mod runtime;
pub mod setup;
pub mod wizard;

#[cfg(feature = "extensions")]
pub mod ext;

pub fn main_entry() -> anyhow::Result<()> {
    cli::run()
}
