use std::ffi::OsStr;
use std::path::{Component, Path, Prefix};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathKey(Vec<Vec<u8>>);

impl PathKey {
    pub fn starts_with(&self, base: &Self) -> bool {
        self.0.starts_with(&base.0)
    }

    pub fn digest_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for component in &self.0 {
            bytes.extend_from_slice(&(component.len() as u64).to_le_bytes());
            bytes.extend_from_slice(component);
        }
        bytes
    }
}

/// Lexical identity only. Resolve links first when comparing physical locations.
pub fn path_key(path: &Path) -> PathKey {
    let path = super::forms::native(path);
    PathKey(
        path.components()
            .filter(|part| !matches!(part, Component::CurDir))
            .map(|part| match part {
                Component::Prefix(prefix) => match prefix.kind() {
                    Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
                        vec![0, drive.to_ascii_lowercase()]
                    }
                    Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                        let server = key_text(server);
                        let mut bytes = vec![1];
                        bytes.extend_from_slice(&(server.len() as u64).to_le_bytes());
                        bytes.extend(server);
                        bytes.extend(key_text(share));
                        bytes
                    }
                    _ => [vec![2], key_text(prefix.as_os_str())].concat(),
                },
                Component::RootDir => vec![3],
                Component::ParentDir => vec![4],
                Component::Normal(value) => [vec![5], key_text(value)].concat(),
                Component::CurDir => unreachable!(),
            })
            .collect(),
    )
}

fn key_text(value: &OsStr) -> Vec<u8> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let folded = value
            .to_str()
            .map(|text| std::ffi::OsString::from(text.to_lowercase()));
        folded
            .as_deref()
            .unwrap_or(value)
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[cfg(not(windows))]
    {
        value.as_encoded_bytes().to_vec()
    }
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}
