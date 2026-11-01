use crate::cli::doctor::DoctorArgs;
use crate::config::{ConfigContext, ConfigOverrides, Scope};
use crate::shell::session;
use anyhow::Result;
pub(super) fn doctor(
    args: DoctorArgs,
    overrides: ConfigOverrides,
    environment: session::Environment,
) -> Result<()> {
    let context = ConfigContext::from_environment(environment)?;
    let config = context.resolve(&overrides, Scope::Local);
    let store = match &config {
        Ok(config) => {
            crate::logging::init(config.log_filter.clone());
            config.toolchains()
        }
        Err(_) => {
            crate::logging::init_inherited(overrides.log.as_deref(), &context.environment);
            context.completion_store(
                overrides.manager_dir.as_deref(),
                overrides.toolchains_dir.as_deref(),
            )?
        }
    };
    let mut report = crate::engine::lifecycle::doctor(&store, args.dir.as_deref(), args.arch)?;
    if let Err(error) = config {
        report.issues.push(format!("config_invalid: {error:#}"));
        report.healthy = false;
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!(
            "{}",
            super::output::fields(&[
                (
                    "Status",
                    if report.healthy {
                        "healthy"
                    } else {
                        "needs attention"
                    }
                    .to_owned()
                ),
                ("Manager", report.manager_dir.display().to_string()),
                ("Issues", report.issues.len().to_string()),
            ])
        );
        if !report.issues.is_empty() {
            println!();
            print!(
                "{}",
                super::output::table(
                    ["#", "Issue"],
                    report
                        .issues
                        .iter()
                        .enumerate()
                        .map(|(index, issue)| [(index + 1).to_string(), issue.clone()])
                        .collect()
                )
            );
        }
        println!("No files changed.");
    }
    anyhow::ensure!(
        report.healthy,
        "toolchain issues found; reinstall incomplete tools or update references"
    );
    Ok(())
}
