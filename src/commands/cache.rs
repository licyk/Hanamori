use crate::config::ResolvedConfig;
use anyhow::{Context, Result};

pub(super) fn clean_cache(dry_run: bool, json: bool, config: &ResolvedConfig) -> Result<()> {
    let report = crate::download::cache_clean::run(
        &config
            .cache_dir
            .as_ref()
            .context("missing cache configuration")?,
        dry_run,
    )?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!(
            "{}",
            super::output::fields(&[
                ("Cache", report.cache_dir.display().to_string()),
                (
                    "Status",
                    if dry_run {
                        "preview"
                    } else if report.completed {
                        "complete"
                    } else {
                        "incomplete"
                    }
                    .to_owned()
                ),
                (
                    "Candidate size",
                    indicatif::HumanBytes(report.candidate_bytes).to_string()
                ),
                (
                    "Removed size",
                    indicatif::HumanBytes(report.removed_bytes).to_string()
                ),
            ])
        );
        if report.entries.is_empty() {
            println!("No cache entries found.");
        } else {
            println!();
            print!(
                "{}",
                super::output::table(
                    ["Status", "Size", "Path", "Reason"],
                    report
                        .entries
                        .iter()
                        .map(|entry| [
                            format!("{:?}", entry.outcome),
                            indicatif::HumanBytes(entry.bytes).to_string(),
                            entry.path.display().to_string(),
                            entry.reason.clone().unwrap_or_default(),
                        ])
                        .collect()
                )
            );
        }
        if dry_run {
            println!(
                "Dry run: no files written or locks acquired; actual cleanup skips busy entries."
            );
        }
    }
    anyhow::ensure!(dry_run || report.completed, "some cache entries were busy or could not be removed; completed removals are listed above, retry after resolving the reported conditions");
    Ok(())
}
