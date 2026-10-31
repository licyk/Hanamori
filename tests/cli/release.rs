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
    let output = directory.join("release-output.txt");
    std::fs::write(&output, "").unwrap();
    let mut command = Command::new("pwsh");
    command
        .current_dir(directory)
        .env("GITHUB_OUTPUT", output)
        .env_remove("CARGO_TARGET_DIR")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/scripts/release.ps1"))
        .args(["-Stage", stage]);
    if !tag.is_empty() {
        command.args(["-Tag", tag]);
    }
    command.output().unwrap()
}

#[test]
fn release_resolves_latest_or_explicit_tags_without_changing_source() {
    if !powershell_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    tagged_source(temp.path(), "1.0.0");
    let git = |args: &[&str]| {
        successful(
            Command::new("git")
                .current_dir(temp.path())
                .args(args)
                .output()
                .unwrap(),
        )
    };
    git(&[
        "-c",
        "user.name=Hanamori test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "tag.gpgsign=false",
        "tag",
        "-a",
        "v10.0.0",
        "-m",
        "Annotated release",
    ]);
    git(&["-c", "tag.gpgsign=false", "tag", "v2.0.0"]);
    git(&["-c", "tag.gpgsign=false", "tag", "unrelated"]);
    let commit = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout).unwrap();
    git(&[
        "-c",
        "user.name=Hanamori test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--allow-empty",
        "-m",
        "After tags",
    ]);
    let head = git(&["rev-parse", "HEAD"]).stdout;

    for (requested, expected) in [("", "v10.0.0"), ("v2.0.0", "v2.0.0")] {
        successful(release(temp.path(), "Resolve", requested));
        let output = std::fs::read_to_string(temp.path().join("release-output.txt")).unwrap();
        assert!(output.lines().any(|line| line == format!("tag={expected}")));
        assert!(output
            .lines()
            .any(|line| line == format!("commit={}", commit.trim())));
        assert_eq!(git(&["rev-parse", "HEAD"]).stdout, head);
    }
    let mismatch = release(temp.path(), "Verify", "v1.0.0");
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("does not match the release tag"));
    let missing = release(temp.path(), "Resolve", "v999.0.0");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("must already exist"));
    git(&["branch", "vbranch"]);
    let branch = release(temp.path(), "Resolve", "vbranch");
    assert!(!branch.status.success());
    assert!(String::from_utf8_lossy(&branch.stderr).contains("must already exist"));
    git(&["tag", "-d", "v1.0.0", "v2.0.0", "v10.0.0"]);
    let empty = release(temp.path(), "Resolve", "");
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("No existing v* release tag"));
}

#[test]
fn release_script_checks_real_tag_and_cargo_version() {
    if !powershell_available() {
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

fn powershell_available() -> bool {
    let available = Command::new("pwsh")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert!(
            std::env::var_os("HANAMORI_REQUIRE_SHELL_TESTS").is_none(),
            "pwsh is required"
        );
        eprintln!("skipping release script integration: pwsh is unavailable");
    }
    available
}
