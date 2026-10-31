//! Read-only configuration resolution. Business services receive explicit options.

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOG: &str = "info";
pub use crate::download::network::parse_url;
pub use crate::download::transfer::{MAX_CONNECTIONS, MAX_FILE_WORKERS};

#[derive(clap::Args, Default)]
pub struct ConfigOverrides {
    /// Manager directory (HANAMORI_HOME; default: platform local data directory).
    /// CLI paths are relative to the working directory; no tilde or variable expansion.
    #[arg(long, global = true, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub manager_dir: Option<PathBuf>,
    /// Managed toolchain storage (HANAMORI_TOOLCHAINS_DIR).
    #[arg(long, global = true, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub toolchains_dir: Option<PathBuf>,
    /// Complete payload cache (HANAMORI_CACHE_DIR).
    #[arg(long, global = true, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub cache_dir: Option<PathBuf>,
    /// Persistent download workspace (HANAMORI_DOWNLOAD_DIR).
    #[arg(long, global = true, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    pub download_dir: Option<PathBuf>,
    /// HTTP(S) proxy (HANAMORI_PROXY).
    #[arg(long, global = true, value_name = "URL")]
    pub proxy: Option<String>,
    /// Payload mirror base URL (HANAMORI_MIRROR).
    #[arg(long, global = true, value_name = "URL")]
    pub mirror: Option<String>,
    /// Concurrent payloads, 1..256 (HANAMORI_FILE_WORKERS; default: 4).
    #[arg(long, global = true, value_name = "N")]
    pub file_workers: Option<usize>,
    /// Connections per payload, 1..64 (HANAMORI_CONNECTIONS; default: 8).
    #[arg(long, global = true, value_name = "N")]
    pub connections: Option<usize>,
    /// Log filter (HANAMORI_LOG; default: info).
    #[arg(long, global = true, value_name = "FILTER")]
    pub log: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Cli,
    Environment,
    ConfigFile,
    Default,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Environment => "environment",
            Self::ConfigFile => "config_file",
            Self::Default => "default",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Setting<T> {
    pub value: T,
    pub source: Source,
    /// Default storage paths derive from the resolved manager directory.
    pub derived_from: Option<Source>,
}

impl<T> Setting<T> {
    fn new(value: T, source: Source) -> Self {
        Self {
            value,
            source,
            derived_from: None,
        }
    }

    fn display(&self, value: Value) -> Value {
        let mut result = json!({ "value": value, "source": self.source.name() });
        if let Some(source) = self.derived_from {
            result["derived_from"] = json!({ "field": "manager_dir", "source": source.name() });
        }
        result
    }
}

/// Resolve only the fields a command consumes. Session restoration bypasses config.
#[derive(Clone, Copy)]
pub enum Scope {
    Local,
    Remote,
    Install,
    Cache,
    All,
}

#[derive(Debug)]
pub struct InstallConfig {
    pub download_dir: Option<PathBuf>,
    pub mirror: Option<reqwest::Url>,
    pub file_workers: crate::download::transfer::FileWorkers,
    pub connections: crate::download::transfer::Connections,
}

#[derive(Debug)]
pub struct ResolvedConfig {
    pub manager_dir: PathBuf,
    pub toolchains_dir: PathBuf,
    pub log: String,
    pub log_filter: tracing_subscriber::EnvFilter,
    pub proxy: Option<reqwest::Url>,
    pub install: Option<InstallConfig>,
    pub cache_dir: Option<PathBuf>,
    diagnostics: Map<String, Value>,
}

impl ResolvedConfig {
    pub fn toolchains(&self) -> crate::state::toolchains::ToolchainStore {
        crate::state::toolchains::ToolchainStore::new(&self.manager_dir)
            .with_directory(&self.toolchains_dir)
    }
    pub fn to_json(&self) -> Value {
        Value::Object(self.diagnostics.clone())
    }
}

/// Injected input keeps tests independent of global process environment mutations.
pub struct ConfigContext {
    pub cwd: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
    pub platform_home: Option<PathBuf>,
}

impl ConfigContext {
    pub fn capture() -> Result<Self> {
        Self::from_environment(std::env::vars_os().collect())
    }

