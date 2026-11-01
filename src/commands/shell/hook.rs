use crate::cli::shell::{HookArgs, HookTargetArgs};
use crate::commands::{completions, script::write_script_mode};
use crate::shell::{hook, syntax::Shell, target::DirectoryTarget};
use anyhow::Result;

pub(crate) fn print_hook(args: HookArgs) -> Result<()> {
    let shell = crate::shell::detection::resolve(args.shell)?.shell;
    let completions = args
        .completions
        .then(|| completions::registration(shell))
        .transpose()?;
    if shell == Shell::Cmd {
        if let Some(output) = &args.output {
            anyhow::ensure!(
                output
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd")
                        || extension.eq_ignore_ascii_case("bat")),
                "CMD batch entry output must have a .cmd or .bat extension"
            );
        }
    }
    let directory = hook_directory(args.target)?;
    let mut script = hook::generate(
        &std::env::current_exe()?,
        directory.as_ref(),
        shell,
        args.auto_activate,
        args.use_on_cd,
    )?;
    if let Some(completions) = completions {
        script.push_str(&completions);
    }
    write_script_mode(shell, args.output, script, shell == Shell::Cmd)
}

pub(super) fn hook_directory(target: HookTargetArgs) -> Result<Option<DirectoryTarget>> {
    target
        .dir
        .map(|path| {
            for (value, parts) in [(&target.toolset, 3), (&target.sdk, 4)] {
                if let Some(value) = value {
                    anyhow::ensure!(
                        crate::env::valid_directory_version(value, parts),
                        "expected an installed directory version"
                    );
                }
            }
            Ok(DirectoryTarget {
                architecture: target.arch,
                path: std::path::absolute(path)?,
                toolset: target.toolset,
                sdk: target.sdk,
                no_sdk: target.no_sdk,
            })
        })
        .transpose()
}
