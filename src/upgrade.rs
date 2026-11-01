//! Explicit self-updates from a GitHub release, independent of toolchain management.

use crate::download::transfer::{validate_digest, DownloadRequest, Downloader};
use crate::{fs::with_suffix, identity, locking::FileLock};
use anyhow::{bail, ensure, Context, Result};
use reqwest::{Client, StatusCode, Url};
use semver::Version;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;

pub const RELEASE_URL: &str = "https://api.github.com/repos/licyk/Hanamori/releases/latest";
pub const ASSET_NAME: &str = "hanamori-x86_64-pc-windows-msvc.exe";

#[derive(Clone, Copy)]
pub enum Mode {
    Check,
    Upgrade { force: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Available,
    UpToDate,
    Ahead,
    Updated,
}

#[derive(Debug)]
pub struct Report {
    pub current: Version,
    pub latest: Version,
    pub executable: PathBuf,
    pub status: Status,
    pub backup: Option<PathBuf>,
}

pub struct Updater {
    client: Client,
    downloader: Downloader,
    endpoint: Url,
    token: Option<reqwest::header::HeaderValue>,
}

impl Updater {
    /// The CLI fixes endpoint and executable; explicit inputs let tests use isolated copies.
    pub fn new(
        endpoint: Url,
        proxy: Option<&Url>,
        token: Option<&str>,
        progress: bool,
    ) -> Result<Self> {
        let mut client = Client::builder()
            .user_agent(identity::USER_AGENT)
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(60));
        if let Some(proxy) = proxy {
            client = client.proxy(reqwest::Proxy::all(proxy.clone())?);
        }
        let token = token
            .filter(|token| !token.is_empty())
            .map(|token| {
                let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                    .context("GITHUB_TOKEN is not a valid HTTP header value")?;
                value.set_sensitive(true);
                Ok::<_, anyhow::Error>(value)
            })
            .transpose()?;
        Ok(Self {
            client: client.build()?,
            downloader: Downloader::new(proxy)?
                .with_connections(1.try_into()?)
                .with_progress(progress),
            endpoint,
            token,
        })
    }

    pub async fn run(&self, executable: &Path, current: Version, mode: Mode) -> Result<Report> {
        let release = self.latest().await?;
        let executable =
            std::fs::canonicalize(executable).context("cannot locate the running executable")?;
        let status = match current.cmp_precedence(&release.version) {
            std::cmp::Ordering::Less => Status::Available,
            std::cmp::Ordering::Equal => Status::UpToDate,
            std::cmp::Ordering::Greater => Status::Ahead,
        };
        let mut report = Report {
            current,
            latest: release.version,
            executable,
            status,
            backup: None,
        };
        let Mode::Upgrade { force } = mode else {
            return Ok(report);
        };
        ensure!(
            report.status != Status::Ahead,
            "refusing to downgrade Hanamori {} to {}; --force only reinstalls the same version",
            report.current,
            report.latest
        );
        if report.status == Status::UpToDate && !force {
            return Ok(report);
        }

        let _lock = FileLock::acquire(&with_suffix(&report.executable, ".update.lock"))?;
        let parent = report
            .executable
            .parent()
            .context("executable has no parent directory")?;
        let staging = tempfile::Builder::new()
            .prefix(".hanamori-update-")
            .tempdir_in(parent)
            .context(
                "cannot create update staging directory; check installation directory permissions",
            )?;
        tracing::info!("Downloading Hanamori {}", report.latest);
        let files = self
            .downloader
            .download_all(&[DownloadRequest {
                url: release.url.to_string(),
                directory: staging.path().to_owned(),
                file_name: "hanamori.exe".into(),
                sha256: release.sha256,
            }])
            .await?;
        let downloaded = &files[0];
        ensure!(
            downloaded.size == release.size,
            "downloaded executable size differs from the release asset"
        );
        #[cfg(unix)]
        std::fs::set_permissions(
            &downloaded.path,
            std::fs::metadata(&report.executable)?.permissions(),
        )?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&downloaded.path)?
            .sync_all()?;
        verify_executable(&downloaded.path, &report.latest, Duration::from_secs(10)).await?;
        report.backup = Some(replace_executable(&report.executable, &downloaded.path)?);
        report.status = Status::Updated;
        Ok(report)
    }

    async fn latest(&self) -> Result<Release> {
        let mut request = self
            .client
            .get(self.endpoint.clone())
            .header(reqwest::header::ACCEPT, "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.header(reqwest::header::AUTHORIZATION, token.clone());
        }
        let response = request
            .send()
            .await
            .context("cannot query GitHub releases")?;
        match response.status() {
            StatusCode::NOT_FOUND => bail!("no published stable Hanamori release is available"),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => bail!("GitHub rejected the release query ({}); check API rate limits and GITHUB_TOKEN permissions", response.status()),
            StatusCode::UNAUTHORIZED => bail!("GitHub rejected GITHUB_TOKEN"),
            _ => {}
        }
        let bytes = response
            .error_for_status()
            .context("GitHub release query failed")?
            .bytes()
            .await?;
        let release: ReleaseResponse =
            serde_json::from_slice(&bytes).context("invalid GitHub release response")?;
        ensure!(
            !release.draft && !release.prerelease,
            "self-update requires a published stable release"
        );
        let version = Version::parse(
            release
                .tag_name
                .strip_prefix('v')
                .context("release tag must start with v")?,
        )
        .context("release tag is not a semantic version")?;
        ensure!(
            version.pre.is_empty(),
            "self-update requires a stable version tag"
        );
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == ASSET_NAME)
            .with_context(|| format!("release {} has no {ASSET_NAME} asset", release.tag_name))?;
        let sha256 = asset
            .digest
            .as_deref()
            .and_then(|value| value.strip_prefix("sha256:"))
            .context("release asset has no SHA-256 digest; self-update was cancelled")?
            .to_owned();
        validate_digest(&sha256).context("invalid release asset SHA-256 digest")?;
        ensure!(asset.size > 0, "release executable is empty");
        let url = crate::download::network::parse_url(
            &asset.browser_download_url,
            "release asset",
            false,
        )?;
        Ok(Release {
            version,
            url,
            sha256,
            size: asset.size,
        })
    }
}

