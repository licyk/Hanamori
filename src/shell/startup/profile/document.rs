//! Profile text encodings and the managed block inside a profile.

use super::{END, START};
use crate::shell::syntax::Shell;
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::ops::Range;

pub(super) const PREFIX_HASH: &str = "# hanamori-added-newline-after-sha256: ";

#[derive(Clone, Copy)]
pub(super) enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

pub(super) struct Document {
    pub(super) text: String,
    pub(super) encoding: Encoding,
}

impl Document {
    pub(super) fn decode(bytes: &[u8], shell: Shell) -> Result<Self> {
        let (encoding, text) = if bytes.starts_with(&[0xff, 0xfe])
            || bytes.starts_with(&[0xfe, 0xff])
        {
            ensure!(
                bytes.len().is_multiple_of(2),
                "profile has an incomplete UTF-16 code unit"
            );
            let little = bytes[0] == 0xff;
            let units: Vec<_> = bytes[2..]
                .chunks_exact(2)
                .map(|pair| {
                    if little {
                        u16::from_le_bytes([pair[0], pair[1]])
                    } else {
                        u16::from_be_bytes([pair[0], pair[1]])
                    }
                })
                .collect();
            (
                if little {
                    Encoding::Utf16Le
                } else {
                    Encoding::Utf16Be
                },
                String::from_utf16(&units).context("profile contains invalid UTF-16")?,
            )
        } else if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
            (
                Encoding::Utf8Bom,
                String::from_utf8(rest.to_vec()).context("profile contains invalid UTF-8")?,
            )
        } else {
            (Encoding::Utf8, String::from_utf8(bytes.to_vec()).context("profile encoding is not UTF-8 or BOM-marked UTF-16; convert it explicitly before editing")?)
        };
        ensure!(!text.contains('\0'), "profile contains NUL characters");
        // UTF-16 is a PowerShell profile format, not a POSIX shell script format.
        ensure!(
            matches!(shell, Shell::PowerShell)
                || !matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be),
            "Bash/Zsh profiles must use UTF-8, not UTF-16"
        );
        ensure!(
            !text.replace("\r\n", "").contains('\r'),
            "profile contains unsupported bare CR line endings"
        );
        Ok(Self { text, encoding })
    }

    pub(super) fn encode(&self, text: &str) -> Vec<u8> {
        match self.encoding {
            Encoding::Utf8 => text.as_bytes().to_vec(),
            Encoding::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let little = matches!(self.encoding, Encoding::Utf16Le);
                let mut bytes = if little {
                    vec![0xff, 0xfe]
                } else {
                    vec![0xfe, 0xff]
                };
                for unit in text.encode_utf16() {
                    bytes.extend(if little {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
                bytes
            }
        }
    }

    fn newline(&self) -> &'static str {
        match self.text.find('\n') {
            Some(offset) if offset > 0 && self.text.as_bytes()[offset - 1] == b'\r' => "\r\n",
            Some(_) => "\n",
            None if cfg!(windows) => "\r\n",
            None => "\n",
        }
    }
}

struct Block {
    range: Range<usize>,
    added_newline_hash: Option<String>,
}

fn block(text: &str) -> Result<Option<Block>> {
    let mut start = None;
    let mut end = None;
    let mut offset = 0;
    let mut added_newline_hash = None;
    for line in text.split_inclusive('\n') {
        let value = line.trim_end_matches(['\r', '\n']);
        if value.trim() == START {
            ensure!(
                value == START && start.is_none() && end.is_none(),
                "duplicate or malformed Hanamori start marker"
            );
            start = Some(offset);
        } else if value.trim() == END {
            ensure!(
                value == END && start.is_some() && end.is_none(),
                "unmatched or duplicate Hanamori end marker"
            );
            end = Some(offset + line.len());
        } else if start.is_some() && end.is_none() {
            if let Some(hash) = value.strip_prefix(PREFIX_HASH) {
                ensure!(
                    added_newline_hash.is_none()
                        && hash.len() == 64
                        && hash.bytes().all(|c| c.is_ascii_hexdigit()),
                    "invalid Hanamori newline metadata"
                );
                added_newline_hash = Some(hash.to_owned());
            }
        }
        offset += line.len();
    }
    match (start, end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => Ok(Some(Block {
            range: start..end,
            added_newline_hash,
        })),
        _ => bail!("incomplete Hanamori block; repair the markers before editing the profile"),
    }
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub(super) fn transform(document: &Document, body: Option<&str>) -> Result<(String, String)> {
    let current = block(&document.text)?;
    let newline = document.newline();
    let mut prefix_hash = current.as_ref().and_then(|b| b.added_newline_hash.clone());
    let mut text = document.text.clone();
    if let Some(body) = body {
        ensure!(
            !body.contains('\r') && !body.contains('\0') && block(body)?.is_none(),
            "invalid initialization loader"
        );
        let range = if let Some(current) = current {
            current.range
        } else {
            if !text.is_empty() && !text.ends_with('\n') {
                prefix_hash = Some(digest(&text));
                text.push_str(newline);
            }
            text.len()..text.len()
        };
        let mut replacement = format!("{START}{newline}");
        if let Some(hash) = prefix_hash {
            replacement.push_str(&format!("{PREFIX_HASH}{hash}{newline}"));
        }
        replacement.push_str(&body.trim_end_matches('\n').replace('\n', newline));
        replacement.push_str(&format!("{newline}{END}{newline}"));
        text.replace_range(range, &replacement);
        Ok((text, replacement))
    } else if let Some(current) = current {
        let preview = text[current.range.clone()].to_owned();
        let mut range = current.range;
        if let Some(hash) = prefix_hash.filter(|_| range.end == text.len()) {
            let prefix = &text[..range.start];
            let stripped = prefix
                .strip_suffix("\r\n")
                .or_else(|| prefix.strip_suffix('\n'));
            if let Some(stripped) = stripped {
                if digest(stripped) == hash {
                    range.start = stripped.len();
                }
            }
        }
        text.replace_range(range, "");
        Ok((text, preview))
    } else {
        Ok((text, String::new()))
    }
}