    pub fn from_environment(environment: BTreeMap<OsString, OsString>) -> Result<Self> {
        Ok(Self {
            cwd: std::env::current_dir().context("cannot determine the working directory")?,
            environment,
            platform_home: dirs::data_local_dir()
                .map(|path| path.join(crate::identity::PRODUCT_NAME)),
        })
    }

    fn env(&self, key: &str) -> Option<OsString> {
        environment_value(&self.environment, key).cloned()
    }

    pub fn manager_dir(&self, override_path: Option<&Path>) -> Result<Setting<PathBuf>> {
        if !self.cwd.is_absolute() {
            bail!("configuration working directory must be absolute");
        }
        let manager_dir = if let Some(path) = override_path {
            Setting::new(absolute(path, &self.cwd)?, Source::Cli)
        } else if let Some(value) = self.env("HANAMORI_HOME") {
            let path = PathBuf::from(value);
            require_absolute(&path, "HANAMORI_HOME")?;
            Setting::new(path, Source::Environment)
        } else {
            let path = self.platform_home.clone().context(
                "cannot locate the platform data directory; set HANAMORI_HOME or --manager-dir",
            )?;
            require_absolute(&path, "platform data directory")?;
            Setting::new(path, Source::Default)
        };
        Ok(manager_dir)
    }

    pub fn completion_store(
        &self,
        manager: Option<&Path>,
        directory: Option<&Path>,
    ) -> Result<crate::state::toolchains::ToolchainStore> {
        let home = self.manager_dir(manager)?;
        let resolver = Resolver {
            context: self,
            file: read_config(&home.value.join("config.json")).unwrap_or_default(),
            home: &home,
        };
        let path = resolver.path("toolchains_dir", directory, "toolchains")?;
        Ok(crate::state::toolchains::ToolchainStore::new(&home.value).with_directory(&path.value))
    }

