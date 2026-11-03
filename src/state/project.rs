use crate::paths::PathExt;
use crate::state::toolchains::Toolchain;
use crate::{identity::PROJECT_FILE, version::VersionPrefix};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn schema_version() -> u32 {
    1
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(untagged)]
pub enum ProjectSdk {
    #[serde(skip)]
    #[default]
    Unspecified,
    Version(VersionPrefix),
    None(()),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    #[serde(default = "schema_version")]
    schema_version: u32,
    pub vs: Option<String>,
    pub channel: Option<String>,
    pub toolset: VersionPrefix,
    #[serde(default)]
    pub sdk: ProjectSdk,
    pub msbuild: Option<bool>,
    #[serde(default)]
    pub host: Option<crate::architecture::Arch>,
    #[serde(default)]
    pub target: Option<crate::architecture::Arch>,
}

impl Project {
    pub fn architecture(&self) -> crate::architecture::Architecture {
        let host = self.host.unwrap_or_else(crate::architecture::Arch::native);
        crate::architecture::Architecture {
            host,
            target: self.target.unwrap_or(host),
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported project schema; expected 1"
        );
        ensure!(
            (1..=3).contains(&self.toolset.segments()),
            "project toolset must be an installed version or numeric prefix (one to three segments)"
        );
        if let ProjectSdk::Version(sdk) = &self.sdk {
            ensure!(
                sdk.segments() == 4,
                "project SDK must have four numeric segments, or be null for no SDK"
            );
        }
        if let Some(vs) = &self.vs {
            ensure!(
                vs.len() == 4 && vs.bytes().all(|c| c.is_ascii_digit()),
                "project vs must be a four-digit generation"
            );
        }
        if let Some(channel) = &self.channel {
            ensure!(
                matches!(channel.as_str(), "release" | "preview"),
                "project channel must be release or preview"
            );
        }
        Ok(())
    }

    pub fn matches(&self, record: &Toolchain) -> bool {
        record.architecture == self.architecture()
            && self.toolset.matches(&record.toolset_version)
            && self
                .msbuild
                .is_none_or(|required| required == record.msbuild_version.is_some())
            && match (&self.sdk, &record.sdk_version) {
                (ProjectSdk::Unspecified, _) | (ProjectSdk::None(()), None) => true,
                (ProjectSdk::Version(a), Some(b)) => a.matches(b),
                _ => false,
            }
            && self
                .vs
                .as_ref()
                .is_none_or(|vs| record.key.starts_with(&format!("vs{vs}-")))
            && self.channel.as_ref().is_none_or(|channel| {
                record
                    .key
                    .split('/')
                    .next()
                    .is_some_and(|source| source.ends_with(&format!("-{channel}")))
            })
    }
}

pub fn find_project(cwd: &Path) -> Result<Option<(PathBuf, Project)>> {
    for directory in cwd.ancestors() {
        let path = directory.join(PROJECT_FILE);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect project {}", path.native_display()))
            }
            Ok(_) => {}
        }
        // Read-only: follow an explicit project symlink, but never silently skip a broken one.
        ensure!(
            std::fs::metadata(&path)
                .with_context(|| format!("cannot inspect project {}", path.native_display()))?
                .is_file(),
            "project {} must be a regular file",
            path.native_display()
        );
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read project {}", path.native_display()))?;
        let project: Project = serde_json::from_str(&text)
            .with_context(|| format!("invalid project {}", path.native_display()))?;
        project
            .validate()
            .with_context(|| format!("invalid project {}", path.native_display()))?;
        return Ok(Some((path, project)));
    }
    Ok(None)
}
