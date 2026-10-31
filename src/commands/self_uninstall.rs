use crate::cli::self_uninstall::UninstallArgs;
use crate::shell::{
    session::{self, Session},
    startup::records::Store,
};
use crate::{
    config::{ConfigContext, ConfigOverrides, Scope},
    self_uninstall::{Completion, Plan},
};
use anyhow::{ensure, Result};

pub(super) fn uninstall(
    args: UninstallArgs,
    overrides: ConfigOverrides,
    environment: session::Environment,
) -> Result<()> {
    let active = Session::read(&environment)?.is_some();
    let context = ConfigContext::from_environment(environment)?;
    let manager = context.manager_dir(overrides.manager_dir.as_deref())?.value;
    let config = args
        .purge
        .then(|| context.resolve(&overrides, Scope::Cache))
        .transpose()?;
    let tools = if args.purge {
        context.completion_store(
            overrides.manager_dir.as_deref(),
            overrides.toolchains_dir.as_deref(),
        )?
    } else {
        crate::state::toolchains::ToolchainStore::new(&manager)
    };
    let cache = config.as_ref().and_then(|c| c.cache_dir.as_deref());
    let store = Store::from_environment(&context.environment)?;
    let plan = Plan::new(&std::env::current_exe()?, &tools, cache, &store)?;
    print_plan(&plan);
    if active {
        println!("Blocked: this shell has an active toolchain; run hanamori deactivate first.");
    }
    if args.dry_run {
        println!("Dry run: no files or registry values changed; no helper launched.");
        return Ok(());
    }
    ensure!(!active, "run hanamori deactivate before self uninstall");
    match plan.execute()? {
        Completion::Removed => {
            tracing::info!(
                "Hanamori executable removed. Restart this shell to discard its loaded hook."
            )
        }
        #[cfg(windows)]
        Completion::Scheduled { script } => {
            tracing::info!(
                "Executable removal scheduled after this process exits; it is not yet complete."
            );
            tracing::info!("The helper reports completion or writes failure.txt beside {}. On failure, close processes using the executable and rerun that script. Restart this shell to discard its loaded hook.", script.display());
        }
    }
    Ok(())
}

fn print_plan(plan: &Plan) {
    println!("Self uninstall plan");
    let mut rows = vec![[
        "Remove".to_owned(),
        "Executable".to_owned(),
        plan.executable.display().to_string(),
    ]];
    if plan.backup.exists() {
        rows.push([
            "Remove".into(),
            "Upgrade backup".into(),
            plan.backup.display().to_string(),
        ]);
    }
    for profile in &plan.startup.profiles {
        rows.push([
            "Remove".into(),
            "Recorded startup block".into(),
            profile.requested.display().to_string(),
        ]);
    }
    if plan.startup.has_cmd() {
        rows.push([
            "Remove".into(),
            "CMD AutoRun".into(),
            "Owned current-user invocation and receipt".into(),
        ]);
    }
    for file in &plan.startup.files {
        rows.push([
            "Remove".into(),
            "CMD helper".into(),
            file.display().to_string(),
        ]);
    }
    for toolchain in &plan.toolchains {
        rows.push([
            "Remove".into(),
            "Entire owned toolchain".into(),
            toolchain.root.display().to_string(),
        ]);
    }
    for imported in &plan.imports {
        rows.push([
            "Preserve".into(),
            "Imported directory".into(),
            imported.display().to_string(),
        ]);
    }
    for file in &plan.metadata {
        rows.push([
            "Remove".into(),
            "Metadata / reference".into(),
            file.display().to_string(),
        ]);
    }
    if plan.cache.is_none() {
        rows.push([
            "Preserve".into(),
            "Toolchains and manager data".into(),
            plan.manager.display().to_string(),
        ]);
    }
    print!(
        "{}",
        super::output::table(["Action", "Resource", "Target"], rows)
    );
    if let Some(cache) = &plan.cache {
        println!(
            "\nCache cleanup ({} entries, {} candidate size)",
            cache.entries.len(),
            indicatif::HumanBytes(cache.candidate_bytes)
        );
        if !cache.entries.is_empty() {
            print!(
                "{}",
                super::output::table(
                    ["Status", "Size", "Path", "Reason"],
                    cache
                        .entries
                        .iter()
                        .map(|entry| [
                            format!("{:?}", entry.outcome),
                            indicatif::HumanBytes(entry.bytes).to_string(),
                            cache.cache_dir.join(&entry.path).display().to_string(),
                            entry.reason.clone().unwrap_or_default(),
                        ])
                        .collect()
                )
            );
        }
    }
    println!("\nNotes");
    for note in &plan.startup.notes {
        println!("  - {note}");
    }
    if !plan.imports.is_empty() {
        println!("  - Imported directories are preserved; their registrations will be removed.");
    }
    if plan.cache.is_some() {
        println!("  - Preserve unknown files, partial download workspaces and profile backups.");
    }
    println!("  - Unrecorded startup entries and manually configured PATH entries require manual removal.");
    println!("  - Close other Hanamori sessions before uninstalling.");
    println!("  - Lock files are preserved, including the executable's .update.lock file.");
}
