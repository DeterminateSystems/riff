use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};
use tokio::{fs::OpenOptions, io::{AsyncReadExt, AsyncWriteExt}, sync::{RwLock, RwLockReadGuard}, task::JoinHandle};
use serde::Deserialize;
use xdg::{BaseDirectories, BaseDirectoriesError};

use crate::dev_env::DevEnvironment;

const DEPENDENCY_REGISTRY_REMOTE_URL: &str = "https://fsm-server.fly.dev/fsm-registry.json";
const DEPENDENCY_REGISTRY_CACHE_PATH: &str = "registry.json";
const DEPENDENCY_REGISTRY_XDG_PREFIX: &str = "fsm";
const DEPENDENCY_REGISTRY_FALLBACK: &str = include_str!("../registry.json");

#[derive(Debug, thiserror::Error)]
pub enum DependencyRegistryError {
    #[error("XDG base directories error")]
    BaseDirectories(#[from] BaseDirectoriesError),
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("Request error")]
    Reqwest(#[from] reqwest::Error),
    #[error("Wrong registry data version: 1 (expected) != {0} (got)",)]
    WrongVersion(usize)
}

pub struct DependencyRegistry {
    data: Arc<RwLock<DependencyRegistryData>>,
    refresh_handle: Option<JoinHandle<()>>
}

impl DependencyRegistry {
    #[tracing::instrument(skip_all, fields(%offline))]
    pub async fn new(offline: bool) -> Result<Self, DependencyRegistryError> {
        let xdg_dirs = BaseDirectories::with_prefix(DEPENDENCY_REGISTRY_XDG_PREFIX)?;
        // Create the directory if needed
        let cached_registry_pathbuf = xdg_dirs.place_cache_file(Path::new(DEPENDENCY_REGISTRY_CACHE_PATH))?;
        // Create the file if needed.
        let mut cached_registry_file = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .create(true) // We do this proactively to avoid the user seeing a non-fatal error later when we freshen the cache.
            .open(cached_registry_pathbuf.clone()).await?;
        let mut cached_registry_content = Default::default();
        cached_registry_file.read_to_string(&mut cached_registry_content).await?;
        drop(cached_registry_file);

        cached_registry_content = if cached_registry_content.is_empty() {
            DEPENDENCY_REGISTRY_FALLBACK.to_string()
        } else { cached_registry_content };

        tracing::debug!("Cached content: {}", cached_registry_content);
        let data: DependencyRegistryData = serde_json::from_str(&cached_registry_content)?;
        if data.version != 1 {
            return Err(DependencyRegistryError::WrongVersion(data.version));
        }

        let data = Arc::new(RwLock::new(data));
        let refresh_handle = if !offline {
            // We detach the join handle as we don't actually care when/if this finishes
            let data = Arc::clone(&data);
            let refresh_handle = tokio::spawn(async move {
                // Refresh the cache
                tracing::trace!("Fetching new registry data from {DEPENDENCY_REGISTRY_REMOTE_URL}");
                let res = match reqwest::get(DEPENDENCY_REGISTRY_REMOTE_URL).await {
                    Ok(res) => res,
                    Err(err) => {
                        tracing::error!(err = %eyre::eyre!(err), "Could not fetch new registry data from {DEPENDENCY_REGISTRY_REMOTE_URL}");
                        return
                    },
                };
                let content = match res.text().await {
                    Ok(content) => content,
                    Err(err) => {
                        tracing::error!(err = %eyre::eyre!(err), "Could not fetch new registry data body from {DEPENDENCY_REGISTRY_REMOTE_URL}");
                        return
                    },
                };
                let fresh_data: DependencyRegistryData = match serde_json::from_str(&content) {
                    Ok(data) => data,
                    Err(err) => {
                        tracing::error!(err = %eyre::eyre!(err), "Could not parse new registry data from {DEPENDENCY_REGISTRY_REMOTE_URL}");
                        return
                    },
                };
                *data.write().await = fresh_data;
                // Write out the update
                let mut cached_registry_file = match OpenOptions::new().truncate(true).create(true).write(true).open(cached_registry_pathbuf.clone()).await {
                    Ok(cached_registry_file) => cached_registry_file,
                    Err(err) => {
                        tracing::error!(err = %eyre::eyre!(err), path = %cached_registry_pathbuf.display(), "Could not truncate XDG cached registry file to empty");
                        return
                    },
                };
                match cached_registry_file.write_all(content.trim().as_bytes()).await {
                    Ok(_) => tracing::trace!(path = %cached_registry_pathbuf.display(), "Refreshed remote registry into XDG cache"),
                    Err(err) => {
                        tracing::error!(err = %eyre::eyre!(err), "Could not write to {}", cached_registry_pathbuf.display());
                        return
                    },
                };
            });
            Some(refresh_handle)
        } else { None };

        Ok(Self {
            data,
            refresh_handle
        })
    }

    pub fn fresh(&self) -> bool {
        if let Some(ref handle) = self.refresh_handle {
            handle.is_finished()
        } else {
            // We're offline
            false
        }
    }

    pub async fn language_rust(&self) -> RwLockReadGuard<RustDependencyRegistryData> {
        RwLockReadGuard::map(self.data.read().await, |v| &v.language_rust)
    }
}

/// A registry of known mappings from language specific dependencies to fsm settings
#[derive(Deserialize, Default, Clone)]
pub struct DependencyRegistryData {
    pub(crate) version: usize, // Checked for ABI compat
    #[serde(default)]
    pub(crate) language_rust: RustDependencyRegistryData,
}

/// A language specific registry of dependencies to fsm settings
#[derive(Deserialize, Default, Clone)]
pub struct RustDependencyRegistryData {
    /// Settings which are needed for every instance of this language (Eg `cargo` for Rust)
    #[serde(default)]
    pub(crate) default: RustDependencyConfiguration,
    /// A mapping of dependencies (by crate name) to configuration
    // TODO(@hoverbear): How do we handle crates with conflicting names? eg a `rocksdb-sys` crate from one repo and another from another having different requirements?
    #[serde(default)]
    pub(crate) dependencies: HashMap<String, RustDependencyConfiguration>,
}
/// Dependency specific information needed for fsm
#[derive(Deserialize, Default, Clone)]
pub struct RustDependencyConfiguration {
    /// The Nix `buildInputs` needed
    #[serde(default)]
    pub(crate) build_inputs: HashSet<String>,
    /// Any packaging specific environment variables that need to be set
    #[serde(default)]
    pub(crate) environment_variables: HashMap<String, String>,
    /// The Nix packages which should have the result of `lib.getLib` run on them placed on the `LD_LIBRARY_PATH`
    #[serde(default)]
    pub(crate) ld_library_path_inputs: HashSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TryApplyError {
    #[error("Duplicate environment variable `{0}`")]
    DuplicateEnvironmentVariables(String),
}

impl RustDependencyConfiguration {
    pub(crate) fn try_apply(&self, dev_env: &mut DevEnvironment) -> Result<(), TryApplyError> {
        dev_env.build_inputs = dev_env
            .build_inputs
            .union(&self.build_inputs)
            .cloned()
            .collect();
        for (ref env_key, ref env_val) in &self.environment_variables {
            if dev_env
                .environment_variables
                .insert(env_key.to_string(), env_val.to_string())
                .is_some()
            {
                return Err(TryApplyError::DuplicateEnvironmentVariables(
                    env_key.to_string(),
                ));
            }
        }
        dev_env.ld_library_path = dev_env
            .ld_library_path
            .union(&self.ld_library_path_inputs)
            .cloned()
            .collect();
        Ok(())
    }
}
