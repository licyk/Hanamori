#[cfg(windows)]
use anyhow::Context;
use anyhow::{ensure, Result};
use shell_quote::{Bash, QuoteRefExt};
use std::ffi::OsStr;

pub fn posix_literal(value: &OsStr) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        ensure!(
            !value.as_bytes().contains(&0),
            "shell value contains a NUL byte"
        );
        Ok(value.quoted(Bash))
    }
    #[cfg(windows)]
    {
        let value = value
            .to_str()
            .context("Bash/Zsh cannot represent this native UTF-16 value")?;
        ensure!(!value.contains('\0'), "shell value contains a NUL byte");
        Ok(value.quoted(Bash))
    }
}

pub fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
