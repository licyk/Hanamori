use super::*;

fn fixture() -> (tempfile::TempDir, ConfigContext) {
    let temp = tempfile::tempdir().unwrap();
    let context = ConfigContext {
        cwd: temp.path().to_path_buf(),
        environment: BTreeMap::new(),
        platform_home: Some(temp.path().join("platform/Hanamori")),
    };
    (temp, context)
}

fn write_config(context: &ConfigContext, value: Value) {
    let home = context.platform_home.as_ref().unwrap();
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(home.join("config.json"), value.to_string()).unwrap();
}

#[test]
fn supplied_environment_drives_configuration_without_recapturing_it() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("snapshot manager");
    let context = ConfigContext::from_environment(BTreeMap::from([
        ("HANAMORI_HOME".into(), home.clone().into_os_string()),
        ("HANAMORI_LOG".into(), "warn".into()),
        ("HANAMORI_FILE_WORKERS".into(), "3".into()),
    ]))
    .unwrap();
    let config = context
        .resolve(&ConfigOverrides::default(), Scope::Install)
        .unwrap();
    assert_eq!(config.manager_dir, home);
    assert_eq!(config.log, "warn");
    assert_eq!(config.install.unwrap().file_workers.get(), 3);
    assert!(!home.exists());
}

#[test]
fn defaults_are_read_only_and_paths_record_the_home_source() {
    let (_temp, context) = fixture();
    let resolved = context
        .resolve(&ConfigOverrides::default(), Scope::All)
        .unwrap();
    let storage = resolved.install.as_ref().unwrap();
    assert_eq!(
        resolved.manager_dir,
        *context.platform_home.as_ref().unwrap()
    );
    assert_eq!(
        resolved.toolchains_dir,
        resolved.manager_dir.join("toolchains")
    );
    assert_eq!(
        resolved.to_json()["cache_dir"]["derived_from"]["source"],
        "default"
    );
    assert!(storage.download_dir.is_none());
    assert_eq!(resolved.log, "info");
    assert!(!resolved.manager_dir.exists());
}

#[test]
fn merges_each_field_without_cli_defaults_hiding_environment_or_file() {
    let (_temp, mut context) = fixture();
    write_config(
        &context,
        json!({"schema_version": 1, "file_workers": 2, "connections": 3, "log": "warn", "cache_dir": "缓存 path "}),
    );
    context
        .environment
        .insert("HANAMORI_FILE_WORKERS".into(), "6".into());
    context
        .environment
        .insert("HANAMORI_LOG".into(), "debug".into());
    let cli = ConfigOverrides {
        log: Some("error".into()),
        ..Default::default()
    };
    let config = context.resolve(&cli, Scope::All).unwrap();
    let download = config.install.as_ref().unwrap();
    assert_eq!(download.file_workers.get(), 6);
    assert_eq!(config.to_json()["file_workers"]["source"], "environment");
    assert_eq!(download.connections.get(), 3);
    assert_eq!(config.to_json()["connections"]["source"], "config_file");
    assert_eq!(config.log, "error");
    assert_eq!(config.to_json()["log"]["source"], "cli");
    assert_eq!(
        config.cache_dir.unwrap(),
        config.manager_dir.join("缓存 path ")
    );
}

#[test]
fn manager_bootstrap_selects_one_file_and_does_not_redirect_itself() {
    let (_temp, mut context) = fixture();
    write_config(
        &context,
        json!({"schema_version": 1, "manager_dir": "forbidden"}),
    );
    let config = context
        .resolve(&ConfigOverrides::default(), Scope::All)
        .unwrap();
    assert_eq!(config.manager_dir, context.platform_home.clone().unwrap());
    let env_home = context.cwd.join("env home");
    context
        .environment
        .insert("HANAMORI_HOME".into(), env_home.as_os_str().into());
    let config = context
        .resolve(&ConfigOverrides::default(), Scope::All)
        .unwrap();
    assert_eq!(config.manager_dir, env_home);
    assert_eq!(
        config.to_json()["cache_dir"]["derived_from"]["source"],
        "environment"
    );
    let cli = ConfigOverrides {
        manager_dir: Some("cli home".into()),
        ..Default::default()
    };
    assert_eq!(
        context.resolve(&cli, Scope::All).unwrap().manager_dir,
        context.cwd.join("cli home")
    );
    assert!(!env_home.exists());
}

#[test]
fn paths_use_the_correct_base_and_never_expand_shell_syntax() {
    let (_temp, mut context) = fixture();
    let cli = ConfigOverrides {
        cache_dir: Some("~/literal/$VAR/$(command)".into()),
        ..Default::default()
    };
    let config = context.resolve(&cli, Scope::All).unwrap();
    assert_eq!(
        config.cache_dir.unwrap(),
        context.cwd.join("~/literal/$VAR/$(command)")
    );
    for field in [
        "HANAMORI_HOME",
        "HANAMORI_TOOLCHAINS_DIR",
        "HANAMORI_CACHE_DIR",
        "HANAMORI_DOWNLOAD_DIR",
    ] {
        context.environment.clear();
        context.environment.insert(field.into(), "relative".into());
        assert!(
            context
                .resolve(&ConfigOverrides::default(), Scope::All)
                .is_err(),
            "{field}"
        );
        let path = context.cwd.join(" 中文 空格 ");
        context
            .environment
            .insert(field.into(), path.as_os_str().into());
        assert!(
            context
                .resolve(&ConfigOverrides::default(), Scope::All)
                .is_ok(),
            "{field}"
        );
    }
}

