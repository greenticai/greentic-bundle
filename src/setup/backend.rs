use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{PersistedSetupState, SETUP_STATE_DIR};

pub trait SetupBackend {
    fn persist(&self, state: &PersistedSetupState) -> Result<Option<PathBuf>>;
}

#[derive(Debug, Clone)]
pub struct FileSetupBackend {
    root: PathBuf,
}

impl FileSetupBackend {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

impl SetupBackend for FileSetupBackend {
    fn persist(&self, state: &PersistedSetupState) -> Result<Option<PathBuf>> {
        let path = self
            .root
            .join(SETUP_STATE_DIR)
            .join(format!("{}.json", state.provider_id));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(state)?))?;
        Ok(Some(path))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSetupBackend;

impl SetupBackend for NoopSetupBackend {
    fn persist(&self, _state: &PersistedSetupState) -> Result<Option<PathBuf>> {
        Ok(None)
    }
}
