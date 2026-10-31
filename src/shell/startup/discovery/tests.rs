use super::*;

#[test]
fn inherited_locations_respect_zdotdir_and_reject_ambiguous_paths() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let zdir = temp.path().join("zsh config");
    assert_eq!(
        inherited_posix(
            Shell::Bash,
            Some(home.clone()),
            Some(zdir.clone().into_os_string())
        )
        .unwrap()
        .path,
        home.join(".bashrc")
    );
    assert_eq!(
        inherited_posix(
            Shell::Zsh,
            Some(home.clone()),
            Some(zdir.clone().into_os_string())
        )
        .unwrap()
        .path,
        zdir.join(".zshrc")
    );
    assert_eq!(
        inherited_posix(Shell::Zsh, Some(home.clone()), None)
            .unwrap()
            .path,
        home.join(".zshrc")
    );
    assert!(inherited_posix(Shell::Zsh, Some(home), Some("".into())).is_err());
    assert_eq!(
        inherited_posix(Shell::Zsh, None, Some("relative".into()))
            .unwrap()
            .path,
        std::path::absolute("relative/.zshrc").unwrap()
    );
    assert!(inherited_posix(Shell::Bash, None, None).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn timed_out_discovery_helper_is_stopped_and_reaped() {
    let temp = tempfile::tempdir().unwrap();
    let pid_file = temp.path().join("pid");
    let error = query(
        OsStr::new("/bin/sh"),
        &[
            "-c",
            "echo $$ > \"$1\"; exec sleep 30",
            "query-probe",
            pid_file.to_str().unwrap(),
        ],
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    let pid = std::fs::read_to_string(pid_file).unwrap();
    assert!(
        !Path::new("/proc").join(pid.trim()).exists(),
        "discovery process was not reaped"
    );
}

#[test]
fn explicit_profiles_bypass_discovery_and_cmd_is_not_a_file_profile() {
    let path = std::path::PathBuf::from("explicit profile");
    assert_eq!(
        discover(Shell::PowerShell, Some(path.clone()), None, None, None)
            .unwrap()
            .path,
        std::path::absolute(&path).unwrap()
    );
    assert!(discover(
        Shell::PowerShell,
        Some(path.clone()),
        Some("missing-program".into()),
        None,
        None,
    )
    .is_err());
    assert!(discover(Shell::Cmd, Some(path), None, None, None).is_err());
}
