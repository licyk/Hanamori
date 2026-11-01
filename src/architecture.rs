use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }

    pub const fn package(self) -> &'static str {
        match self {
            Self::X64 => "X64",
            Self::Arm64 => "ARM64",
        }
    }

    pub const fn platform(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "ARM64",
        }
    }

    pub const fn host_directory(self) -> &'static str {
        match self {
            Self::X64 => "Hostx64",
            Self::Arm64 => "Hostarm64",
        }
    }

    pub const fn msbuild_bin(self) -> &'static str {
        match self {
            Self::X64 => "MSBuild/Current/Bin/amd64",
            Self::Arm64 => "MSBuild/Current/Bin/arm64",
        }
    }

    pub const fn release_target(self) -> &'static str {
        match self {
            Self::X64 => "x86_64-pc-windows-msvc",
            Self::Arm64 => "aarch64-pc-windows-msvc",
        }
    }

    pub fn native() -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
            let (mut process, mut native) = (0, 0);
            if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } != 0 {
                if native == 0xaa64 {
                    return Self::Arm64;
                }
                if native == 0x8664 {
                    return Self::X64;
                }
            }
        }
        if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::X64
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Arch {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "x64" | "amd64" | "x86_64" => Ok(Self::X64),
            "arm64" | "aarch64" => Ok(Self::Arm64),
            _ => Err(format!(
                "unsupported architecture {value}; expected x64 or arm64"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Architecture {
    pub host: Arch,
    pub target: Arch,
}

impl Architecture {
    pub const X64: Self = Self {
        host: Arch::X64,
        target: Arch::X64,
    };
    pub const ALL: [Self; 4] = [
        Self::X64,
        Self {
            host: Arch::X64,
            target: Arch::Arm64,
        },
        Self {
            host: Arch::Arm64,
            target: Arch::X64,
        },
        Self {
            host: Arch::Arm64,
            target: Arch::Arm64,
        },
    ];

    pub fn native() -> Self {
        let host = Arch::native();
        Self { host, target: host }
    }

    pub fn key_suffix(self) -> String {
        format!("_host-{}_target-{}", self.host, self.target)
    }

    pub fn ensure_runnable(self) -> Result<()> {
        if cfg!(windows) && Arch::native() == Arch::X64 && self.host == Arch::Arm64 {
            bail!(
                "an ARM64-hosted compiler cannot run on x64 Windows; select x64/{}",
                self.target
            );
        }
        Ok(())
    }
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.host, self.target)
    }
}

impl FromStr for Architecture {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, String> {
        let (host, target) = value
            .split_once('/')
            .ok_or("expected HOST/TARGET, such as x64/arm64")?;
        Ok(Self {
            host: host.parse()?,
            target: target.parse()?,
        })
    }
}