#[derive(Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<AssetResponse>,
}

#[derive(Deserialize)]
struct AssetResponse {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

struct Release {
    version: Version,
    url: Url,
    sha256: String,
    size: u64,
}

async fn verify_executable(path: &Path, expected: &Version, timeout: Duration) -> Result<()> {
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW: verification must not flash a console.
    let mut child = command
        .spawn()
        .context("cannot run the downloaded Hanamori executable")?;
    let mut stdout = child.stdout.take().expect("piped version output");
    let mut bytes = Vec::new();
    let result = tokio::time::timeout(timeout, async {
        let (status, _) = tokio::try_join!(child.wait(), stdout.read_to_end(&mut bytes))?;
        Ok::<_, std::io::Error>(status)
    })
    .await;
    let status = match result {
        Ok(result) => result.context("cannot read downloaded executable version")?,
        Err(_) => {
            child
                .kill()
                .await
                .context("cannot stop timed-out version check")?;
            bail!(
                "downloaded executable version check timed out; current executable was preserved"
            );
        }
    };
    ensure!(
        status.success(),
        "downloaded executable version check failed ({status})"
    );
    let text =
        std::str::from_utf8(&bytes).context("downloaded executable returned an invalid version")?;
    ensure!(
        text.trim() == format!("hanamori {expected}"),
        "downloaded executable does not report Hanamori {expected}; self-update was cancelled"
    );
    Ok(())
}

fn replace_executable(target: &Path, staged: &Path) -> Result<PathBuf> {
    let backup = with_suffix(target, ".outdated");
    match crate::file_usage::remove_file(&backup) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "cannot remove previous update backup {}; close processes using it and retry",
                    backup.display()
                )
            })
        }
    }
    std::fs::rename(target, &backup).with_context(|| {
        format!(
            "cannot rename running executable {}; check permissions and file sharing",
            target.display()
        )
    })?;
    if let Err(error) = std::fs::rename(staged, target) {
        if let Err(restore) = std::fs::rename(&backup, target) {
            bail!("cannot install new executable: {error}; rollback also failed: {restore}. Restore {} to {} manually", backup.display(), target.display());
        }
        return Err(error)
            .context("cannot install new executable; the original executable was restored");
    }
    // The old image may still be mapped by this process on Windows. Keep it until the next update.
    Ok(backup)
}

#[cfg(test)]
mod tests;
