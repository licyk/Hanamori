use clap::Args;

#[derive(Args)]
pub(crate) struct DefaultArgs {
    /// Directory key, reference or unique installed version.
    pub(crate) selector: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct AliasArgs {
    #[arg(requires = "name")]
    pub(crate) selector: Option<String>,
    #[arg(requires = "selector")]
    pub(crate) name: Option<String>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct UnaliasArgs {
    pub(crate) name: String,
}
