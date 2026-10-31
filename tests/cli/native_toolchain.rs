//! Explicit acceptance gate, not a synthetic compiler or an automatic network test.
#[test]
#[ignore = "requires native Windows and two installed MSVC/SDK combinations; see README"]
fn two_real_toolchains_compile_link_and_run_from_clean_and_active_sessions() -> anyhow::Result<()> {
    #[cfg(not(windows))]
    anyhow::bail!("this acceptance test must be run on native Windows");
    #[cfg(windows)]
    native::run(false)
}

#[test]
#[ignore = "requires native Windows and two installed MSBuild-enabled toolchains; see testing docs"]
fn two_real_msbuild_toolchains_build_projects_and_solutions() -> anyhow::Result<()> {
    #[cfg(not(windows))]
    anyhow::bail!("this acceptance test must be run on native Windows");
    #[cfg(windows)]
    native::run(true)
}

#[cfg(windows)]
mod native {
    use anyhow::{ensure, Context, Result};
    use hanamori::config::{ConfigContext, ConfigOverrides, Scope};
    use hanamori::shell::session;
    use hanamori::state::local;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn input(name: &str) -> Result<String> {
        std::env::var(name).with_context(|| format!("set {name} explicitly for native acceptance"))
    }

    fn cli(home: &Path, cwd: &Path, environment: &session::Environment) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hanamori"));
        command
            .env_clear()
            .envs(environment)
            .current_dir(cwd)
            .arg("--manager-dir")
            .arg(home);
        command
    }

    fn successful(command: &mut Command) -> Result<String> {
        let output = command
            .output()
            .context("cannot start native acceptance command")?;
        ensure!(
            output.status.success(),
            "native acceptance command failed ({}):\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn run(with_msbuild: bool) -> Result<()> {
        let home = PathBuf::from(input("HANAMORI_TEST_MANAGER_DIR")?);
        ensure!(
            home.is_absolute(),
            "HANAMORI_TEST_MANAGER_DIR must be absolute"
        );
        let ids = [
            input("HANAMORI_TEST_TOOLCHAIN_A")?,
            input("HANAMORI_TEST_TOOLCHAIN_B")?,
        ];
        ensure!(
            ids[0] != ids[1],
            "two distinct toolchain selectors are required"
        );
        let store = ConfigContext::from_environment(Default::default())?
            .resolve(
                &ConfigOverrides {
                    manager_dir: Some(home.clone()),
                    ..Default::default()
                },
                Scope::Local,
            )?
            .toolchains();
        let references_before =
            hanamori_test_support::toolchain::inventory(&home.join("references"))
                .context("cannot snapshot references")?;
        let directory = tempfile::Builder::new()
            .prefix("hanamori-native-acceptance-")
            .tempdir()?;
        let mut selections = Vec::new();
        for id in &ids {
            let selection = local::resolve(
                &store,
                &local::SelectionRequest::Selector(id),
                directory.path(),
            )?;
            ensure!(
                selection.layout().has_windows_sdk(),
                "each acceptance combination needs an SDK"
            );
            ensure!(
                !with_msbuild || selection.layout().msbuild_version.is_some(),
                "install both combinations with MSBuild enabled (the default)"
            );
            selections.push(selection);
        }
        ensure!(
            selections[0].layout().vc_tools_version != selections[1].layout().vc_tools_version,
            "acceptance requires two different actual compiler directory versions"
        );
        ensure!(
            selections[0].layout().windows_sdk_version
                != selections[1].layout().windows_sdk_version,
            "acceptance requires two different actual SDK directory versions"
        );
        let system =
            PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is missing")?);
        ensure!(
            system.is_absolute() && system.join("System32").is_dir(),
            "invalid SystemRoot"
        );
        // No inherited INCLUDE/LIB/CL/_CL_/LINK/LIBPATH, VS installation or shell
        // initialization may make an incomplete Hanamori environment pass.
        let baseline = session::Environment::from([
            ("SystemRoot".into(), system.clone().into_os_string()),
            ("WINDIR".into(), system.clone().into_os_string()),
            ("PATH".into(), system.join("System32").into_os_string()),
            ("TEMP".into(), directory.path().as_os_str().to_owned()),
            ("TMP".into(), directory.path().as_os_str().to_owned()),
            (
                "PROCESSOR_ARCHITECTURE".into(),
                input("PROCESSOR_ARCHITECTURE")?.into(),
            ),
        ]);
        let active_a = session::prepare(&selections[0], &home, &baseline)?;
        let mut fingerprints = Vec::new();
        for (attempt, selected) in [0, 1, 0].into_iter().enumerate() {
            let work = directory.path().join(format!("构建 {attempt}"));
            std::fs::create_dir(&work)?;
            std::fs::write(
                work.join("probe.cpp"),
                include_bytes!("../fixtures/native_build.cpp"),
            )?;
            std::fs::write(
                work.join("probe.rc"),
                include_bytes!("../fixtures/native_build.rc"),
            )?;
            std::fs::write(
                work.join("probe.manifest"),
                include_bytes!("../fixtures/native_build.manifest"),
            )?;
            let command = || {
                let mut command = cli(&home, &work, &baseline);
                if attempt == 1 {
                    active_a.apply(&mut command);
                }
                command.args(["exec", "--using", &ids[selected], "--"]);
                command
            };
            let layout = selections[selected].layout();
            let executable = if with_msbuild {
                build_msbuild(&command, &work, layout, selections[selected].key().unwrap())?
            } else {
                successful(command().args(["rc.exe", "/nologo", "/foprobe.res", "probe.rc"]))?;
                successful(command().args([
                    "cl.exe",
                    "/nologo",
                    "/EHsc",
                    "/std:c++17",
                    "/MT",
                    "/Feprobe.exe",
                    "/Foprobe.obj",
                    "probe.cpp",
                    "probe.res",
                    "/link",
                    "/MANIFEST:NO",
                ]))?;
                ensure!(
                    work.join("probe.exe").is_file(),
                    "compiler did not produce the requested executable"
                );
                successful(command().args([
                    "mt.exe",
                    "-nologo",
                    "-manifest",
                    "probe.manifest",
                    "-outputresource:probe.exe;#1",
                ]))?;
                work.join("probe.exe")
            };
            let output = successful(
                command().arg(executable).args([
                    &layout.vc_tools_version,
                    &layout.windows_sdk_version,
                    selections[selected]
                        .key()
                        .context("missing toolchain key")?,
                ]),
            )?;
            let fingerprint = output
                .trim()
                .strip_prefix("hanamori-native-build ")
                .context("built program did not produce the expected result")?
                .parse::<u32>()?;
            ensure!(fingerprint != 0, "compiler version is missing");
            eprintln!(
                "Verified {}: toolset {}, SDK {}, _MSC_FULL_VER {}",
                ids[selected], layout.vc_tools_version, layout.windows_sdk_version, fingerprint
            );
            fingerprints.push(fingerprint);
        }
        ensure!(
            fingerprints[0] != fingerprints[1],
            "both combinations executed the same compiler build"
        );
        ensure!(
            fingerprints[0] == fingerprints[2],
            "returning to the first combination changed its compiler"
        );
        ensure!(
            hanamori_test_support::toolchain::inventory(&home.join("references"))?
                == references_before,
            "acceptance unexpectedly modified the manager references"
        );
        Ok(())
    }

    fn build_msbuild(
        command: &impl Fn() -> Command,
        work: &Path,
        layout: &hanamori::env::ToolchainLayout,
        key: &str,
    ) -> Result<PathBuf> {
        let msbuild =
            hanamori::msbuild::Layout::for_version(layout.msbuild_version.as_deref().unwrap());
        let vc_root = format!("VCInstallDir_{}", msbuild.targets_version);
        let tools_root = format!("VCToolsInstallDir_{}", msbuild.targets_version);
        for (name, content) in [
            (
                "probe.vcxproj",
                include_str!("../fixtures/native_build.vcxproj"),
            ),
            (
                "library.vcxproj",
                include_str!("../fixtures/native_library.vcxproj"),
            ),
            ("probe.sln", include_str!("../fixtures/native_build.sln")),
            ("library.cpp", "int linked_value() { return 42; }\n"),
        ] {
            std::fs::write(
                work.join(name),
                content.replace("@PLATFORM_TOOLSET@", msbuild.platform_toolset),
            )?;
        }
        successful(command().args(["MSBuild.exe", "-nologo", "-version"]))?;
        for configuration in ["Debug", "Release"] {
            let build = |project: &str, target: &str| {
                let mut cmd = command();
                cmd.args(["MSBuild.exe", project, "-nologo", "-m:2", "-p:Platform=x64"])
                    .arg(format!("-p:Configuration={configuration}"))
                    .arg(format!("-t:{target}"));
                cmd
            };
            let properties = successful(
                command()
                    .args([
                        "MSBuild.exe",
                        "probe.vcxproj",
                        "-nologo",
                        "-p:Platform=x64",
                    ])
                    .arg(format!("-p:Configuration={configuration}"))
                    .arg(format!("-getProperty:MSBUILDDISABLENODEREUSE,{vc_root},{tools_root},VCInstallDir,VCToolsInstallDir,WindowsSdkDir,VCTargetsPath,CLTrackerFrameworkPath,CLTrackerSdkPath,LinkTrackerFrameworkPath,LibTrackerFrameworkPath,RCTrackerFrameworkPath,ManifestTrackerFrameworkPath")),
            )?;
            let properties: serde_json::Value = serde_json::from_str(&properties)?;
            ensure!(
                properties["Properties"]["MSBUILDDISABLENODEREUSE"] == "1",
                "MSBuild node reuse is not disabled by the selected environment"
            );
            for (name, expected) in [
                (vc_root.as_str(), layout.root.join("VC")),
                ("VCInstallDir", layout.root.join("VC")),
                (tools_root.as_str(), layout.tools_directory()),
                ("VCToolsInstallDir", layout.tools_directory()),
            ] {
                let path = properties["Properties"][name]
                    .as_str()
                    .context("missing evaluated compiler root")?;
                ensure!(
                    Path::new(path) == expected,
                    "{name} does not point at the selected compiler: {path}"
                );
            }
            for name in [
                "VCToolsInstallDir",
                "WindowsSdkDir",
                "VCTargetsPath",
                "CLTrackerFrameworkPath",
                "CLTrackerSdkPath",
                "LinkTrackerFrameworkPath",
                "LibTrackerFrameworkPath",
                "RCTrackerFrameworkPath",
                "ManifestTrackerFrameworkPath",
            ] {
                let path = properties["Properties"][name]
                    .as_str()
                    .context("missing evaluated build path")?;
                ensure!(
                    Path::new(path).starts_with(&layout.root),
                    "{name} escaped the selected toolchain: {path}"
                );
            }
            successful(build("probe.vcxproj", "Build").arg(format!("-bl:{configuration}.binlog")))?;
            let object = work.join(format!("obj/{configuration}/probe/probe.obj"));
            let modified = std::fs::metadata(&object)?.modified()?;
            successful(&mut build("probe.sln", "Build"))?;
            ensure!(
                std::fs::metadata(&object)?.modified()? == modified,
                "unchanged project recompiled"
            );
            let executable = work.join(format!("out/{configuration}/probe.exe"));
            successful(command().arg(&executable).args([
                &layout.vc_tools_version,
                &layout.windows_sdk_version,
                key,
            ]))?;
            std::fs::write(
                work.join("library.cpp"),
                "int linked_value() { return 41; }\n",
            )?;
            successful(&mut build("probe.sln", "Build"))?;
            let changed = command()
                .arg(&executable)
                .args([&layout.vc_tools_version, &layout.windows_sdk_version, key])
                .output()?;
            ensure!(
                changed.status.code() == Some(17),
                "incremental project reference did not rebuild/relink"
            );
            std::fs::write(
                work.join("library.cpp"),
                "int linked_value() { return 42; }\n",
            )?;
            successful(&mut build("probe.sln", "Clean"))?;
            ensure!(
                !executable.exists() && !object.exists(),
                "Clean left build outputs"
            );
            successful(&mut build("probe.sln", "Rebuild"))?;
            successful(command().arg(&executable).args([
                &layout.vc_tools_version,
                &layout.windows_sdk_version,
                key,
            ]))?;
        }
        // An explicit unavailable version must fail instead of using a system installation.
        let output = command()
            .args([
                "MSBuild.exe",
                "probe.vcxproj",
                "-nologo",
                "-p:Platform=x64",
                "-p:Configuration=Release",
                "-p:WindowsTargetPlatformVersion=10.0.99999.0",
            ])
            .output()?;
        ensure!(
            !output.status.success(),
            "an unavailable SDK unexpectedly built"
        );
        Ok(work.join("out/Release/probe.exe"))
    }
}
