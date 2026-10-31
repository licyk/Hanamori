use crate::cli::import::ImportArgs;
use crate::config::ResolvedConfig;
use anyhow::Result;

pub(super) fn import(args: ImportArgs, config: &ResolvedConfig) -> Result<()> {
    crate::engine::import::run(
        &config.toolchains(),
        &args.dir,
        args.toolset.as_deref(),
        args.sdk.as_deref(),
        args.no_sdk,
        &args.name,
    )?;
    if args.json {
        println!("{}", serde_json::json!({"reference": args.name}));
    } else {
        tracing::info!("Reference: {}", args.name);
    }
    Ok(())
}
