//! Verified payload transfers, resumable ranges, mirror fallback and a shared SHA-256 cache.

use crate::download::cache::{matches_digest, regular_or_missing, CacheEntry};
pub use crate::download::digest::{compute_sha256, validate_digest};
use crate::download::ranges::{
    fetch_range, part_path, prepare_parts, probe, split_ranges, write_response, InvalidPayload,
    Probe,
};
use crate::fs::ensure_safe_path;
use crate::locking::FileLock;
use crate::progress::configure;
use anyhow::{bail, Context, Result};
use futures_util::{stream, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget};
use reqwest::{Client, Url};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub use crate::download::request::DownloadRequest;

pub const MAX_FILE_WORKERS: usize = 256;
pub const MAX_CONNECTIONS: usize = 64;

pub const DEFAULT_FILE_WORKERS: usize = 4;
pub const DEFAULT_CONNECTIONS: usize = 8;
const RANGE_THRESHOLD: u64 = 1 << 20;

#[derive(Debug, PartialEq, Eq)]
pub struct DownloadedFile {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}

/// A validated concurrency limit, shared by configuration and the downloader.
#[derive(Clone, Copy, Debug)]
pub struct Concurrency<const MAX: usize>(usize);

impl<const MAX: usize> Concurrency<MAX> {
    pub fn get(self) -> usize {
        self.0
    }
}

impl<const MAX: usize> TryFrom<usize> for Concurrency<MAX> {
    type Error = anyhow::Error;
    fn try_from(value: usize) -> Result<Self> {
        anyhow::ensure!(
            (1..=MAX).contains(&value),
            "must be an integer between 1 and {MAX}"
        );
        Ok(Self(value))
    }
}

pub type FileWorkers = Concurrency<MAX_FILE_WORKERS>;
pub type Connections = Concurrency<MAX_CONNECTIONS>;
pub const DEFAULT_WORKERS: FileWorkers = Concurrency(DEFAULT_FILE_WORKERS);
pub const DEFAULT_RANGES: Connections = Concurrency(DEFAULT_CONNECTIONS);

pub struct Downloader {
    client: Client,
    file_workers: usize,
    connections: usize,
    retries: usize,
    cache_directory: Option<PathBuf>,
    mirror: Option<Url>,
    show_progress: bool,
}

