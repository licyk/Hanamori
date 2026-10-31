use crate::cli::remove::RemoveArgs;
use crate::config::ResolvedConfig;
use crate::shell::session::{self, Session};
use anyhow::Result;
pub(super) fn uninstall(
    args: RemoveArgs,
    config: &ResolvedConfig,
    environment: &session::Environment,
) -> Result<()> {
    let session = Session::read(environment)?;
    let plan = crate::engine::uninstall::run(
        &config.toolchains(),
        &args.id,
        session.as_ref().map(|s| &s.active),
        args.dry_run,
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else if plan.completed {
        tracing::info!("Uninstalled {} ({})", plan.key, plan.root.display());
    } else {
        println!("Uninstall plan");
        print!(
            "{}",
            super::output::fields(&[
                ("Toolchain", plan.key.clone()),
                ("Remove directory", plan.root.display().to_string()),
                (
                    "Remove references",
                    if plan.aliases.is_empty() {
                        "none".to_owned()
                    } else {
                        plan.aliases.join(", ")
                    }
                ),
            ])
        );
        println!("Dry run: no files or references changed.");
    }
    Ok(())
}