    pub fn resolve(&self, overrides: &ConfigOverrides, scope: Scope) -> Result<ResolvedConfig> {
        let manager_dir = self.manager_dir(overrides.manager_dir.as_deref())?;
        let file = read_config(&manager_dir.value.join("config.json"))?;
        let resolver = Resolver {
            context: self,
            file,
            home: &manager_dir,
        };
        let log = resolver
            .string("log", overrides.log.clone().map(Into::into))?
            .unwrap_or_else(|| Setting::new(DEFAULT_LOG.to_owned(), Source::Default));
        let mut diagnostics = Map::new();
        diagnostics.insert(
            "manager_dir".into(),
            manager_dir.display(json!(manager_dir.value.to_string_lossy())),
        );
        diagnostics.insert("log".into(), log.display(json!(log.value)));
        let log_filter = parse_log_filter(&log.value, log.source)?;
        let proxy = if matches!(scope, Scope::Remote | Scope::Install | Scope::All) {
            let setting = resolver.url("proxy", overrides.proxy.clone(), true)?;
            diagnostics.insert(
                "proxy".into(),
                setting.display(json!(setting.value.as_ref().map(|url| url.as_str()))),
            );
            setting.value
        } else {
            None
        };
        let toolchains_dir = if matches!(scope, Scope::Local | Scope::Install | Scope::All) {
            resolver.path(
                "toolchains_dir",
                overrides.toolchains_dir.as_deref(),
                "toolchains",
            )?
        } else {
            Setting::new(manager_dir.value.join("toolchains"), Source::Default)
        };
        diagnostics.insert(
            "toolchains_dir".into(),
            toolchains_dir.display(json!(toolchains_dir.value.to_string_lossy())),
        );
        let install = if matches!(scope, Scope::Install | Scope::All) {
            let download_dir =
                resolver.optional_path("download_dir", overrides.download_dir.as_deref())?;
            let mirror = resolver.url("mirror", overrides.mirror.clone(), false)?;
            let file_workers = resolver.number(
                "file_workers",
                overrides.file_workers,
                crate::download::transfer::DEFAULT_WORKERS,
            )?;
            let connections = resolver.number(
                "connections",
                overrides.connections,
                crate::download::transfer::DEFAULT_RANGES,
            )?;

            diagnostics.insert(
                "download_dir".into(),
                download_dir.display(json!(download_dir
                    .value
                    .as_ref()
                    .map(|path| path.to_string_lossy()))),
            );
            diagnostics.insert(
                "mirror".into(),
                mirror.display(json!(mirror.value.as_ref().map(|url| url.as_str()))),
            );
            diagnostics.insert(
                "file_workers".into(),
                file_workers.display(json!(file_workers.value.get())),
            );
            diagnostics.insert(
                "connections".into(),
                connections.display(json!(connections.value.get())),
            );
            Some(InstallConfig {
                download_dir: download_dir.value,
                mirror: mirror.value,
                file_workers: file_workers.value,
                connections: connections.value,
            })
        } else {
            None
        };
        let cache_dir = if matches!(scope, Scope::Cache | Scope::Install | Scope::All) {
            let setting = resolver.path("cache_dir", overrides.cache_dir.as_deref(), "cache")?;
            diagnostics.insert(
                "cache_dir".into(),
                setting.display(json!(setting.value.to_string_lossy())),
            );
            Some(setting.value)
        } else {
            None
        };
        Ok(ResolvedConfig {
            manager_dir: manager_dir.value,
            toolchains_dir: toolchains_dir.value,
            log: log.value,
            diagnostics,
            log_filter,
            proxy,
            install,
            cache_dir,
        })
    }
}

/// Resolve logging without loading configuration or requiring a working directory.
pub fn inherited_log_filter(
    log: Option<&str>,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<tracing_subscriber::EnvFilter> {
    let (value, source) = if let Some(log) = log {
        (log, Source::Cli)
    } else if let Some(value) = environment_value(environment, "HANAMORI_LOG") {
        (
            value.to_str().context("log must be valid Unicode")?,
            Source::Environment,
        )
    } else {
        (DEFAULT_LOG, Source::Default)
    };
    parse_log_filter(value, source)
}

fn parse_log_filter(value: &str, source: Source) -> Result<tracing_subscriber::EnvFilter> {
    if value.is_empty() {
        bail!("invalid log filter from {}", source.name());
    }
    tracing_subscriber::EnvFilter::try_new(value)
        .with_context(|| format!("invalid log filter {value:?} from {}", source.name()))
}

fn environment_value<'a>(
    environment: &'a BTreeMap<OsString, OsString>,
    key: &str,
) -> Option<&'a OsString> {
    #[cfg(windows)]
    let value = environment.iter().find_map(|(name, value)| {
        name.to_str()
            .filter(|name| name.eq_ignore_ascii_case(key))
            .map(|_| value)
    });
    #[cfg(not(windows))]
    let value = environment.get(&OsString::from(key));
    value.filter(|value| !value.is_empty())
}

fn read_config(path: &Path) -> Result<Map<String, Value>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let value: Value = serde_json::from_slice(&bytes).context("invalid config.json syntax")?;
    let Value::Object(file) = value else {
        bail!("config.json must be an object");
    };
    if file
        .get("schema_version")
        .is_some_and(|version| version.as_u64() != Some(1))
    {
        bail!("config.json requires schema_version 1");
    }
    Ok(file)
}

struct Resolver<'a> {
    context: &'a ConfigContext,
    file: Map<String, Value>,
    home: &'a Setting<PathBuf>,
}

