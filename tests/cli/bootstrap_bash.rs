use crate::support::success;
#[cfg(windows)]
use std::path::PathBuf;

#[test]
fn bash_installer_help_and_argument_errors_need_no_network() {
    let Some(shell) = crate::support::shell_path("bash") else {
        return;
    };
    let source = include_str!("../../install.sh");
    let output = |arguments: &[&str]| {
        crate::support::shell_command(&shell)
            .args(["--noprofile", "--norc", "-c", source, "install.sh"])
            .args(arguments)
            .output()
            .unwrap()
    };
    assert!(success(output(&["--help"])).contains("MSYS2 or Git Bash"));
    for arguments in [
        vec!["--unknown"],
        vec!["--version"],
        vec!["--install-dir", ""],
    ] {
        assert!(!output(&arguments).status.success());
    }
    #[cfg(not(windows))]
    {
        let rejected = output(&[]);
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("requires MSYS2 or Git Bash"));
    }
}

#[test]
#[cfg(windows)]
fn native_msys_installers_download_and_initialize_their_own_bash() {
    use hanamori_test_support::http::{Reply, Server};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fs;
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let bytes = fs::read(env!("CARGO_BIN_EXE_hanamori")).unwrap();
    let size = bytes.len();
    let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    let asset = Server::new(move |_| Reply::full(&bytes));
    let url = asset.url.clone();
    let api = Server::new(move |request| {
        let broken = request.path.ends_with("/v9.9.9");
        let body = json!({
            "tag_name": if broken { "v9.9.9" } else { concat!("v", env!("CARGO_PKG_VERSION")) },
            "draft": false, "prerelease": false,
            "assets": [{"name": hanamori::upgrade::ASSET_NAME, "size": size,
                "browser_download_url": url,
                "digest": if broken { format!("sha256:{}", "0".repeat(64)) } else { digest.clone() }}]
        });
        let mut reply = Reply::full(&serde_json::to_vec(&body).unwrap());
        reply
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        reply
    });
    let backend = include_str!("../../install.ps1").replace(
        "Install-Hanamori @PSBoundParameters",
        "Install-Hanamori @PSBoundParameters -ReleaseApi $env:INSTALLER_API -EnvironmentKey $env:INSTALLER_REGISTRY",
    );
    let script_bytes = backend.as_bytes().to_vec();
    let scripts = Server::new(move |_| Reply::full(&script_bytes));
    let Some(shell) = crate::support::shell_path("bash") else {
        return;
    };
    let mut shells = vec![shell];
    let explicit = std::env::var_os("HANAMORI_TEST_MSYS2_BASH");
    let msys = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(format!(
            "{}\\msys64\\usr\\bin\\bash.exe",
            std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
        ))
    });
    if msys.is_file() {
        shells.push(msys);
    } else {
        assert!(
            explicit.is_none(),
            "configured MSYS2 Bash is missing: {}",
            msys.display()
        );
        eprintln!(
            "SKIPPED MSYS2 installer: {} is unavailable; set HANAMORI_TEST_MSYS2_BASH",
            msys.display()
        );
    }
    for shell in shells {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("用户 '& [test]");
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("install.sh"),
            include_str!("../../install.sh")
                .replace("https://hanamori.netlify.app/install.ps1", &scripts.url),
        )
        .unwrap();
        // Inject only isolated HTTP/registry locations; execute the actual installer functions.
        fs::write(directory.join("install.ps1"), &backend).unwrap();
        let location = format!(
            r"Software\Hanamori-Bash-Installer-Test-{}",
            uuid::Uuid::new_v4()
        );
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = root.create_subkey(&location).unwrap();
        key.set_value("Path", &r"D:\Keep").unwrap();
        let output = crate::support::shell_command(&shell)
            .args([
                "--noprofile",
                "--norc",
                "-c",
                include_str!("bootstrap_bash.sh"),
            ])
            .env("INSTALLER_ROOT", &directory)
            .env("INSTALLER_API", &api.url)
            .env("INSTALLER_REGISTRY", &location)
            .env("INSTALLER_VERSION", env!("CARGO_PKG_VERSION"))
            .env("LOCALAPPDATA", temp.path().join("data"))
            .env("HANAMORI_HOME", temp.path().join("manager"))
            .env_remove("GITHUB_TOKEN")
            .env_remove("BASH_ENV")
            .env_remove("PSModulePath")
            .output()
            .unwrap();
        let path: String = key.get_value("Path").unwrap();
        drop(key);
        root.delete_subkey_all(&location).unwrap();
        crate::support::assert_success(output);
        assert!(path.ends_with(r";D:\Keep"), "{path}");
        assert_eq!(
            path.split(';').count(),
            2,
            "duplicate user PATH entry: {path}"
        );
        assert!(!temp.path().join("manager/toolchains").exists());
        assert!(!directory.join("bare/hanamori.exe.install.json").exists());
    }
    assert!(!scripts.requests.lock().unwrap().is_empty());
}
