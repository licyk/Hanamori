use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

pub(super) const REG_SZ: u32 = 1;
pub(super) const REG_EXPAND_SZ: u32 = 2;
pub(super) const REG_BINARY: u32 = 3;
const MARKER: &str = "--hanamori-autorun-v1";
const SEPARATOR: &str = " & ";
const MAX_BYTES: usize = 1024 * 1024;

fn has_marker(text: &str) -> bool {
    text.to_ascii_lowercase().contains("--hanamori-autorun-")
}

/// Raw bytes are retained so empty, missing, and unterminated string values differ.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub autorun: Option<RawValue>,
    pub receipt: Option<RawValue>,
}

pub(super) struct StringValue {
    pub(super) text: String,
    kind: u32,
    terminators: usize,
}

impl StringValue {
    pub(super) fn read(raw: &RawValue) -> Result<Self> {
        ensure!(
            matches!(raw.kind, REG_SZ | REG_EXPAND_SZ),
            "AutoRun must be REG_SZ or REG_EXPAND_SZ; its value was preserved"
        );
        ensure!(
            raw.bytes.len() <= MAX_BYTES && raw.bytes.len().is_multiple_of(2),
            "AutoRun has oversized or incomplete UTF-16 data"
        );
        let mut units: Vec<_> = raw
            .bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let size = units.len();
        while units.last() == Some(&0) {
            units.pop();
        }
        let terminators = size - units.len();
        let text = String::from_utf16(&units).context("AutoRun has invalid UTF-16")?;
        ensure!(
            !text.contains(['\0', '\r', '\n']),
            "AutoRun contains embedded NUL or line breaks; its value was preserved"
        );
        Ok(Self {
            text,
            kind: raw.kind,
            terminators,
        })
    }

    pub(super) fn write(&self, text: &str) -> RawValue {
        RawValue {
            kind: self.kind,
            bytes: text
                .encode_utf16()
                .chain(std::iter::repeat_n(0, self.terminators))
                .flat_map(u16::to_le_bytes)
                .collect(),
        }
    }
}

/// Keep CALL's second expansion out of literal paths. The future loader publisher
/// must choose a representable path or report this error before any registry write.
fn invocation(loader: &str) -> Result<String> {
    let bytes = loader.as_bytes();
    let drive_rooted =
        bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let unc = loader
        .strip_prefix("\\\\")
        .is_some_and(|rest| rest.split('\\').take(3).filter(|p| !p.is_empty()).count() == 3);
    ensure!(
        drive_rooted || unc,
        "CMD startup loader must have an absolute Windows path"
    );
    ensure!(
        !loader.contains(['"', '%', '!', '^'])
            && !loader.chars().any(char::is_control)
            && !has_marker(loader)
            && loader.to_ascii_lowercase().ends_with(".cmd"),
        "CMD startup loader requires a .cmd path without quotes, percent, exclamation, caret, or control characters"
    );
    // Placing our command first avoids changing a user's trailing REM, redirection,
    // conditional chain or parentheses. Never evaluate or re-quote their command.
    Ok(format!("if exist \"{loader}\" call \"{loader}\" {MARKER}"))
}

fn join(command: &str, rest: &str) -> String {
    if rest.is_empty() {
        command.to_owned()
    } else {
        format!("{command}{SEPARATOR}{rest}")
    }
}

fn unowned<'a>(text: &'a str, command: &str) -> Result<&'a str> {
    let tail = text.strip_prefix(command).context(
        "Hanamori's AutoRun command was edited or moved; restore its recorded leading command before retrying",
    )?;
    let tail = if tail.is_empty() {
        tail
    } else {
        tail.strip_prefix(SEPARATOR)
            .context("Hanamori's AutoRun command boundary was edited; no values were changed")?
    };
    ensure!(
        !has_marker(tail),
        "duplicate Hanamori AutoRun marker; no values were changed"
    );
    Ok(tail)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    loader: String,
    original: Option<RawValue>,
    installed: RawValue,
}

