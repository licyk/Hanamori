//! Product identity shared by the CLI, HTTP clients and generated files.

pub const PRODUCT_NAME: &str = "Hanamori";
pub const COMMAND_NAME: &str = "hanamori";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("hanamori/", env!("CARGO_PKG_VERSION"));
pub const TEMPORARY_PREFIX: &str = "hanamori-";
pub const ENV_PREFIX: &str = "HANAMORI_";
pub const SESSION_PREFIX: &str = "_HANAMORI_";
pub const PROJECT_FILE: &str = "hanamori-toolchain.json";
pub const INIT_START: &str = "# >>> hanamori initialize >>>";
pub const INIT_END: &str = "# <<< hanamori initialize <<<";
