use super::hook::hook_directory;
use crate::cli::shell::{DeinitArgs, InitArgs};
use crate::commands::{completions, report_status};
use crate::paths::PathExt;
#[cfg(windows)]
use crate::shell::target::DirectoryTarget;
use crate::shell::{hook, session, startup::profile::ProfilePlan, syntax::Shell};
use anyhow::{Context, Result};
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn init_shell(mut args: InitArgs, environment: &session::Environment) -> Result<()> {
    let shell = resolve_startup_shell(args.shell, &args.profile, &mut args.shell_executable)?;
    if args.completions {
        completions::ensure_supported(shell)?;
    }
    if shell == Shell::Cmd {
        cmd_init_parameters(&args.profile, &args.shell_executable)?;
    }
    let directory = hook_directory(args.target)?;
    if shell == Shell::Cmd {
        #[cfg(windows)]
        return initialize_cmd(
            &std::env::current_exe()?,
            directory.as_ref(),
            args.auto_activate,
            args.use_on_cd,
            args.dry_run,
            environment,
        );
        #[cfg(not(windows))]
        anyhow::bail!("persistent CMD initialization requires native Windows and the current-user registry; use env --shell cmd --output hanamori.cmd for a portable entry");
    }
    let loader = hook::loader(
        &std::env::current_exe()?,
        directory.as_ref(),
        shell,
        args.auto_activate,
        args.completions,
        args.use_on_cd,
    )?;
    discover_and_edit(
        shell,
        args.profile,
        args.shell_executable,
        Some(loader),
        args.dry_run,
        environment,
    )
}

fn cmd_init_parameters(profile: &Option<PathBuf>, executable: &Option<OsString>) -> Result<()> {
    anyhow::ensure!(profile.is_none(), "CMD initialization uses the current-user AutoRun registry value; --profile is not a registry path and is not supported for CMD");
    anyhow::ensure!(executable.is_none(), "--shell-executable is for startup-file discovery; CMD initialization uses the current-user registry");
    Ok(())
}

pub(crate) fn deinit_shell(mut args: DeinitArgs, environment: &session::Environment) -> Result<()> {
    let shell = resolve_startup_shell(args.shell, &args.profile, &mut args.shell_executable)?;
    if shell == Shell::Cmd {
        cmd_init_parameters(&args.profile, &args.shell_executable)?;
        #[cfg(windows)]
        return deinitialize_cmd(args.dry_run);
        #[cfg(not(windows))]
        anyhow::bail!(
            "persistent CMD deinitialization requires native Windows and the current-user registry"
        );
    }
    discover_and_edit(
        shell,
        args.profile,
        args.shell_executable,
        None,
        args.dry_run,
        environment,
    )
}

fn resolve_startup_shell(
    shell: Option<Shell>,
    profile: &Option<PathBuf>,
    executable: &mut Option<OsString>,
) -> Result<Shell> {
    let explicit = shell.or_else(|| {
        executable.as_ref().and_then(|path| {
            std::path::Path::new(path)
                .file_stem()?
                .to_str()
                .and_then(|name| Shell::parse(name).ok())
        })
    });
    let detected = crate::shell::detection::resolve(explicit)?;
    if explicit.is_none()
        && profile.is_none()
        && executable.is_none()
        && detected.shell != Shell::Cmd
    {
        // Profile discovery must use the detected runtime, especially for PowerShell 5.1 vs 7.
        *executable = Some(
            detected
                .executable
                .context("cannot locate the detected shell executable; specify --shell-executable or --profile")?
                .into_os_string(),
        );
    }
    Ok(detected.shell)
}

