use crate::cli::list::ListArgs;
use crate::shell::session::{self, Session};
use anyhow::Result;

pub(super) fn current(args: ListArgs, environment: &session::Environment) -> Result<()> {
    let Some(session) = Session::read(environment)? else {
        if args.json {
            println!("{}", serde_json::json!({"active": null}));
        } else {
            println!("No Hanamori session is active.");
        }
        return Ok(());
    };
    let active = &session.active;
    let layout = crate::env::ToolchainLayout {
        architecture: crate::architecture::Architecture {
            host: active.host,
            target: active.target,
        },
        root: active.root.clone().into(),
        vc_tools_version: active.toolset.clone(),
        windows_sdk_version: active.sdk.clone().unwrap_or_default(),
        msbuild_version: active.msbuild.clone(),
    };
    let status = match crate::paths::canonicalize(&layout.root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "inaccessible",
        Ok(path)
            if !crate::paths::same_path(&path, std::path::Path::new(&active.canonical_root)) =>
        {
            "path_changed"
        }
        _ if crate::env::verify(&layout).is_err() => "incomplete",
        _ => "ready",
    };
    let differences = session.environment_differences(environment)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"active": active, "status": status, "modified_variables": differences})
            )?
        );
    } else {
        println!("Current toolchain");
        print!(
            "{}",
            super::output::fields(&[
                (
                    "Toolchain",
                    active.key.as_deref().unwrap_or("directory").to_owned()
                ),
                ("Status", status.to_owned()),
                ("MSVC", active.toolset.clone()),
                (
                    "Windows SDK",
                    active.sdk.as_deref().unwrap_or("none").to_owned()
                ),
                (
                    "MSBuild",
                    active.msbuild.as_deref().unwrap_or("none").to_owned()
                ),
                (
                    "Architecture",
                    format!("{} -> {}", active.host, active.target)
                ),
                ("Directory", active.root.clone()),
                ("Manager", active.manager_dir.clone()),
            ])
        );
        if !differences.is_empty() {
            println!(
                "\nVariables changed since selection ({})",
                differences.len()
            );
            for name in differences {
                println!("  - {name}");
            }
        }
    }
    Ok(())
}