impl Downloader {
    pub fn new(proxy: Option<&Url>) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent(crate::identity::USER_AGENT)
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(30 * 60))
            .no_gzip();
        if let Some(proxy) = proxy {
            builder = builder
                .proxy(reqwest::Proxy::all(proxy.clone()).context("proxy is not a usable URL")?);
        }
        Ok(Self {
            client: builder.build().context("failed to build the HTTP client")?,
            file_workers: DEFAULT_FILE_WORKERS,
            connections: DEFAULT_CONNECTIONS,
            retries: 5,
            cache_directory: None,
            mirror: None,
            show_progress: true,
        })
    }

    pub fn with_file_workers(mut self, workers: FileWorkers) -> Self {
        self.file_workers = workers.get();
        self
    }
    pub fn with_connections(mut self, connections: Connections) -> Self {
        self.connections = connections.get();
        self
    }
    pub fn with_progress(mut self, visible: bool) -> Self {
        self.show_progress = visible;
        self
    }
    /// Retries after a range transfer failure. Zero retries indefinitely.
    pub fn with_retries(mut self, retries: usize) -> Self {
        self.retries = retries;
        self
    }
    pub fn with_cache_directory(mut self, directory: Option<&Path>) -> Self {
        self.cache_directory = directory.map(Path::to_path_buf);
        self
    }
    /// Prefix the original payload path with this mirror's path. Only mirror credentials/query
    /// are sent to the mirror; source credentials and query tokens are never forwarded.
    pub fn with_mirror(mut self, mirror: Option<&Url>) -> Self {
        self.mirror = mirror.cloned();
        self
    }

    /// Each target appears once in the result, including cache hits. All running transfers are
    /// awaited on failure; cancellation drops borrowed futures rather than detaching tasks.
    pub async fn download_all(&self, requests: &[DownloadRequest]) -> Result<Vec<DownloadedFile>> {
        let urls = requests
            .iter()
            .map(DownloadRequest::validate)
            .collect::<Result<Vec<_>>>()?;
        let mut targets = BTreeSet::new();
        let mut groups: BTreeMap<String, Vec<(usize, DownloadRequest, Url)>> = BTreeMap::new();
        for (index, (request, url)) in requests.iter().zip(urls).enumerate() {
            std::fs::create_dir_all(&request.directory)?;
            let mut request = request.clone();
            request.directory = crate::paths::canonicalize(&request.directory)?;
            if !targets.insert(crate::paths::path_key(&request.target())) {
                bail!("duplicate payload destination");
            }
            // Same-digest jobs within this batch run sequentially and reuse their first cache write.
            let key = if self.cache_directory.is_some() && !request.sha256.is_empty() {
                request.sha256.to_ascii_lowercase()
            } else {
                format!("uncached-{index}")
            };
            groups.entry(key).or_default().push((index, request, url));
        }
        let multi = MultiProgress::with_draw_target(if self.show_progress {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        });
        let progress = multi.add(build_progress(requests.len() as u64));
        let mut work = stream::iter(groups.into_values())
            .map(|group| {
                let multi = &multi;
                let progress = &progress;
                async move {
                    let mut failures = Vec::new();
                    let mut completed = Vec::new();
                    for (index, request, url) in group {
                        match self.fetch(&request, &url, multi).await {
                            Ok(sha256) => {
                                completed.push((index, sha256));
                                progress.inc(1);
                            }
                            Err(error) => {
                                failures.push(format!("{}: {error:#}", request.file_name));
                            }
                        }
                    }
                    (completed, failures)
                }
            })
            .buffer_unordered(self.file_workers);
        let mut failures = Vec::new();
        let mut completed = Vec::new();
        while let Some((files, errors)) = work.next().await {
            completed.extend(files);
            failures.extend(errors);
        }
        if !failures.is_empty() {
            progress.abandon_with_message("Download failed");
            let _ = multi.clear();
            bail!(
                "failed to download {} payload(s): {}",
                failures.len(),
                failures.join("; ")
            );
        }
        progress.finish_and_clear();
        completed.sort_unstable_by_key(|(index, _)| *index);
        let files = completed
            .into_iter()
            .map(|(index, sha256)| {
                let path = requests[index].target();
                let size = std::fs::metadata(&path)?.len();
                Ok(DownloadedFile { path, sha256, size })
            })
            .collect::<Result<Vec<_>>>()?;
        tracing::info!(
            "Download complete: {} payloads, {} bytes in {:.2}s (including reused files)",
            files.len(),
            files.iter().map(|file| file.size).sum::<u64>(),
            progress.elapsed().as_secs_f64()
        );
        Ok(files)
    }

    async fn fetch(
        &self,
        request: &DownloadRequest,
        url: &Url,
        multi: &MultiProgress,
    ) -> Result<String> {
        let target = request.target();
        regular_or_missing(&target)?;
        let key = format!(
            "{:x}",
            Sha256::digest(crate::paths::path_key(Path::new(&request.file_name)).digest_bytes())
        );
        let locks = Path::new(".hanamori-parts/locks");
        ensure_safe_path(&request.directory, &locks.join(format!("{key}.lock")))?;
        std::fs::create_dir_all(request.directory.join(locks))?;
        let _target_lock =
            FileLock::acquire(&request.directory.join(locks).join(format!("{key}.lock")))?;
        let cache = self
            .cache_directory
            .as_ref()
            .filter(|_| !request.sha256.is_empty())
            .map(|root| CacheEntry::acquire(root, &request.sha256))
            .transpose()?;
        if matches_digest(&target, &request.sha256).await? {
            if let Some(cache) = &cache {
                cache.publish(&target).await?;
            }
            return Ok(request.sha256.to_ascii_lowercase());
        }
        if let Some(cache) = &cache {
            if cache.restore(&target).await? {
                return Ok(request.sha256.to_ascii_lowercase());
            }
        }
        let work = Path::new(".hanamori-parts/work").join(key);
        ensure_safe_path(&request.directory, &work)?;
        let work = request.directory.join(work);
        if let Some(mirror) = &self.mirror {
            let mirrored = mirror_url(mirror, url);
            if mirrored != request.url {
                match self.transfer(request, &mirrored, &work, multi).await {
                    Ok(sha256) => {
                        if let Some(cache) = &cache {
                            cache.publish(&target).await?;
                        }
                        return Ok(sha256);
                    }
                    Err(error) => multi.suspend(|| {
                        tracing::warn!(
                            "mirror transfer failed: {error:#}; trying the original payload URL"
                        )
                    }),
                }
            }
        }
        let sha256 = self.transfer(request, &request.url, &work, multi).await?;
        if let Some(cache) = &cache {
            cache.publish(&target).await?;
        }
        Ok(sha256)
    }

    async fn transfer(
        &self,
        request: &DownloadRequest,
        url: &str,
        work: &Path,
        multi: &MultiProgress,
    ) -> Result<String> {
        let probe = probe(&self.client, url).await?;
        let length = match &probe {
            Probe::Ranges(length) => Some(*length),
            Probe::Full(response) => response.content_length(),
        };
        let bar = multi.add(ProgressBar::new(length.unwrap_or(0)));
        configure(&bar, "{spinner:.green} [{elapsed_precise}] [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta}) {msg}");
        bar.set_message(request.file_name.clone());
        let result = async {
            let temporary = tempfile::Builder::new()
                .prefix("hanamori-payload-")
                .tempfile_in(&request.directory)?
                .into_temp_path();
            let mut output = tokio::fs::File::create(&temporary).await?;
            let actual_length = match probe {
                Probe::Full(response) => {
                    // Servers without ranges cannot safely resume previous partial responses.
                    let count = write_response(response, &mut output, length, &bar).await?;
                    if let Some(expected) = length {
                        if count != expected {
                            bail!(InvalidPayload("incomplete response body"));
                        }
                    }
                    count
                }
                Probe::Ranges(length) => {
                    let ranges = split_ranges(
                        length,
                        if length >= RANGE_THRESHOLD {
                            self.connections
                        } else {
                            1
                        },
                    );
                    prepare_parts(work, request, url, length, &ranges)?;
                    let mut transfers = stream::iter(ranges.iter().enumerate())
                        .map(|(index, &(start, end))| {
                            let part = part_path(work, index);
                            let bar = &bar;
                            async move {
                                fetch_range(
                                    &self.client,
                                    url,
                                    start,
                                    end,
                                    length,
                                    &part,
                                    bar,
                                    self.retries,
                                )
                                .await
                            }
                        })
                        .buffer_unordered(self.connections);
                    let mut failure = None;
                    while let Some(result) = transfers.next().await {
                        if let Err(error) = result {
                            if failure.is_none() {
                                failure = Some(error);
                            }
                        }
                    }
                    if let Some(error) = failure {
                        return Err(error);
                    }
                    for (index, &(start, end)) in ranges.iter().enumerate() {
                        let part = part_path(work, index);
                        if !regular_or_missing(&part)?
                            || std::fs::metadata(&part)?.len() != end - start + 1
                        {
                            bail!(InvalidPayload("invalid completed fragment length"));
                        }
                        let mut input = tokio::fs::File::open(&part).await?;
                        tokio::io::copy(&mut input, &mut output).await?;
                    }
                    length
                }
            };
            output.flush().await?;
            output.sync_all().await?;
            drop(output);
            if actual_length == 0 || tokio::fs::metadata(&temporary).await?.len() != actual_length {
                bail!(InvalidPayload("empty or incomplete payload"));
            }
            let sha256 = compute_sha256(&temporary).await?;
            if !request.sha256.is_empty() && !sha256.eq_ignore_ascii_case(&request.sha256) {
                bail!(InvalidPayload("payload checksum mismatch"));
            }
            regular_or_missing(&request.target())?;
            temporary
                .persist(request.target())
                .map_err(|error| error.error)
                .context("cannot publish verified download")?;
            Ok(sha256)
        }
        .await;
        bar.finish_and_clear();
        if (result.is_ok()
            || result
                .as_ref()
                .is_err_and(|error: &anyhow::Error| error.is::<InvalidPayload>()))
            && work.try_exists()?
        {
            std::fs::remove_dir_all(work)?;
        }
        result
    }
}

fn mirror_url(mirror: &Url, source: &Url) -> String {
    let mut url = mirror.clone();
    url.set_path(&format!(
        "{}{}",
        mirror.path().trim_end_matches('/'),
        source.path()
    ));
    url.to_string()
}

fn build_progress(files: u64) -> ProgressBar {
    let bar = ProgressBar::new(files);
    configure(&bar, "{spinner:.green} [{elapsed_precise}] [{bar:32.cyan/blue}] {pos}/{len} payloads ({per_sec}, {eta})");
    bar
}

#[cfg(test)]
mod tests;