impl Receipt {
    pub(super) fn read(raw: &RawValue) -> Result<Self> {
        ensure!(
            raw.kind == REG_BINARY && raw.bytes.len() <= MAX_BYTES,
            "Hanamori AutoRun receipt has an unsupported type or size"
        );
        let receipt: Self = serde_json::from_slice(&raw.bytes)
            .context("Hanamori AutoRun receipt is malformed; no values were changed")?;
        ensure!(
            receipt.schema_version == 1,
            "unsupported AutoRun receipt schema"
        );
        let installed = StringValue::read(&receipt.installed)?;
        let original = receipt
            .original
            .as_ref()
            .map(StringValue::read)
            .transpose()?;
        let rest = original.as_ref().map_or("", |value| value.text.as_str());
        ensure!(
            !has_marker(rest)
                && installed.text == join(&invocation(&receipt.loader)?, rest)
                && original.as_ref().map_or(
                    installed.kind == REG_SZ && installed.terminators == 1,
                    |value| value.kind == installed.kind
                        && value.terminators == installed.terminators
                ),
            "Hanamori AutoRun receipt is inconsistent; no values were changed"
        );
        Ok(receipt)
    }

    fn write(&self) -> Result<RawValue> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_BYTES,
            "Hanamori AutoRun receipt exceeds its size limit"
        );
        Ok(RawValue {
            kind: REG_BINARY,
            bytes,
        })
    }
}

/// Planning is pure: dry-run callers need only a read-only registry snapshot.
pub struct Plan {
    before: Snapshot,
    after: Snapshot,
}

impl Plan {
    pub fn new(before: Snapshot, loader: Option<&str>) -> Result<Self> {
        let receipt = before.receipt.as_ref().map(Receipt::read).transpose()?;
        Self::with_receipt(before, loader, receipt)
    }

    pub fn removal_for(before: Snapshot, loader: &std::path::Path) -> Result<Option<Self>> {
        let receipt = before.receipt.as_ref().map(Receipt::read).transpose()?;
        if !receipt.as_ref().is_some_and(|receipt| {
            crate::paths::same_path(std::path::Path::new(&receipt.loader), loader)
        }) {
            return Ok(None);
        }
        Ok(Some(Self::with_receipt(before, None, receipt)?))
    }

    fn with_receipt(
        before: Snapshot,
        loader: Option<&str>,
        receipt: Option<Receipt>,
    ) -> Result<Self> {
        // A deinit with no receipt never claims or normalizes any registry data.
        if receipt.is_none() && loader.is_none() {
            return Ok(Self {
                after: before.clone(),
                before,
            });
        }
        let value = before.autorun.as_ref().map(StringValue::read).transpose()?;
        let text = value.as_ref().map_or("", |value| value.text.as_str());
        let original = match (&receipt, &value) {
            (Some(receipt), Some(value)) => {
                let tail = unowned(text, &invocation(&receipt.loader)?)?;
                if before.autorun.as_ref() == Some(&receipt.installed) {
                    receipt.original.clone()
                } else {
                    // A later user edit is the new baseline, including its value type.
                    // Only strip the exact owned prefix, never restore an old backup.
                    Some(value.write(tail))
                }
            }
            (None, _) => {
                ensure!(
                    !has_marker(text),
                    "Hanamori AutoRun marker has no ownership receipt; no values were changed"
                );
                before.autorun.clone()
            }
            (Some(_), None) => None, // The user already deleted AutoRun; do not resurrect it.
        };
        let after = if let Some(loader) = loader {
            let command = invocation(loader)?;
            let original_text = original.as_ref().map(StringValue::read).transpose()?;
            let text = join(&command, original_text.as_ref().map_or("", |v| &v.text));
            ensure!(
                text.encode_utf16().count() < 8191,
                "initialized AutoRun exceeds CMD's command length limit"
            );
            let format = original_text.unwrap_or(StringValue {
                text: String::new(),
                kind: REG_SZ,
                terminators: 1,
            });
            let installed = format.write(&text);
            let receipt = Receipt {
                schema_version: 1,
                loader: loader.into(),
                original,
                installed: installed.clone(),
            };
            Snapshot {
                autorun: Some(installed),
                receipt: Some(receipt.write()?),
            }
        } else {
            Snapshot {
                autorun: original,
                receipt: None,
            }
        };
        Ok(Self { before, after })
    }

    pub fn changed(&self) -> bool {
        self.before != self.after
    }

    pub fn before(&self) -> &Snapshot {
        &self.before
    }

    pub fn after(&self) -> &Snapshot {
        &self.after
    }

    /// The registry adapter calls this inside the transaction before writing either value.
    pub fn verify_snapshot(&self, current: &Snapshot) -> Result<()> {
        ensure!(
            self.before == *current,
            "CMD AutoRun or its Hanamori receipt changed after planning; inspect and retry"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) mod test_support;
