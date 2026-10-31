//! Shared environment calculation for child processes and future shell activation.
//! Only managed variables are saved; unrelated environment values never enter the state.

use crate::env;
use crate::shell::syntax::{Shell, ShellEnvironment};
pub use crate::state::active::ActiveToolchain;
use crate::state::local::Selection;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;

pub type Environment = BTreeMap<OsString, OsString>;
pub type Changes = BTreeMap<String, Option<OsString>>;

pub const MANAGED_KEYS: &[&str] = &[
    "PATH",
    "INCLUDE",
    "LIB",
    "CC",
    "CXX",
    "DISTUTILS_USE_SDK",
    "VSCMD_ARG_HOST_ARCH",
    "VSCMD_ARG_TGT_ARCH",
    "VCToolsVersion",
    "VCToolsInstallDir",
    "VCINSTALLDIR",
    "WindowsSDKVersion",
    "WindowsSDKLibVersion",
    "WindowsSdkDir",
    "WindowsSdkBinPath",
    "WindowsSdkVerBinPath",
    "VSINSTALLDIR",
    "VCTargetsPath",
    "MSBuildExtensionsPath",
    "MSBuildExtensionsPath32",
    "ForceImportAfterCppDefaultProps",
    "ForceImportAfterCppTargets",
    "UseEnv",
    "DisableRegistryUse",
    "PreferredToolArchitecture",
    "MSBUILDDISABLENODEREUSE",
];

pub const STATE_KEY: &str = "_HANAMORI_STATE";
pub const STATE_KEYS: &[&str] = &[
    STATE_KEY,
    "_HANAMORI_STATE_VERSION",
    "_HANAMORI_ACTIVE_ID",
    "_HANAMORI_ACTIVE_MANAGER_DIR",
    "_HANAMORI_ACTIVE_ROOT",
];

/// Native OS values are losslessly saved, including unset, empty and non-Unicode values.
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "encoding",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SavedValue {
    Text(String),
    UnixBytes(Vec<u8>),
    WindowsWide(Vec<u16>),
}

