use super::{RawValue, StringValue};

pub(crate) fn string(kind: u32, text: &str, terminators: usize) -> RawValue {
    StringValue {
        text: String::new(),
        kind,
        terminators,
    }
    .write(text)
}
