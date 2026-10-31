use crate::download::digest::validate_digest;
use anyhow::{bail, Result};
use reqwest::Url;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: String,
    pub directory: PathBuf,
    pub file_name: String,
    /// Empty means no trusted expected digest: never reuse a cache, old file or old fragments.
    pub sha256: String,
}

impl DownloadRequest {
    pub(super) fn target(&self) -> PathBuf {
        self.directory.join(&self.file_name)
    }

    pub(super) fn validate(&self) -> Result<Url> {
        crate::fs::validate_relative(Path::new(&self.file_name))?;
        if Path::new(&self.file_name).components().count() != 1
            || self.file_name.eq_ignore_ascii_case(".hanamori-parts")
            || self.file_name.contains(['?', '*', '<', '>', '"', '|'])
            || self.file_name.ends_with(['.', ' '])
        {
            bail!("payload must have a single filename outside reserved workspace metadata");
        }
        let stem = self
            .file_name
            .split('.')
            .next()
            .unwrap()
            .to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            bail!("payload filename is a reserved Windows device name");
        }
        if !self.sha256.is_empty() {
            validate_digest(&self.sha256)?;
        }
        crate::download::network::parse_url(&self.url, "download", false)
    }
}
