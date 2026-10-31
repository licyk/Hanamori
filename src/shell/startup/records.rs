//! Exact startup locations and content installed by each executable.

use crate::shell::{session::Environment, startup::profile::ProfilePlan, Shell};
use crate::{fs::atomic_write, locking};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Record {
    schema_version: u32,
    program: OsString,
    runtime: Option<OsString>,
    entry: Entry,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Entry {
    Profile {
        path: OsString,
        target: OsString,
        shell: String,
        block: String,
    },
    Cmd {
        loader: OsString,
        files: Vec<(OsString, Vec<u8>)>,
    },
}

pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn from_environment(environment: &Environment) -> Result<Self> {
        #[cfg(windows)]
        let base = environment
            .get(std::ffi::OsStr::new("LOCALAPPDATA"))
            .map(PathBuf::from);
        #[cfg(not(windows))]
        let base = environment
            .get(std::ffi::OsStr::new("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| {
                environment
                    .get(std::ffi::OsStr::new("HOME"))
                    .map(|home| PathBuf::from(home).join(".local/share"))
            });
        let base = base
            .or_else(dirs::data_local_dir)
            .context("cannot locate startup registration storage")?;
        ensure!(
            base.is_absolute(),
            "startup registration storage must be absolute"
        );
        Ok(Self::new(
            base.join(crate::identity::PRODUCT_NAME)
                .join("shell/registrations"),
        ))
    }

    fn path(&self, kind: &str, target: &Path) -> PathBuf {
        self.directory.join(format!(
            "{kind}-{:x}.json",
            Sha256::digest(locking::path_key(target))
        ))
    }

    pub fn profiles(
        &self,
        program: &Path,
        runtime: Option<&OsString>,
        shell: Shell,
        plans: &[ProfilePlan],
    ) -> Result<()> {
        for plan in plans {
            let path = self.path("profile", &plan.target);
            if plan.is_removal() {
                remove_file(&path)?;
            } else {
                let record = Record {
                    schema_version: 1,
                    program: fs::canonicalize(program)?.into_os_string(),
                    runtime: runtime.cloned(),
                    entry: Entry::Profile {
                        path: plan.requested.as_os_str().to_owned(),
                        target: plan.target.as_os_str().to_owned(),
                        shell: shell.name().into(),
                        block: plan.preview.clone(),
                    },
                };
                fs::create_dir_all(&self.directory)?;
                atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn cmd(
        &self,
        program: &Path,
        loader: &Path,
        files: Vec<(OsString, Vec<u8>)>,
    ) -> Result<()> {
        let record = Record {
            schema_version: 1,
            program: fs::canonicalize(program)?.into_os_string(),
            runtime: None,
            entry: Entry::Cmd {
                loader: loader.as_os_str().to_owned(),
                files,
            },
        };
        fs::create_dir_all(&self.directory)?;
        atomic_write(
            &self.path("cmd", loader),
            &serde_json::to_vec_pretty(&record)?,
        )
    }

    pub fn removal(&self, program: &Path) -> Result<Removal> {
        let program = fs::canonicalize(program)?;
        let mut removal = Removal {
            profiles: Vec::new(),
            files: Vec::new(),
            records: Vec::new(),
            notes: Vec::new(),
            directory: self.directory.clone(),
            #[cfg(windows)]
            cmd: None,
        };
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(removal),
            Err(error) => return Err(error).context("cannot read startup registrations"),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let key = name
                .strip_prefix("profile-")
                .or_else(|| name.strip_prefix("cmd-"))
                .and_then(|key| key.strip_suffix(".json"));
            if !entry.file_type()?.is_file()
                || !key.is_some_and(|key| {
                    key.len() == 64
                        && key
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
            {
                continue;
            }
            let record: Record = serde_json::from_slice(&fs::read(&path)?).with_context(|| {
                format!(
                    "cannot read startup record {}; preserve it or repair it before retrying",
                    path.display()
                )
            })?;
            ensure!(
                record.schema_version == 1,
                "unsupported startup record: {}",
                path.display()
            );
            if !locking::same_path(Path::new(&record.program), &program) {
                continue;
            }
            match record.entry {
                Entry::Profile {
                    path: profile,
                    target,
                    shell,
                    block,
                } => {
                    ensure!(
                        Path::new(&profile).is_absolute() && Path::new(&target).is_absolute(),
                        "invalid startup profile path"
                    );
                    let plan = ProfilePlan::new(Path::new(&profile), Shell::parse(&shell)?, None)?;
                    if !locking::same_path(&plan.target, Path::new(&target))
                        || (!plan.preview.is_empty() && plan.preview != block)
                    {
                        removal.notes.push(format!(
                            "Preserved changed startup entry: {} (remove manually)",
                            Path::new(&profile).display()
                        ));
                        continue;
                    }
                    let plan = plan.remove_empty_owned(crate::shell::startup::bash::CREATED)?;
                    plan.check()?;
                    removal.profiles.push(plan);
                }
                Entry::Cmd { loader, files } => {
                    let directory = Path::new(&loader)
                        .parent()
                        .context("invalid CMD loader path")?;
                    ensure!(directory.is_absolute(), "invalid CMD loader directory");
                    let mut changed = false;
                    for (file, expected) in &files {
                        let file = Path::new(file);
                        ensure!(
                            file.parent() == Some(directory)
                                && matches!(
                                    file.file_name().and_then(|s| s.to_str()),
                                    Some("hanamori.cmd" | "init.cmd" | "cd.cmd")
                                ),
                            "invalid CMD helper path"
                        );
                        match fs::read(file) {
                            Ok(bytes) if bytes != *expected => changed = true,
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                            result => {
                                result?;
                            }
                        }
                    }
                    if changed {
                        removal.notes.push(format!(
                            "Preserved modified CMD helpers: {}",
                            directory.display()
                        ));
                        continue;
                    }
                    #[cfg(windows)]
                    if let Some(plan) = crate::shell::cmd::autorun::windows::UserPlan::removal_for(
                        Path::new(&loader),
                    )? {
                        removal.cmd = Some(plan);
                    }
                    removal
                        .files
                        .extend(files.into_iter().map(|(path, _)| PathBuf::from(path)));
                }
            }
            removal.records.push(path);
        }
        Ok(removal)
    }
}

pub struct Removal {
    pub profiles: Vec<ProfilePlan>,
    pub files: Vec<PathBuf>,
    pub notes: Vec<String>,
    records: Vec<PathBuf>,
    directory: PathBuf,
    #[cfg(windows)]
    cmd: Option<crate::shell::cmd::autorun::windows::UserPlan>,
}

impl Removal {
    pub fn has_cmd(&self) -> bool {
        #[cfg(windows)]
        {
            self.cmd.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn apply(&self) -> Result<()> {
        let backups = crate::shell::startup::profile::apply_batch(&self.profiles)?;
        for backup in backups.into_iter().flatten() {
            tracing::info!("Original profile backed up at {}", backup.display());
        }
        #[cfg(windows)]
        if let Some(cmd) = &self.cmd {
            cmd.apply()?;
        }
        for path in &self.files {
            remove_file(path)?;
        }
        for path in &self.records {
            remove_file(path)?;
        }
        for path in &self.files {
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        let _ = fs::remove_dir(&self.directory);
        Ok(())
    }
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot remove {}", path.display())),
    }
}
