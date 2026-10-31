use crate::catalog::manifest::ManifestPayload;
use crate::download::transfer::DownloadRequest;
use std::path::PathBuf;

/// A payload together with the directory it is downloaded into.
#[derive(Debug, Clone)]
pub(super) struct PayloadRequest {
    pub(super) role: PayloadRole,
    pub(super) package_kind: String,
    pub(super) payload: ManifestPayload,
    pub(super) directory: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PayloadRole {
    Toolset,
    Sdk,
    Msbuild,
}

impl PayloadRequest {
    pub(super) fn to_download_request(&self) -> DownloadRequest {
        DownloadRequest {
            url: self.payload.url.clone(),
            directory: self.directory.clone(),
            file_name: self.payload.base_name().to_owned(),
            sha256: self.payload.sha256.clone(),
        }
    }

    pub(super) fn path(&self) -> PathBuf {
        self.directory.join(self.payload.base_name())
    }
}

pub(super) fn is_extension(file_name: &str, extension: &str) -> bool {
    file_name
        .rsplit_once('.')
        .is_some_and(|(_, actual)| actual.eq_ignore_ascii_case(extension))
}

#[cfg(test)]
mod tests;