#[test]
fn empty_environment_is_absent_but_invalid_effective_values_do_not_fall_back() {
    let (_temp, mut context) = fixture();
    write_config(&context, json!({"schema_version": 1, "connections": 5}));
    context
        .environment
        .insert("HANAMORI_CONNECTIONS".into(), "".into());
    assert_eq!(
        context
            .resolve(&ConfigOverrides::default(), Scope::All)
            .unwrap()
            .install
            .unwrap()
            .connections
            .get(),
        5
    );
    for bad in [
        "0",
        "65",
        "-1",
        "abc",
        " 8",
        "999999999999999999999999999999999",
    ] {
        context
            .environment
            .insert("HANAMORI_CONNECTIONS".into(), bad.into());
        assert!(
            context
                .resolve(&ConfigOverrides::default(), Scope::All)
                .is_err(),
            "{bad}"
        );
    }
    let cli = ConfigOverrides {
        connections: Some(7),
        ..Default::default()
    };
    assert_eq!(
        context
            .resolve(&cli, Scope::All)
            .unwrap()
            .install
            .unwrap()
            .connections
            .get(),
        7
    );
}

#[test]
fn overridden_fields_skip_validation_and_config_accepts_extensions() {
    let (_temp, mut context) = fixture();
    write_config(
        &context,
        json!({"schema_version": 1, "connections": ["invalid"], "proxy": "secret invalid"}),
    );
    context
        .environment
        .insert("HANAMORI_PROXY".into(), "https://localhost:8080".into());
    let cli = ConfigOverrides {
        connections: Some(4),
        ..Default::default()
    };
    assert!(context.resolve(&cli, Scope::All).is_ok());
    for bad in [
        json!([]),
        json!({"schema_version": 2}),
        json!({"schema_version": "1"}),
    ] {
        write_config(&context, bad);
        assert!(context.resolve(&cli, Scope::All).is_err());
    }
    for compatible in [json!({}), json!({"schema_version": 1, "unknown": true})] {
        write_config(&context, compatible);
        assert!(context.resolve(&cli, Scope::All).is_ok());
    }
    std::fs::write(
        context.platform_home.as_ref().unwrap().join("config.json"),
        "{ not json secret",
    )
    .unwrap();
    let error = context.resolve(&cli, Scope::All).unwrap_err();
    assert!(format!("{error:#}").contains("line"));
}

#[test]
fn local_operations_ignore_unrelated_download_settings_and_legacy_names() {
    let (_temp, mut context) = fixture();
    for name in [
        "HANAMORI_CONNECTIONS",
        "HANAMORI_PROXY",
        "HANAMORI_CACHE_DIR",
        "RUST_LOG",
        "MSVC_INSTALLER_HOME",
        "_MSVC_INSTALLER_ACTIVE_ROOT",
    ] {
        context.environment.insert(name.into(), "invalid".into());
    }
    let config = context
        .resolve(&ConfigOverrides::default(), Scope::Local)
        .unwrap();
    assert_eq!(config.log, DEFAULT_LOG);
    assert_eq!(config.to_json()["manager_dir"]["source"], "default");
    assert!(config.install.is_none());
    assert!(config.proxy.is_none());
    context
        .environment
        .insert("HANAMORI_LOG".into(), "=invalid-level!".into());
    assert!(context
        .resolve(&ConfigOverrides::default(), Scope::Local)
        .is_err());
}

#[test]
fn configuration_does_not_read_or_migrate_old_project_files() {
    let (_temp, context) = fixture();
    std::fs::write(context.cwd.join("msvc-toolchain.json"), "invalid").unwrap();
    std::fs::create_dir(context.cwd.join(".msvc")).unwrap();
    context
        .resolve(&ConfigOverrides::default(), Scope::All)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(context.cwd.join("msvc-toolchain.json")).unwrap(),
        "invalid"
    );
    assert!(context.cwd.join(".msvc").is_dir());
    assert!(!context.platform_home.unwrap().exists());
}

#[test]
fn url_values_errors_and_debug_output_preserve_original_values() {
    let (_temp, context) = fixture();
    let cli = ConfigOverrides {
        proxy: Some("https://username:password@proxy.example:8080".into()),
        mirror: Some(
            "https://username:password@mirror.example/private-token?token=query-secret".into(),
        ),
        ..Default::default()
    };
    let config = context.resolve(&cli, Scope::All).unwrap();
    for output in [config.to_json().to_string(), format!("{config:?}")] {
        for secret in ["username", "password", "private-token", "query-secret"] {
            assert!(output.contains(secret));
        }
        assert!(output.contains("proxy.example"));
    }
    for url in [
        "invalid secret",
        "file:///secret",
        "https://host/secret",
        "https://host/?token=secret",
        "socks5://host:8080",
    ] {
        let cli = ConfigOverrides {
            proxy: Some(url.into()),
            ..Default::default()
        };
        let error = context.resolve(&cli, Scope::All).unwrap_err();
        assert!(format!("{error:#}").contains(url));
    }
}
