//! Remove only the running executable and explicitly owned resources.

#[cfg(windows)]
use super::UserPath;

use crate::download::cache_clean;
use crate::engine::uninstall;
use crate::shell::startup::records;
use crate::state::toolchains::ToolchainStore;
use crate::{fs::with_suffix, locking::FileLock};
use anyhow::{ensure, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Plan {
    pub executable: PathBuf,
    pub backup: PathBuf,
    pub manager: PathBuf,
    pub store: ToolchainStore,
    pub toolchains: Vec<uninstall::Plan>,
    pub imports: Vec<PathBuf>,
    pub cache: Option<cache_clean::Report>,
    pub startup: records::Removal,
    pub metadata: Vec<PathBuf>,
    #[cfg(windows)]
    pub user_path: Option<UserPath>,
}

impl Plan {
    pub fn new(
        executable: &Path,
        store: &ToolchainStore,
        cache: Option<&Path>,
        startup: &records::Store,
    ) -> Result<Self> {
        let manager = &store.home;
        let executable = fs::canonicalize(executable).context("cannot resolve this executable")?;
        let backup = with_suffix(&executable, ".outdated");
        for path in [&executable, &backup] {
            regular_or_missing(path)?;
        }
        let mut plan = Self {
            #[cfg(windows)]
            user_path: UserPath::for_executable(&executable)?,
            startup: startup.removal(&executable)?,
            executable,
            backup,
            manager: manager.to_owned(),
            store: store.clone(),
            toolchains: Vec::new(),
            imports: Vec::new(),
            cache: None,
            metadata: Vec::new(),
        };
        if let Some(cache) = cache {
            let items = store.scan()?;
            for item in items.installations.values() {
                if !item.managed {
                    plan.imports.push(item.path.clone());
                    continue;
                }
                let identity = fs::canonicalize(&item.path)?;
                ensure!(
                    !plan.executable.starts_with(&identity)
                        && !std::path::absolute(manager)?.starts_with(&identity),
                    "executable or manager is inside toolchain {}",
                    item.path.display()
                );
                plan.toolchains
                    .push(uninstall::run(store, &item.key, None, true)?);
            }
            for path in fs::read_dir(manager.join("references"))
                .into_iter()
                .flatten()
            {
                let path = path?.path();
                if path.extension().is_some_and(|e| e == "json") && regular_or_missing(&path)? {
                    plan.metadata.push(path);
                }
            }
            plan.cache = Some(cache_clean::run(cache, true)?);
            for name in ["config.json"] {
                let path = manager.join(name);
                if regular_or_missing(&path)? {
                    plan.metadata.push(path);
                }
            }
        }
        Ok(plan)
    }

    pub fn execute(self) -> Result<Completion> {
        let _upgrade = FileLock::acquire(&with_suffix(&self.executable, ".update.lock"))?;
        #[cfg(windows)]
        let deferred = Deferred::prepare(&self.executable, &self.backup)?;
        #[cfg(windows)]
        {
            let helper_directory = fs::canonicalize(deferred.directory.path())?;
            for toolchain in &self.toolchains {
                ensure!(!helper_directory.starts_with(fs::canonicalize(&toolchain.root)?),
                    "temporary helper is inside a toolchain scheduled for deletion; set TEMP outside that toolchain");
            }
        }
        for toolchain in &self.toolchains {
            let result = uninstall::run(&self.store, &toolchain.key, None, false)?;
            tracing::info!("Removed toolchain: {}", result.root.display());
        }
        if let Some(cache) = &self.cache {
            let result = cache_clean::run(&cache.cache_dir, false)?;
            for entry in &result.entries {
                let message = format!(
                    "Cache {:?}: {}{}",
                    entry.outcome,
                    result.cache_dir.join(&entry.path).display(),
                    entry
                        .reason
                        .as_ref()
                        .map(|reason| format!(" ({reason})"))
                        .unwrap_or_default()
                );
                if matches!(
                    entry.outcome,
                    cache_clean::Outcome::Busy | cache_clean::Outcome::Failed
                ) {
                    tracing::warn!("{message}");
                } else {
                    tracing::info!("{message}");
                }
            }
            ensure!(result.completed, "cache cleanup is incomplete; executable and manager metadata retained; resolve the reported conditions and retry");
        }
        self.startup.apply()?;
        #[cfg(windows)]
        if let Some(path) = &self.user_path {
            path.remove()?;
        }
        for path in &self.metadata {
            remove_file(path)?;
        }
        if self.cache.is_some() {
            let _ = fs::remove_dir(self.manager.join("toolchains"));
            let _ = fs::remove_dir(&self.manager);
            if self.manager.exists() {
                tracing::info!(
                    "Preserved remaining manager files and lock directories: {}",
                    self.manager.display()
                );
            }
        }
        #[cfg(windows)]
        {
            deferred.launch()
        }
        #[cfg(not(windows))]
        {
            remove_file(&self.backup)?;
            remove_file(&self.executable)?;
            Ok(Completion::Removed)
        }
    }
}

pub enum Completion {
    Removed,
    #[cfg(windows)]
    Scheduled {
        script: PathBuf,
    },
}

fn regular_or_missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !crate::fs::is_link(&meta),
                "expected a regular file: {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot inspect {}", path.display())),
    }
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot remove {}", path.display())),
    }
}

#[cfg(windows)]
struct Deferred {
    directory: tempfile::TempDir,
    script: PathBuf,
}

#[cfg(windows)]
impl Deferred {
    fn prepare(executable: &Path, backup: &Path) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("hanamori-uninstall-")
            .tempdir()?;
        let script = directory.path().join("uninstall.ps1");
        fs::write(&script, include_str!("../self_uninstall.ps1"))?;
        fs::write(
            directory.path().join("files.json"),
            serde_json::to_vec(&[backup, executable])?,
        )?;
        Ok(Self { directory, script })
    }

    fn launch(self) -> Result<Completion> {
        let system = std::env::var_os("SystemRoot")
            .context("SystemRoot is required to launch Windows PowerShell for self removal")?;
        let powershell =
            PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        std::process::Command::new(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&self.script)
            .arg("-ParentProcessId")
            .arg(std::process::id().to_string())
            .stdin(std::process::Stdio::null())
            .spawn()
            .context("could not start self removal helper; the executable has been preserved")?;
        let _ = self.directory.keep();
        Ok(Completion::Scheduled {
            script: self.script,
        })
    }
}
