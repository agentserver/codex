use crate::setup::ProtectedMetadataMode;
use crate::setup::ProtectedMetadataTarget;
use anyhow::Context;
use anyhow::Result;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// Layer: Windows enforcement. Existing metadata objects can be protected with
/// ACLs; missing names are monitored and removed if the sandbox creates them.
#[derive(Debug)]
pub(crate) struct ProtectedMetadataGuard {
    deny_paths: Vec<PathBuf>,
    monitored_paths: Vec<PathBuf>,
}

impl ProtectedMetadataGuard {
    pub(crate) fn deny_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.deny_paths.iter()
    }

    pub(crate) fn cleanup_created_monitored_paths(&self) -> Result<Vec<PathBuf>> {
        let mut removed = Vec::new();
        for path in &self.monitored_paths {
            if std::fs::symlink_metadata(path).is_err() {
                continue;
            }
            remove_metadata_path(path)
                .with_context(|| format!("failed to remove protected metadata {}", path.display()))?;
            removed.push(path.clone());
        }
        Ok(removed)
    }
}

pub(crate) fn prepare_protected_metadata_targets(
    targets: &[ProtectedMetadataTarget],
) -> ProtectedMetadataGuard {
    let mut deny_paths = Vec::new();
    let mut monitored_paths = Vec::new();
    for target in targets {
        match target.mode {
            ProtectedMetadataMode::ExistingDeny => {
                if std::fs::symlink_metadata(&target.path).is_ok() {
                    deny_paths.push(target.path.clone());
                }
            }
            ProtectedMetadataMode::MissingCreationMonitor => {
                monitored_paths.push(target.path.clone());
            }
        }
    }
    ProtectedMetadataGuard {
        deny_paths,
        monitored_paths,
    }
}

fn remove_metadata_path(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to inspect protected metadata {}", path.display()));
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove protected metadata {}", path.display()))?;
    } else {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove protected metadata {}", path.display()))?;
    }
    Ok(())
}
