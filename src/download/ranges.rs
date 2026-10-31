//! Probing range support and transferring resumable byte-range fragments.

use crate::download::cache::regular_or_missing;
use crate::download::request::DownloadRequest;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::ProgressBar;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const PART_SUFFIX: &str = ".part";

#[derive(Debug)]
pub(super) struct InvalidPayload(pub(super) &'static str);
impl std::fmt::Display for InvalidPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for InvalidPayload {}

#[derive(Debug)]
struct PermanentFailure(String);
impl std::fmt::Display for PermanentFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for PermanentFailure {}

fn check_status(response: &Response, url: &str) -> Result<()> {
    if !response.status().is_success() {
        let message = format!("{} answered {}", url, response.status());
        if response.status().is_client_error()
            && !matches!(
                response.status(),
                StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
            )
        {
            bail!(PermanentFailure(message));
        }
        bail!("{message}");
    }
    Ok(())
}

pub(super) enum Probe {
    Ranges(u64),
    Full(Response),
}

pub(super) async fn probe(client: &Client, url: &str) -> Result<Probe> {
    let response = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .with_context(|| format!("cannot reach {url}"))?;
    check_status(&response, url)?;
    if response.status() == StatusCode::OK {
        return Ok(Probe::Full(response));
    }
    let (start, end, length) = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|header| header.to_str().ok())
        .and_then(parse_content_range)
        .context(InvalidPayload("missing or invalid range probe header"))?;
    if response.status() != StatusCode::PARTIAL_CONTENT || start != 0 || end != 0 {
        bail!(InvalidPayload(
            "range probe did not return the requested byte"
        ));
    }
    let mut stream = response.bytes_stream();
    let mut count = 0;
    while let Some(chunk) = stream.next().await {
        count += chunk
            .with_context(|| format!("cannot read probe {url}"))?
            .len();
        if count > 1 {
            bail!(InvalidPayload("range probe returned excess bytes"));
        }
    }
    if count != 1 {
        bail!(InvalidPayload("range probe returned no data"));
    }
    Ok(Probe::Ranges(length))
}

pub(super) fn split_ranges(length: u64, count: usize) -> Vec<(u64, u64)> {
    let count = count.clamp(1, length.max(1).min(usize::MAX as u64) as usize);
    let chunk = length.div_ceil(count as u64);
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < length {
        let end = start.saturating_add(chunk - 1).min(length - 1);
        ranges.push((start, end));
        start = end + 1;
    }
    ranges
}

pub(super) fn part_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("{index}{PART_SUFFIX}"))
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Resume {
    schema_version: u32,
    source_key: String,
    length: u64,
    ranges: Vec<(u64, u64)>,
}

pub(super) fn prepare_parts(
    directory: &Path,
    request: &DownloadRequest,
    url: &str,
    length: u64,
    ranges: &[(u64, u64)],
) -> Result<()> {
    let state = Resume {
        schema_version: 1,
        source_key: format!(
            "{:x}",
            Sha256::digest(format!(
                "{}\0{}\0{}",
                request.url,
                url,
                request.sha256.to_ascii_lowercase()
            ))
        ),
        length,
        ranges: ranges.to_vec(),
    };
    let metadata = directory.join("resume.json");
    let reusable = !request.sha256.is_empty()
        && regular_or_missing(&metadata)?
        && std::fs::read(&metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Resume>(&bytes).ok())
            .as_ref()
            == Some(&state);
    if !reusable && directory.try_exists()? {
        std::fs::remove_dir_all(directory)?;
    }
    std::fs::create_dir_all(directory)?;
    crate::fs::atomic_write(&metadata, &serde_json::to_vec(&state)?)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn fetch_range(
    client: &Client,
    url: &str,
    start: u64,
    end: u64,
    total: u64,
    part: &Path,
    progress: &ProgressBar,
    retries: usize,
) -> Result<()> {
    let expected = end - start + 1;
    let mut written = if regular_or_missing(part)? {
        std::fs::metadata(part)?.len()
    } else {
        0
    };
    if written > expected {
        std::fs::remove_file(part)?;
        written = 0;
    }
    progress.inc(written);
    let mut attempts = 0;
    loop {
        if written == expected {
            return Ok(());
        }
        let result = stream_range(
            client,
            url,
            start + written,
            end,
            total,
            written,
            part,
            progress,
        )
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) if error.is::<InvalidPayload>() || error.is::<PermanentFailure>() => {
                return Err(error)
            }
            Err(error) => {
                attempts += 1;
                if retries > 0 && attempts > retries {
                    return Err(error).context("range retry limit reached");
                }
                progress.suspend(|| tracing::warn!("range transfer failed: {error:#}; retrying"));
                tokio::time::sleep(Duration::from_secs(5)).await;
                written = if regular_or_missing(part)? {
                    std::fs::metadata(part)?.len()
                } else {
                    0
                };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_range(
    client: &Client,
    url: &str,
    from: u64,
    to: u64,
    total: u64,
    resume: u64,
    part: &Path,
    progress: &ProgressBar,
) -> Result<()> {
    let response = client
        .get(url)
        .header(RANGE, format!("bytes={from}-{to}"))
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .with_context(|| format!("cannot reach {url}"))?;
    check_status(&response, url)?;
    let actual = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range);
    if response.status() != StatusCode::PARTIAL_CONTENT || actual != Some((from, to, total)) {
        bail!(InvalidPayload("server returned an unexpected byte range"));
    }
    let expected = to - from + 1;
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length != expected)
    {
        bail!(InvalidPayload(
            "range Content-Length does not match the requested bytes"
        ));
    }
    regular_or_missing(part)?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(resume > 0)
        .truncate(resume == 0)
        .open(part)
        .await?;
    let result = write_response(response, &mut file, Some(expected), progress).await;
    // Flush even on a network error so the retry observes all completed writes.
    file.flush().await?;
    let received = result?;
    if received != expected {
        bail!("incomplete range response");
    }
    Ok(())
}

pub(super) async fn write_response(
    response: Response,
    file: &mut tokio::fs::File,
    expected: Option<u64>,
    progress: &ProgressBar,
) -> Result<u64> {
    let url = response.url().clone();
    let mut stream = response.bytes_stream();
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("transfer interrupted for {url}"))?;
        received += chunk.len() as u64;
        if expected.is_some_and(|expected| received > expected) {
            bail!(InvalidPayload("response body exceeds the requested size"));
        }
        file.write_all(&chunk).await?;
        progress.inc(chunk.len() as u64);
    }
    Ok(received)
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (start.parse().ok()?, end.parse().ok()?, total.parse().ok()?);
    (start <= end && end < total).then_some((start, end, total))
}

#[cfg(test)]
mod tests;
