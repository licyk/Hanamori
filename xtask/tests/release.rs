use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn xtask(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(root)
        .env("CARGO", env!("CARGO"))
        .args(args)
        .output()
        .unwrap()
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn workspace() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join("src")).unwrap();
    fs::create_dir_all(root.join("member/src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\".\", \"member\"]\nresolver = \"2\"\n\n[workspace.package]\nversion = \"1.2.3\" # release version\nedition = \"2021\"\n\n[package]\nname = \"hanamori\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "").unwrap();
    fs::write(
        root.join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(root.join("member/src/lib.rs"), "").unwrap();
    success(
        Command::new(env!("CARGO"))
            .current_dir(root)
            .args(["generate-lockfile", "--offline"])
            .output()
            .unwrap(),
    );
    success(git(root, &["init", "--quiet"]));
    for (key, value) in [
        ("user.name", "Hanamori test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
    ] {
        success(git(root, &["config", key, value]));
    }
    success(git(root, &["add", "."]));
    success(git(root, &["commit", "--quiet", "-m", "Fixture"]));
    temp
}

#[test]
fn bump_updates_inherited_versions_and_lockfile_without_reformatting_the_manifest() {
    let temp = workspace();
    let root = temp.path();
    let original = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for (args, version) in [
        (vec!["bump"], "1.2.4"),
        (vec!["bump", "minor"], "1.3.0"),
        (vec!["bump", "major"], "2.0.0"),
    ] {
        success(xtask(&root.join("member"), &args));
        assert_eq!(
            fs::read_to_string(root.join("Cargo.toml")).unwrap(),
            original.replace("1.2.3", version)
        );
        let lock = fs::read_to_string(root.join("Cargo.lock"))
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        let packages = lock["package"].as_array_of_tables().unwrap();
        assert_eq!(packages.len(), 2);
        for package in packages {
            assert_eq!(package["version"].as_str().unwrap(), version);
        }
    }
    assert!(success(git(root, &["tag", "--list"])).stdout.is_empty());
}

#[test]
fn tag_uses_the_committed_version_and_pushes_only_the_requested_tag() {
    let temp = workspace();
    let root = temp.path();
    let remote = tempfile::tempdir().unwrap();
    success(git(remote.path(), &["init", "--bare", "--quiet"]));
    success(git(
        root,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    ));
    success(git(root, &["tag", "unrelated"]));

    success(xtask(&root.join("src"), &["tag"]));
    let first = success(git(root, &["rev-parse", "v1.2.3^{commit}"])).stdout;
    assert_eq!(first, success(git(root, &["rev-parse", "HEAD"])).stdout);
    assert!(!git(remote.path(), &["show-ref", "--tags"]).status.success());
    assert!(!xtask(root, &["tag", "--push"]).status.success());
    assert_eq!(
        first,
        success(git(root, &["rev-parse", "v1.2.3^{commit}"])).stdout
    );

    success(xtask(root, &["bump", "minor"]));
    let dirty = xtask(root, &["tag"]);
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("commit tracked changes"));
    success(git(root, &["add", "Cargo.toml", "Cargo.lock"]));
    assert!(!xtask(root, &["tag"]).status.success());
    success(git(root, &["commit", "--quiet", "-m", "Bump"]));
    let mismatch = xtask(root, &["tag", "1.3.1"]);
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("does not match workspace version"));
    success(xtask(root, &["tag", "1.3.0", "--push"]));
    assert_eq!(
        success(git(remote.path(), &["tag", "--list"])).stdout,
        b"v1.3.0\n"
    );
    assert_eq!(
        success(git(remote.path(), &["rev-parse", "v1.3.0^{commit}"])).stdout,
        success(git(root, &["rev-parse", "HEAD"])).stdout
    );
    assert!(!git(remote.path(), &["show-ref", "--heads"])
        .status
        .success());
}

#[test]
fn push_failure_keeps_the_created_tag_and_reports_the_retry_command() {
    let temp = workspace();
    let output = xtask(temp.path(), &["tag", "--push"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("git push origin refs/tags/v1.2.3"));
    assert_eq!(
        success(git(temp.path(), &["rev-parse", "v1.2.3^{commit}"])).stdout,
        success(git(temp.path(), &["rev-parse", "HEAD"])).stdout
    );
}

#[test]
fn invalid_arguments_and_lockfile_failures_leave_the_version_unchanged() {
    let temp = workspace();
    let root = temp.path();
    let original = fs::read(root.join("Cargo.toml")).unwrap();
    for args in [
        vec!["bump", "invalid"],
        vec!["tag", "invalid"],
        vec!["tag", "--unknown"],
    ] {
        assert_eq!(xtask(root, &args).status.code(), Some(2));
        assert_eq!(fs::read(root.join("Cargo.toml")).unwrap(), original);
    }
    fs::write(root.join("Cargo.lock"), "not a TOML lockfile").unwrap();
    assert!(!xtask(root, &["bump"]).status.success());
    assert_eq!(fs::read(root.join("Cargo.toml")).unwrap(), original);
    assert!(success(git(root, &["tag", "--list"])).stdout.is_empty());
}
