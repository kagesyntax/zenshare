//! Filesystem locations the engine needs, supplied by the platform.
//!
//! The desktop answers this with `directories`; Android answers it with the
//! `AndroidApp` handle, because there is no `$HOME` and the writable locations
//! are app-scoped sandboxes. Keeping it as a parameter rather than a global is
//! what lets the same engine run on both.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Where this installation keeps its identity and drops received files.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Long-term Ed25519 seed. Must live somewhere private and persistent.
    pub identity: PathBuf,
    /// Directory received files land in.
    pub downloads: PathBuf,
}

impl AppPaths {
    /// Desktop layout: XDG data dir for the identity, the user's Downloads
    /// folder for received files.
    pub fn desktop(profile: &str) -> Result<Self> {
        let dirs = directories::ProjectDirs::from("dev", "zenshare", "ZenShare")
            .context("no home directory")?;

        let mut dir = dirs.data_dir().to_path_buf();
        if !profile.is_empty() {
            dir.push("profiles");
            dir.push(profile);
        }

        let downloads = directories::UserDirs::new()
            .and_then(|u| u.download_dir().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| dir.join("Downloads"));

        Ok(Self { identity: dir.join("identity.key"), downloads })
    }

    /// Android layout: both inside the app sandbox. `downloads` is the external
    /// files directory, which is app-scoped but visible over USB and in most
    /// file managers, so the user can actually get at what they received
    /// without a storage-permission dance.
    #[must_use]
    pub fn android(internal: PathBuf, external: Option<PathBuf>) -> Self {
        let downloads = external.unwrap_or_else(|| internal.clone()).join("ZenShare");
        Self { identity: internal.join("identity.key"), downloads }
    }

    /// Create the directories that must exist before the first transfer.
    pub fn ensure(&self) -> Result<()> {
        if let Some(parent) = self.identity.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        std::fs::create_dir_all(&self.downloads)
            .with_context(|| format!("create {}", self.downloads.display()))?;
        Ok(())
    }
}
