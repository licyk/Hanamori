use crate::cli::list::ListArgs;
use crate::config::ResolvedConfig;
use crate::shell::session::{self, Session};
use crate::state::toolchains::{Toolchain, Toolchains};
use anyhow::Result;
pub(super) fn list(
    args: ListArgs,
    config: &ResolvedConfig,
    environment: &session::Environment,
) -> Result<()> {
    let items = config.toolchains().scan()?;
    let active = Session::read(environment)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&list_json(&items, active.as_ref()))?
        );
    } else {
        if items.installations.is_empty() {
            println!("No toolchain directories found.");
        } else {
            println!("Toolchains ({})", items.installations.len());
        }
        for item in items.installations.values() {
            let mut markers = Vec::new();
            if is_active(item, active.as_ref()) {
                markers.push("active");
            }
            if items.references.get("default") == Some(&item.reference()) {
                markers.push("default");
            }
            println!(
                "\n{}{}",
                item.key,
                if markers.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", markers.join(", "))
                }
            );
            let aliases = items.aliases(item);
            print!(
                "{}",
                super::output::fields(&[
                    ("Status", item.status.clone()),
                    (
                        "Type",
                        if item.managed { "managed" } else { "external" }.to_owned()
                    ),
                    ("MSVC", item.toolset_version.clone()),
                    (
                        "Windows SDK",
                        item.sdk_version.as_deref().unwrap_or("none").to_owned()
                    ),
                    (
                        "MSBuild",
                        item.msbuild_version.as_deref().unwrap_or("none").to_owned()
                    ),
                    (
                        "Aliases",
                        if aliases.is_empty() {
                            "none".to_owned()
                        } else {
                            aliases.join(", ")
                        }
                    ),
                    ("Directory", item.path.display().to_string()),
                ])
            );
        }
        for issue in &items.issues {
            tracing::warn!("{issue}");
        }
    }
    Ok(())
}
fn list_json(items: &Toolchains, active: Option<&Session>) -> serde_json::Value {
    let rows: Vec<_> = items.installations.values().map(|item| serde_json::json!({
        "key": item.key, "path": item.path, "managed": item.managed,
        "toolset": item.toolset_version, "sdk": item.sdk_version, "msbuild": item.msbuild_version, "status": item.status,
        "aliases": items.aliases(item), "default": items.references.get("default") == Some(&item.reference()),
        "active": is_active(item, active)
    })).collect();
    serde_json::json!({"installations": rows, "references": items.references, "issues": items.issues})
}

fn is_active(item: &Toolchain, session: Option<&Session>) -> bool {
    session.is_some_and(|session| {
        item.path.to_str() == Some(session.active.root.as_str())
            && item.toolset_version == session.active.toolset
            && item.sdk_version == session.active.sdk
            && item.msbuild_version == session.active.msbuild
    })
}
