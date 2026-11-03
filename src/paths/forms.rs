use std::fmt;
use std::path::{Path, PathBuf};

pub fn native(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::path::{Component, Prefix};
        let mut components = path.components();
        if let Some(Component::Prefix(prefix)) = components.next() {
            let mut result = match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
                Prefix::VerbatimUNC(server, share) => {
                    let mut root = OsString::from(r"\\");
                    root.push(server);
                    root.push(r"\");
                    root.push(share);
                    root.push(r"\");
                    PathBuf::from(root)
                }
                _ => return path.to_owned(),
            };
            for component in components {
                if !matches!(component, Component::RootDir) {
                    result.push(component.as_os_str());
                }
            }
            return result;
        }
    }
    path.to_owned()
}

pub struct NativeDisplay(PathBuf);

impl fmt::Display for NativeDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(formatter)
    }
}

pub trait PathExt {
    fn native_display(&self) -> NativeDisplay;
    fn native_text(&self) -> std::io::Result<String>;
}

impl<T: AsRef<Path> + ?Sized> PathExt for T {
    fn native_display(&self) -> NativeDisplay {
        NativeDisplay(native(self.as_ref()))
    }

    fn native_text(&self) -> std::io::Result<String> {
        native(self.as_ref())
            .into_os_string()
            .into_string()
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "path is not valid Unicode")
            })
    }
}

/// Drive/UNC spelling for Git Bash and MSYS2, not a general Cygwin/WSL resolver.
pub fn to_posix(value: &Path) -> String {
    let text = native(value).to_string_lossy().into_owned();
    let Some((drive, rest)) = text.split_once(':') else {
        return text.replace('\\', "/");
    };
    if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
        return text.replace('\\', "/");
    }
    let rest = rest.replace('\\', "/");
    format!(
        "/{}/{}",
        drive.to_ascii_lowercase(),
        rest.strip_prefix('/').unwrap_or(&rest)
    )
}
