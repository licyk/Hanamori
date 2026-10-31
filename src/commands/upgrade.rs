use crate::cli::upgrade::UpgradeArgs;
use crate::shell::session;
use crate::{
    config::ResolvedConfig,
    upgrade::{self, Mode, Status, Updater},
};
use anyhow::{ensure, Result};

pub(super) fn upgrade(
    args: UpgradeArgs,
    config: &ResolvedConfig,
    environment: &session::Environment,
) -> Result<()> {
    ensure!(cfg!(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")), "self-update has no release asset for this build; currently only Windows x64 MSVC releases support upgrade");
    let token = environment
        .get(std::ffi::OsStr::new("GITHUB_TOKEN"))
        .map(|value| {
            value
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("GITHUB_TOKEN must be valid Unicode"))
        })
        .transpose()?;
    let updater = Updater::new(
        upgrade::RELEASE_URL.parse()?,
        config.proxy.as_ref(),
        token,
        crate::logging::progress_enabled(),
    )?;
    let mode = if args.check {
        Mode::Check
    } else {
        Mode::Upgrade { force: args.force }
    };
    let report = crate::runtime::block_on(updater.run(
        &std::env::current_exe()?,
        crate::identity::VERSION.parse()?,
        mode,
    ))??;
    if args.check {
        print!(
            "{}",
            super::output::fields(&[
                ("Current version", report.current.to_string()),
                ("Latest stable", report.latest.to_string()),
                ("Executable", report.executable.display().to_string()),
            ])
        );
    } else {
        tracing::info!(
            "Current: {}; latest stable: {}",
            report.current,
            report.latest
        );
        tracing::info!("Executable: {}", report.executable.display());
    }
    match report.status {
        Status::Available => super::report_status(
            args.check,
            format_args!("Update available. Run hanamori upgrade to install it."),
        ),
        Status::UpToDate => {
            super::report_status(args.check, format_args!("Hanamori is already up to date."))
        }
        Status::Ahead => super::report_status(
            args.check,
            format_args!(
                "This build is newer than the latest release; no downgrade will be performed."
            ),
        ),
        Status::Updated => {
            tracing::info!(
                "Updated to Hanamori {}. The next invocation uses the new executable.",
                report.latest
            );
            tracing::info!(
                "Previous executable retained at {} until the next update.",
                report.backup.unwrap().display()
            );
        }
    }
    Ok(())
}
