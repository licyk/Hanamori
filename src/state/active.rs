//! Identity of the toolchain currently in use.

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveToolchain {
    #[serde(deserialize_with = "Option::deserialize")]
    pub key: Option<String>,
    pub manager_dir: String,
    pub root: String,
    pub canonical_root: String,
    pub toolset: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub sdk: Option<String>,
    pub msbuild: Option<String>,
    pub host: String,
    pub target: String,
}
