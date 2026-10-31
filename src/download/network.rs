//! Parsed network settings.

use anyhow::{ensure, Context, Result};

/// Only HTTP(S) proxies are enabled by the current reqwest feature set.
pub fn parse_url(value: &str, field: &str, proxy: bool) -> Result<reqwest::Url> {
    let url =
        reqwest::Url::parse(value).with_context(|| format!("invalid {field} URL {value:?}"))?;
    ensure!(
        value.trim() == value
            && !value.chars().any(char::is_control)
            && matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.fragment().is_none()
            && (!proxy || (url.path() == "/" && url.query().is_none())),
        "{field} must be a valid HTTP(S) {} URL; received {value:?}",
        if proxy { "proxy" } else { "mirror" }
    );
    Ok(url)
}
