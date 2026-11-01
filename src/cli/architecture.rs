use crate::architecture::Architecture;
use clap::builder::{PossibleValuesParser, TypedValueParser};

pub(crate) fn parser() -> impl TypedValueParser<Value = Architecture> {
    PossibleValuesParser::new(["x64/x64", "x64/arm64", "arm64/x64", "arm64/arm64"])
        .map(|value| value.parse().expect("supported architecture pair"))
}
