use crate::cli::alias::{AliasArgs, DefaultArgs, UnaliasArgs};
use crate::config::ResolvedConfig;
use crate::state::toolchains;
use anyhow::Result;
pub(super) fn default(args: DefaultArgs, config: &ResolvedConfig) -> Result<()> {
    let store = config.toolchains();
    if let Some(selector) = args.selector {
        assign(&store, &selector, "default")?;
    }
    let items = store.scan()?;
    if args.json {
        println!(
            "{}",
            serde_json::json!({"default": items.references.get("default")})
        );
    } else if let Some(reference) = items.references.get("default") {
        print_reference("default", reference, &items);
    } else {
        println!("No default selected.");
    }
    Ok(())
}
pub(super) fn alias(args: AliasArgs, config: &ResolvedConfig) -> Result<()> {
    let store = config.toolchains();
    if let (Some(selector), Some(name)) = (&args.selector, &args.name) {
        assign(&store, selector, name)?;
    }
    let items = store.scan()?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&items.references)?);
    } else if items.references.is_empty() {
        println!("No aliases defined.");
    } else {
        println!("Aliases ({})", items.references.len());
        for (name, reference) in &items.references {
            println!();
            print_reference(name, reference, &items);
        }
    }
    Ok(())
}

fn print_reference(name: &str, reference: &toolchains::Reference, items: &toolchains::Toolchains) {
    let key = items.reference_key(name).unwrap();
    let status = items
        .installations
        .get(&key)
        .map(|item| item.status.as_str())
        .unwrap_or("missing");
    println!("{name}");
    let mut rows = vec![("Status", status.to_owned())];
    match reference {
        toolchains::Reference::Managed { key } => {
            rows.push(("Type", "managed".to_owned()));
            rows.push(("Toolchain", key.clone()));
            if let Some(item) = items.installations.get(key) {
                rows.push(("Directory", item.path.display().to_string()));
            }
        }
        toolchains::Reference::External {
            path,
            toolset,
            sdk,
            msbuild,
            architecture,
        } => {
            rows.extend([
                ("Type", "external".to_owned()),
                ("MSVC", toolset.clone()),
                ("Windows SDK", sdk.as_deref().unwrap_or("none").to_owned()),
                ("MSBuild", msbuild.as_deref().unwrap_or("none").to_owned()),
                ("Architecture", architecture.to_string()),
                ("Directory", path.display().to_string()),
            ]);
        }
    }
    print!("{}", super::output::fields(&rows));
}
fn assign(store: &toolchains::ToolchainStore, selector: &str, name: &str) -> Result<()> {
    let items = store.scan()?;
    let item = toolchains::select(&items, selector)?;
    anyhow::ensure!(
        item.status == "ready",
        "toolchain is {}; install it again",
        item.status
    );
    store.write_reference(name, &item.reference())
}
pub(super) fn unalias(args: UnaliasArgs, config: &ResolvedConfig) -> Result<()> {
    config.toolchains().remove_reference(&args.name)
}