impl SavedValue {
    fn capture(value: &OsStr) -> Self {
        if let Some(text) = value.to_str() {
            return Self::Text(text.to_owned());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::UnixBytes(value.as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self::WindowsWide(value.encode_wide().collect())
        }
    }

    fn restore(&self) -> Result<OsString> {
        match self {
            Self::Text(text) => {
                ensure!(
                    !text.contains('\0'),
                    "session baseline contains a NUL value"
                );
                Ok(text.into())
            }
            #[cfg(unix)]
            Self::UnixBytes(bytes) => {
                use std::os::unix::ffi::OsStringExt;
                ensure!(!bytes.contains(&0), "session baseline contains a NUL value");
                Ok(OsString::from_vec(bytes.clone()))
            }
            #[cfg(windows)]
            Self::WindowsWide(wide) => {
                use std::os::windows::ffi::OsStringExt;
                ensure!(!wide.contains(&0), "session baseline contains a NUL value");
                Ok(OsString::from_wide(wide))
            }
            _ => anyhow::bail!("session baseline was captured for a different operating system"),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Session {
    schema_version: u32,
    pub active: ActiveToolchain,
    #[serde(serialize_with = "write_baseline")]
    baseline: Changes,
}

impl<'de> Deserialize<'de> for Session {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Snapshot {
            schema_version: u32,
            active: ActiveToolchain,
            #[serde(deserialize_with = "read_baseline")]
            baseline: Changes,
        }

        let snapshot = Snapshot::deserialize(deserializer)?;
        let session = Self {
            schema_version: snapshot.schema_version,
            active: snapshot.active,
            baseline: snapshot.baseline,
        };
        session.validate().map_err(serde::de::Error::custom)?;
        Ok(session)
    }
}

fn read_baseline<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Changes, D::Error> {
    let saved: BTreeMap<String, Option<SavedValue>> =
        crate::serde_helpers::unique_map(deserializer)?;
    saved
        .into_iter()
        .map(|(key, value)| {
            let value = value
                .map(|value| value.restore())
                .transpose()
                .map_err(serde::de::Error::custom)?;
            Ok((key, value))
        })
        .collect()
}

fn write_baseline<S: serde::Serializer>(
    baseline: &Changes,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(baseline.len()))?;
    for (key, value) in baseline {
        map.serialize_entry(key, &value.as_deref().map(SavedValue::capture))?;
    }
    map.end()
}

impl Session {
    /// Read the inherited snapshot, never the current references or project/default settings.
    /// Partial or invalid state is an error rather than a new baseline to stack on top of.
    pub fn read(environment: &Environment) -> Result<Option<Self>> {
        let Some(raw) = lookup(environment, STATE_KEY)? else {
            for key in &STATE_KEYS[1..] {
                ensure!(
                    lookup(environment, key)?.is_none(),
                    "incomplete Hanamori session state; {STATE_KEY} is missing"
                );
            }
            return Ok(None);
        };
        let raw = raw
            .to_str()
            .context("Hanamori session state is not Unicode")?;
        let session: Self = serde_json::from_str(raw).context("invalid Hanamori session state")?;
        Ok(Some(session))
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported Hanamori session schema; expected 1"
        );
        ensure!(
            self.baseline.len() == MANAGED_KEYS.len()
                && MANAGED_KEYS
                    .iter()
                    .all(|key| self.baseline.contains_key(*key)),
            "Hanamori session variables do not match this build (an older or incomplete session); open a fresh terminal from the desktop/start menu and activate again. Starting a child shell inherits the old state. Installed toolchains do not need reinstalling"
        );
        let active = &self.active;
        for (label, path) in [
            ("manager directory", &active.manager_dir),
            ("root", &active.root),
            ("canonical root", &active.canonical_root),
        ] {
            ensure!(
                !path.contains('\0') && Path::new(path).is_absolute(),
                "session {label} must be an absolute native path"
            );
        }
        ensure!(
            env::valid_directory_version(&active.toolset, 3),
            "invalid session toolset version"
        );
        ensure!(
            active
                .sdk
                .as_ref()
                .is_none_or(|sdk| env::valid_directory_version(sdk, 4)),
            "invalid session SDK version"
        );
        ensure!(
            active.host == "x64" && active.target == "x64",
            "unsupported session architecture"
        );
        Ok(())
    }

    fn markers(&self) -> [(&'static str, String); 4] {
        [
            ("_HANAMORI_STATE_VERSION", self.schema_version.to_string()),
            (
                "_HANAMORI_ACTIVE_ID",
                self.active
                    .key
                    .clone()
                    .unwrap_or_else(|| "directory".into()),
            ),
            (
                "_HANAMORI_ACTIVE_MANAGER_DIR",
                self.active.manager_dir.clone(),
            ),
            ("_HANAMORI_ACTIVE_ROOT", self.active.root.clone()),
        ]
    }

    /// Restore only managed keys and remove our active state; public config is untouched.
    pub fn restoration(&self) -> Changes {
        let mut changes = self.baseline.clone();
        changes.extend(STATE_KEYS.iter().map(|key| ((*key).to_owned(), None)));
        changes
    }
}

pub struct PreparedEnvironment {
    pub changes: Changes,
    pub session: Session,
    pub toolchain_changed: bool,
}

impl PreparedEnvironment {
    /// Apply child-only overrides. Command retains all unrelated inherited variables and I/O.
    pub fn apply(&self, command: &mut std::process::Command) {
        for (key, value) in &self.changes {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
    }
}

/// Validate and calculate everything before any process is started or script is applied.
pub fn prepare(
    selection: &Selection,
    manager_dir: &Path,
    environment: &Environment,
) -> Result<PreparedEnvironment> {
    let previous = Session::read(environment)?;
    let previous_active = previous.as_ref().map(|session| session.active.clone());
    let baseline = if let Some(previous) = previous {
        previous.baseline
    } else {
        MANAGED_KEYS
            .iter()
            .map(|key| {
                Ok((
                    (*key).to_owned(),
                    lookup(environment, key)?.map(OsStr::to_owned),
                ))
            })
            .collect::<Result<_>>()?
    };
    let layout = selection.layout();
    // Layout variables are text-based; reject lossy conversion of a target path explicitly.
    let session = Session {
        schema_version: 1,
        active: ActiveToolchain {
            key: selection.key().map(str::to_owned),
            manager_dir: manager_dir
                .to_str()
                .context("manager path cannot be represented in session state")?
                .to_owned(),
            root: layout
                .root
                .to_str()
                .context("toolchain path cannot be represented in the compiler environment")?
                .to_owned(),
            canonical_root: std::fs::canonicalize(&layout.root)?
                .to_str()
                .context("canonical toolchain path cannot be represented in session state")?
                .to_owned(),
            toolset: layout.vc_tools_version.clone(),
            sdk: layout
                .has_windows_sdk()
                .then(|| layout.windows_sdk_version.clone()),
            host: "x64".into(),
            msbuild: layout.msbuild_version.clone(),
            target: "x64".into(),
        },
        baseline,
    };
    session.validate()?;
    let changes = session.native_changes()?;
    let toolchain_changed = previous_active.is_none_or(|previous| {
        previous.manager_dir != session.active.manager_dir
            || previous.canonical_root != session.active.canonical_root
            || previous.toolset != session.active.toolset
            || previous.sdk != session.active.sdk
            || previous.msbuild != session.active.msbuild
            || previous.host != session.active.host
            || previous.target != session.active.target
    });
    Ok(PreparedEnvironment {
        changes,
        session,
        toolchain_changed,
    })
}

impl Session {
    /// Rebuild without consulting references or requiring the old files to still exist.
    pub fn native_changes(&self) -> Result<Changes> {
        let layout = env::ToolchainLayout {
            root: self.active.root.clone().into(),
            vc_tools_version: self.active.toolset.clone(),
            windows_sdk_version: self.active.sdk.clone().unwrap_or_default(),
            msbuild_version: self.active.msbuild.clone(),
        };
        // Cmd selects native path spelling; joining PATH uses the current OS separator.
        let parts = ShellEnvironment::build(&layout, Shell::Cmd);
        let mut changes: Changes = MANAGED_KEYS
            .iter()
            .map(|key| ((*key).to_owned(), None))
            .collect();
        changes.extend(
            parts
                .variables
                .into_iter()
                .map(|(key, value)| (key, Some(value.into()))),
        );
        let path = std::env::join_paths(parts.path_entries.iter().map(Path::new))
            .context("cannot represent toolchain directories in PATH")?;
        for (key, mut prefix, separator) in [
            ("PATH", path, if cfg!(windows) { ";" } else { ":" }),
            (
                "INCLUDE",
                OsString::from(parts.include_entries.join(";")),
                ";",
            ),
            ("LIB", OsString::from(parts.lib_entries.join(";")), ";"),
        ] {
            if let Some(original) = &self.baseline[key] {
                if !original.is_empty() {
                    prefix.push(separator);
                    prefix.push(original);
                }
            }
            changes.insert(key.into(), Some(prefix));
        }
        for (key, value) in self.markers() {
            changes.insert(key.into(), Some(value.into()));
        }
        changes.insert(STATE_KEY.into(), Some(serde_json::to_string(self)?.into()));
        Ok(changes)
    }

    pub fn environment_differences(&self, environment: &Environment) -> Result<Vec<String>> {
        let changes = self.native_changes()?;
        MANAGED_KEYS
            .iter()
            .filter_map(|key| match lookup(environment, key) {
                Ok(value) if value == changes[*key].as_deref() => None,
                Ok(_) => Some(Ok((*key).to_owned())),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }
}

fn lookup<'a>(environment: &'a Environment, key: &str) -> Result<Option<&'a OsStr>> {
    let mut found = environment.iter().filter(|(name, _)| {
        if cfg!(windows) {
            name.to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(key))
        } else {
            name.as_os_str() == OsStr::new(key)
        }
    });
    let value = found.next().map(|(_, value)| value.as_os_str());
    ensure!(
        found.next().is_none(),
        "duplicate managed environment variable {key}"
    );
    Ok(value)
}
