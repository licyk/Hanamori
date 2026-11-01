//! Channel and package manifest retrieval.

use crate::catalog::manifest::{self, ManifestPackageSet, ManifestPayload};
use crate::catalog::selector;
use crate::progress::{configure, progress_spinner};
use anyhow::{bail, Context, Result};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Downloads the manifests and returns everything a listing needs.
pub async fn resolve(
    manifest_url: &str,
    proxy: Option<&reqwest::Url>,
    strict_manifest_integrity: bool,
    show_progress: bool,
    arch: crate::architecture::Architecture,
) -> Result<(ManifestPackageSet, String)> {
    let client = build_client(proxy)?;
    let payload = fetch_visual_studio_manifest(&client, manifest_url, show_progress).await?;
    let packages =
        fetch_packages(&client, &payload, strict_manifest_integrity, show_progress).await?;
    let toolset = selector::find_toolset_version(&packages, arch)
        .context("the manifest does not offer any MSVC toolset")?;
    Ok((packages, toolset))
}

pub(super) fn build_client(proxy: Option<&reqwest::Url>) -> Result<Client> {
    let mut builder = Client::builder()
        .user_agent(crate::identity::USER_AGENT)
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(10 * 60))
        .gzip(true);

    if let Some(proxy) = proxy {
        builder =
            builder.proxy(reqwest::Proxy::all(proxy.clone()).context("proxy is not a usable URL")?);
    }

    builder.build().context("failed to build the HTTP client")
}

pub(super) async fn fetch_visual_studio_manifest(
    client: &Client,
    manifest_url: &str,
    show_progress: bool,
) -> Result<ManifestPayload> {
    let body = download_bytes(client, manifest_url, show_progress).await?;
    manifest::parse_visual_studio_manifest_payload(std::str::from_utf8(&body)?)
}

pub(super) async fn fetch_packages(
    client: &Client,
    payload: &ManifestPayload,
    strict_manifest_integrity: bool,
    show_progress: bool,
) -> Result<ManifestPackageSet> {
    let body = download_bytes(client, &payload.url, show_progress).await?;
    check_package_manifest(&body, payload, strict_manifest_integrity)?;
    manifest::parse_packages(std::str::from_utf8(&body)?)
}

fn check_package_manifest(
    bytes: &[u8],
    payload: &ManifestPayload,
    strict_manifest_integrity: bool,
) -> Result<()> {
    let result = verify_package_manifest(bytes, payload);
    if strict_manifest_integrity {
        result
    } else {
        if let Err(error) = result {
            tracing::warn!("{error:#}; continuing by default; use --strict-manifest-integrity to reject inconsistent package manifests; payload verification remains enabled");
        }
        Ok(())
    }
}

fn verify_package_manifest(bytes: &[u8], payload: &ManifestPayload) -> Result<()> {
    if !payload.sha256.is_empty() {
        crate::download::transfer::validate_digest(&payload.sha256)
            .context("invalid package manifest SHA-256")?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&payload.sha256) {
            bail!("package manifest checksum mismatch: expected SHA-256 {}; received SHA-256 {actual} ({} bytes)", payload.sha256.to_ascii_lowercase(), bytes.len());
        }
    }
    if payload.size != 0 && payload.size != bytes.len() as u64 {
        bail!(
            "package manifest size mismatch: expected {} bytes; received {} bytes",
            payload.size,
            bytes.len()
        );
    }
    Ok(())
}

async fn download_bytes(client: &Client, url: &str, show_progress: bool) -> Result<Vec<u8>> {
    let progress = progress_spinner("Waiting for manifest response", show_progress);
    let result = async {
        let mut response = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("cannot fetch {url}"))?;

        if !response.status().is_success() {
            bail!("{} answered {}", url, response.status());
        }

        // The channel's advertised size can differ from the response. Gzip decoding
        // may also leave the decoded length unknown, so use the HTTP client's length.
        let template = if let Some(length) = response.content_length() {
            progress.set_length(length);
            "{spinner:.green} [{elapsed_precise}] {msg} [{bar:24.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})"
        } else {
            "{spinner:.green} [{elapsed_precise}] {msg} {bytes} ({bytes_per_sec})"
        };
        configure(&progress, template);
        progress.set_message("Downloading manifest");
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .with_context(|| format!("cannot read the response of {url}"))?
        {
            bytes.extend_from_slice(&chunk);
            progress.inc(chunk.len() as u64);
        }
        Ok(bytes)
    }
    .await;
    progress.finish_and_clear();
    if let Ok(bytes) = &result {
        tracing::info!(
            "Manifest download complete: {} bytes in {:.2}s",
            bytes.len(),
            progress.elapsed().as_secs_f64()
        );
    }
    result
}

#[cfg(test)]
mod tests;