#[cfg(windows)]
fn initialize_cmd(
    program: &std::path::Path,
    directory: Option<&DirectoryTarget>,
    auto_activate: bool,
    use_on_cd: bool,
    dry_run: bool,
    environment: &session::Environment,
) -> Result<()> {
    use crate::shell::cmd::autorun::windows::USER_KEY;
    let report = crate::shell::cmd::startup::initialize(
        program,
        directory,
        auto_activate,
        use_on_cd,
        dry_run,
    )?;
    if !dry_run {
        crate::shell::startup::records::Store::from_environment(environment)?.cmd(
            program,
            &report.loader,
            report.files.clone(),
        )?;
    }
    if dry_run {
        print!(
            "{}",
            crate::commands::output::fields(&[
                ("Registry", format!("HKCU\\{USER_KEY}\\AutoRun")),
                ("Loader", report.loader.native_display().to_string()),
                ("Entry", report.entry.native_display().to_string()),
                (
                    "Action",
                    if report.changed {
                        "Publish missing loaders and add/update Hanamori's AutoRun invocation"
                    } else {
                        "No changes needed"
                    }
                    .to_owned()
                ),
            ])
        );
        println!("Dry run: no files or registry values written.");
    } else {
        tracing::info!("CMD registry: HKCU\\{USER_KEY}\\AutoRun");
        tracing::info!("CMD loader: {}", report.loader.native_display());
        tracing::info!("CMD entry: {}", report.entry.native_display());
    }
    if !report.changed && !dry_run {
        tracing::info!("No changes needed.");
    }
    if report.changed && !dry_run {
        tracing::info!("Updated Hanamori CMD initialization");
    }
    report_status(
        dry_run,
        format_args!(
            "{} open a new CMD window, or call \"{}\" in CMD to load the entry now.",
            if dry_run {
                "After applying,"
            } else {
                "To use,"
            },
            report.loader.native_display()
        ),
    );
    report_status(dry_run, format_args!("The interactive hanamori macro calls the batch entry; in another batch file, use call \"%_HANAMORI_CMD_ENTRY%\" COMMAND."));
    Ok(())
}

#[cfg(windows)]
fn deinitialize_cmd(dry_run: bool) -> Result<()> {
    use crate::shell::cmd::autorun::windows::USER_KEY;
    let changed = crate::shell::cmd::startup::deinitialize(dry_run)?;
    if dry_run {
        print!(
            "{}",
            crate::commands::output::fields(&[
                ("Registry", format!("HKCU\\{USER_KEY}\\AutoRun")),
                (
                    "Action",
                    if changed {
                        "Remove only Hanamori's AutoRun invocation and receipt"
                    } else {
                        "No changes needed"
                    }
                    .to_owned()
                ),
            ])
        );
        println!("Dry run: no files or registry values written.");
    } else {
        tracing::info!("CMD registry: HKCU\\{USER_KEY}\\AutoRun");
    }
    if !changed && !dry_run {
        tracing::info!("No changes needed.");
    } else if changed && !dry_run {
        tracing::info!("Removed Hanamori CMD initialization");
    }
    report_status(dry_run, format_args!("Deinitialization affects future CMD windows. Loader files are retained for open sessions; run hanamori deactivate before closing an active terminal. No current macro, environment or toolchain was removed."));
    Ok(())
}

