use crate::cli::remove::RemoveArgs;
use crate::config::ResolvedConfig;
use crate::shell::session;
use anyhow::Result;
pub(super) fn remove(
    args: RemoveArgs,
    config: &ResolvedConfig,
    _: &session::Environment,
) -> Result<()> {
    crate::state::toolchains::validate_alias(&args.id)?;
    if !args.dry_run {
        config.toolchains().remove_reference(&args.id)?;
    }
    if args.json {
        println!(
            "{}",
            serde_json::json!({"reference": args.id, "removed": !args.dry_run, "files_preserved": true})
        );
    } else {
        super::report_status(
            args.dry_run,
            format_args!(
                "{} reference {}; target files preserved.",
                if args.dry_run {
                    "Would remove"
                } else {
                    "Removed"
                },
                args.id
            ),
        );
    }
    Ok(())
}
