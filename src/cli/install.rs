use super::catalog::CommonArgs;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct InstallArgs {
    /// Exact installation directory (default: an independent managed toolchain directory).
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath, help_heading = "Destination")]
    pub(crate) dir: Option<PathBuf>,

    /// Alias for the installed fixed combination.
    #[arg(long, value_name = "ALIAS", help_heading = "Destination")]
    pub(crate) name: Option<String>,

    /// Managed directory key, installed MSVC version/prefix, or latest. Projects supply omitted constraints.
    #[arg(value_name = "VERSION", conflicts_with = "toolset_id")]
    pub(crate) version: Option<String>,
    /// Exact catalog download identifier; distinct from an installed MSVC version.
    #[arg(long, value_name = "ID", help_heading = "Versions and components")]
    pub(crate) toolset_id: Option<String>,
    /// Exact installed SDK directory version.
    #[arg(
        long,
        value_name = "VERSION",
        conflicts_with = "sdk_id",
        help_heading = "Versions and components"
    )]
    pub(crate) sdk: Option<String>,
    /// Exact SDK download identifier from list-remote.
    #[arg(
        long,
        value_name = "ID",
        conflicts_with = "no_sdk",
        help_heading = "Versions and components"
    )]
    pub(crate) sdk_id: Option<String>,

    /// Install only the compiler (excludes the SDK and, by default, MSBuild).
    #[arg(long, conflicts_with = "sdk", help_heading = "Versions and components")]
    pub(crate) no_sdk: bool,

    /// Enable MSBuild explicitly, overriding the project (default: enabled with the SDK).
    #[arg(long, conflicts_with_all = ["no_msbuild", "no_sdk"], help_heading = "Versions and components")]
    pub(crate) msbuild: bool,

    /// Exclude MSBuild, overriding the project. Keep the compiler and SDK by default.
    #[arg(long, help_heading = "Versions and components")]
    pub(crate) no_msbuild: bool,

    /// Fetch catalog metadata and show a plan; do not download payloads or create installations.
    #[arg(long, help_heading = "Installation")]
    pub(crate) dry_run: bool,

    /// Replace the entire installation directory, even if it appears complete.
    #[arg(long, help_heading = "Installation")]
    pub(crate) reinstall: bool,

    /// Host/target architecture pair, e.g. x64/x64.
    #[arg(long, value_name = "ARCH", default_value = "x64/x64", value_parser = ["x64/x64"], help_heading = "Versions and components")]
    pub(crate) arch: String,

    #[command(flatten, next_help_heading = "Catalog")]
    pub(crate) common: CommonArgs,
}