fn discover_and_edit(
    shell: Shell,
    profile: Option<PathBuf>,
    executable: Option<OsString>,
    body: Option<String>,
    dry_run: bool,
    environment: &session::Environment,
) -> Result<()> {
    let default_bash = shell == Shell::Bash && profile.is_none();
    let runtime = executable.clone();
    let location = crate::shell::startup::discover(
        shell,
        profile,
        executable,
        dirs::home_dir(),
        environment.get(std::ffi::OsStr::new("ZDOTDIR")).cloned(),
    )?;
    tracing::info!("Profile selection: {}", location.description);
    for notice in location.notices {
        tracing::warn!("{notice}");
    }
    if default_bash {
        let batch = crate::shell::startup::bash::plan(&location.path, body.as_deref())?;
        if let Some(login) = batch.login {
            report_status(
                dry_run,
                format_args!("Bash login profile: {}", login.native_display()),
            );
        }
        for notice in batch.notices {
            tracing::info!("{notice}");
        }
        let reload = if body.is_some() {
            Some(shell.invocation(&location.path)?)
        } else {
            None
        };
        report_profiles(&batch.files, dry_run)?;
        if !dry_run {
            crate::shell::startup::records::Store::from_environment(environment)?.profiles(
                &std::env::current_exe()?,
                runtime.as_ref(),
                shell,
                &batch.files,
            )?;
        }
        profile_reload(reload, dry_run);
        return Ok(());
    }
    let plan = edit_profile(shell, location.path, body, dry_run)?;
    if !dry_run {
        crate::shell::startup::records::Store::from_environment(environment)?.profiles(
            &std::env::current_exe()?,
            runtime.as_ref(),
            shell,
            &[plan],
        )?;
    }
    Ok(())
}

fn edit_profile(
    shell: Shell,
    path: PathBuf,
    body: Option<String>,
    dry_run: bool,
) -> Result<ProfilePlan> {
    let body = if shell == Shell::Bash {
        body.map(|loader| crate::shell::startup::bash::preserve_interactive_marker(&path, &loader))
            .transpose()?
    } else {
        body
    };
    let plan = ProfilePlan::new(&path, shell, body.as_deref())?;
    // Validate the reload instruction before publishing any file.
    let reload = if body.is_some() {
        Some(shell.invocation(&plan.requested)?)
    } else {
        None
    };
    report_profiles(std::slice::from_ref(&plan), dry_run)?;
    profile_reload(reload, dry_run);
    Ok(plan)
}

fn report_profiles(plans: &[ProfilePlan], dry_run: bool) -> Result<()> {
    for plan in plans {
        if dry_run {
            let action = if !plan.changed() {
                "No changes needed"
            } else if plan.removes_file() {
                "Remove this empty Hanamori-created login file and its block"
            } else if plan.is_removal() {
                "Remove this managed block"
            } else {
                "Add or replace this managed block"
            };
            let mut rows = vec![("Profile", plan.requested.native_display().to_string())];
            if plan.requested != plan.target {
                rows.push(("Resolved target", plan.target.native_display().to_string()));
            }
            rows.push(("Action", action.to_owned()));
            print!("{}", crate::commands::output::fields(&rows));
            println!("Dry run: no files written.");
            if plan.changed() && !plan.preview.is_empty() {
                println!("\nChanges:");
                print!("{}", plan.preview);
                println!();
            }
        } else {
            tracing::info!("Profile: {}", plan.requested.native_display());
            if plan.requested != plan.target {
                tracing::info!("Resolved target: {}", plan.target.native_display());
            }
            if !plan.changed() {
                tracing::info!("No changes needed.");
            }
        }
    }
    if !dry_run {
        let backups = crate::shell::startup::profile::apply_batch(plans)?;
        for (plan, backup) in plans.iter().zip(backups) {
            if let Some(backup) = backup {
                tracing::info!("Original profile backed up at {}", backup.native_display());
            }
            if plan.changed() {
                let action = if plan.removes_file() {
                    "Removed empty generated login file"
                } else if plan.is_removal() {
                    "Removed Hanamori initialization block from"
                } else {
                    "Updated Hanamori initialization block in"
                };
                tracing::info!("{action} {}", plan.requested.native_display());
            }
        }
    }
    Ok(())
}

fn profile_reload(reload: Option<String>, dry_run: bool) {
    if let Some(reload) = reload {
        report_status(
            dry_run,
            format_args!(
                "{} reload the selected profile in its shell: {reload}",
                if dry_run {
                    "After applying,"
                } else {
                    "To load now,"
                }
            ),
        );
    } else {
        report_status(dry_run, format_args!("Deinitialization affects future profile loads; run hanamori deactivate to restore an active session. Restart the shell to remove its loaded function."));
    }
}
