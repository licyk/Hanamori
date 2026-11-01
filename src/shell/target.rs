//! A toolchain directory bound into generated shell integration.

use std::ffi::OsString;
use std::path::PathBuf;

pub struct DirectoryTarget {
    pub architecture: Option<crate::architecture::Architecture>,
    pub path: PathBuf,
    pub toolset: Option<String>,
    pub sdk: Option<String>,
    pub no_sdk: bool,
}

impl DirectoryTarget {
    pub fn arguments(&self) -> Vec<OsString> {
        let mut arguments = vec!["--dir".into(), self.path.as_os_str().to_owned()];
        for (flag, value) in [("--toolset", &self.toolset), ("--sdk", &self.sdk)] {
            if let Some(value) = value {
                arguments.extend([flag.into(), value.into()]);
            }
        }
        if let Some(architecture) = self.architecture {
            arguments.extend(["--arch".into(), architecture.to_string().into()]);
        }
        if self.no_sdk {
            arguments.push("--no-sdk".into());
        }
        arguments
    }
}
