use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn successful(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn tagged_source(directory: &Path, version: &str) {
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::write(
        directory.join("Cargo.toml"),
        format!("[package]\nname = \"hanamori\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
    )
    .unwrap();
    std::fs::write(directory.join("src/main.rs"), "fn main() {}\n").unwrap();
    successful(
        Command::new(env!("CARGO"))
            .current_dir(directory)
            .args(["generate-lockfile", "--offline"])
            .output()
            .unwrap(),
    );
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "Cargo.toml", "Cargo.lock", "src/main.rs"],
        vec![
            "-c",
            "user.name=Hanamori test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "Release fixture",
        ],
    ] {
        successful(
            Command::new("git")
                .current_dir(directory)
                .args(args)
                .output()
                .unwrap(),
        );
    }
    successful(
        Command::new("git")
            .current_dir(directory)
            .args(["-c", "tag.gpgsign=false", "tag", &format!("v{version}")])
            .output()
            .unwrap(),
    );
}

fn release(directory: &Path, stage: &str, tag: &str) -> Output {
    Command::new("pwsh")
        .current_dir(directory)
        .env_remove("GITHUB_OUTPUT")
        .env_remove("CARGO_TARGET_DIR")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/scripts/release.ps1"))
        .args(["-Stage", stage, "-Tag", tag])
        .output()
        .unwrap()
}

#[test]
fn release_script_checks_real_tag_and_cargo_version() {
    if !Command::new("pwsh")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "pwsh is required"
        );
        eprintln!("skipping release script integration: pwsh is unavailable");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    tagged_source(temp.path(), hanamori::identity::VERSION);
    let tag = format!("v{}", hanamori::identity::VERSION);
    successful(release(temp.path(), "Verify", &tag));
    successful(
        Command::new("git")
            .current_dir(temp.path())
            .args(["-c", "tag.gpgsign=false", "tag", "v99.0.0"])
            .output()
            .unwrap(),
    );
    let mismatch = release(temp.path(), "Verify", "v99.0.0");
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8_lossy(&mismatch.stderr).contains("does not match Cargo package version")
    );

    #[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
    {
        let binary = temp
            .path()
            .join("target/x86_64-pc-windows-msvc/release/hanamori.exe");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_hanamori"), &binary).unwrap();
        successful(release(temp.path(), "Package", &tag));
        assert!(temp.path().join(hanamori::upgrade::ASSET_NAME).exists());
        assert!(temp
            .path()
            .join("hanamori-x86_64-pc-windows-msvc.zip")
            .exists());

        let wrong = tempfile::tempdir().unwrap();
        tagged_source(wrong.path(), "99.0.0");
        let binary = wrong
            .path()
            .join("target/x86_64-pc-windows-msvc/release/hanamori.exe");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_hanamori"), binary).unwrap();
        let mismatch = release(wrong.path(), "Package", "v99.0.0");
        assert!(!mismatch.status.success());
        assert!(String::from_utf8_lossy(&mismatch.stderr)
            .contains("does not report the release version"));
        assert!(!wrong.path().join(hanamori::upgrade::ASSET_NAME).exists());
    }
}