impl Resolver<'_> {
    fn input(&self, field: &str, cli: Option<OsString>) -> Result<Option<Setting<OsString>>> {
        if let Some(value) = cli {
            return Ok(Some(Setting::new(value, Source::Cli)));
        }
        let key = format!(
            "{}{}",
            crate::identity::ENV_PREFIX,
            field.to_ascii_uppercase()
        );
        if let Some(value) = self.context.env(&key) {
            return Ok(Some(Setting::new(value, Source::Environment)));
        }
        match self.file.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(Setting::new(value.into(), Source::ConfigFile))),
            Some(Value::Number(value)) if matches!(field, "connections" | "file_workers") => Ok(
                Some(Setting::new(value.to_string().into(), Source::ConfigFile)),
            ),
            Some(_) => bail!("invalid {field} type in config.json"),
        }
    }

    fn string(&self, field: &str, cli: Option<OsString>) -> Result<Option<Setting<String>>> {
        self.input(field, cli)?
            .map(|setting| {
                let value = setting.value.into_string().map_err(|value| {
                    anyhow::anyhow!("{field} must be valid Unicode; received {value:?}")
                })?;
                Ok(Setting::new(value, setting.source))
            })
            .transpose()
    }

    fn optional_path(&self, field: &str, cli: Option<&Path>) -> Result<Setting<Option<PathBuf>>> {
        let Some(input) = self.input(field, cli.map(|p| p.as_os_str().to_owned()))? else {
            return Ok(Setting::new(None, Source::Default));
        };
        let path = PathBuf::from(input.value);
        let path = match input.source {
            Source::Environment => {
                require_absolute(&path, field)?;
                path
            }
            Source::ConfigFile => absolute(&path, &self.home.value)?,
            _ => absolute(&path, &self.context.cwd)?,
        };
        Ok(Setting::new(Some(path), input.source))
    }

    fn path(&self, field: &str, cli: Option<&Path>, default: &str) -> Result<Setting<PathBuf>> {
        let setting = self.optional_path(field, cli)?;
        Ok(match setting.value {
            Some(path) => Setting::new(path, setting.source),
            None => Setting {
                value: self.home.value.join(default),
                source: Source::Default,
                derived_from: Some(self.home.source),
            },
        })
    }

    fn number<const MAX: usize>(
        &self,
        field: &str,
        cli: Option<usize>,
        default: crate::download::transfer::Concurrency<MAX>,
    ) -> Result<Setting<crate::download::transfer::Concurrency<MAX>>> {
        let (number, source) = if let Some(number) = cli {
            (Some(number), Source::Cli)
        } else if let Some(input) = self.string(field, None)? {
            (input.value.parse::<usize>().ok(), input.source)
        } else {
            return Ok(Setting::new(default, Source::Default));
        };
        let number = number.context("concurrency must be an integer")?;
        let number =
            crate::download::transfer::Concurrency::try_from(number).with_context(|| {
                format!(
                    "{field} from {} must be an integer between 1 and {MAX}",
                    source.name()
                )
            })?;
        Ok(Setting::new(number, source))
    }

    fn url(
        &self,
        field: &str,
        cli: Option<String>,
        proxy: bool,
    ) -> Result<Setting<Option<reqwest::Url>>> {
        let Some(input) = self.string(field, cli.map(Into::into))? else {
            return Ok(Setting::new(None, Source::Default));
        };
        let url = parse_url(&input.value, field, proxy)?;
        Ok(Setting::new(Some(url), input.source))
    }
}

fn require_absolute(path: &Path, field: &str) -> Result<()> {
    if !path.is_absolute() {
        bail!("{field} must be an absolute path");
    }
    Ok(())
}

pub fn absolute(path: &Path, base: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("path must not be empty");
    }
    // Joining does not expand '~', environment variables, or command substitutions.
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    require_absolute(&path, "resolved path")?;
    Ok(path)
}

#[cfg(test)]
mod tests;
