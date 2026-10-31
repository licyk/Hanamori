use std::path::Path;

/// Synthetic layout for management tests, not executable Windows compiler payloads.
pub fn toolchain(root: &Path, toolset: &str, sdk: Option<&str>) {
    let tools = root.join("VC/Tools/MSVC").join(toolset);
    for relative in [
        "bin/Hostx64/x64/cl.exe",
        "bin/Hostx64/x64/link.exe",
        "bin/Hostx64/x64/lib.exe",
        "include/vcruntime.h",
        "include/vector",
        "lib/x64/libcmt.lib",
        "lib/x64/libcpmt.lib",
        "lib/x64/vcruntime.lib",
    ] {
        let path = tools.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"synthetic fixture").unwrap();
    }
    if let Some(sdk) = sdk {
        let kits = root.join("Windows Kits/10");
        for relative in ["um/Windows.h", "shared/sdkddkver.h", "ucrt/stdio.h"] {
            let path = kits.join("Include").join(sdk).join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"synthetic SDK header").unwrap();
        }
        for relative in ["um/x64/kernel32.Lib", "ucrt/x64/ucrt.lib"] {
            let path = kits.join("Lib").join(sdk).join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"synthetic SDK library").unwrap();
        }
        for relative in [
            "rc.exe",
            "rcdll.dll",
            "mt.exe",
            "midlrtmd.dll",
            "ServicingCommon.dll",
        ] {
            let path = kits.join("bin").join(sdk).join("x64").join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"synthetic SDK tool").unwrap();
        }
    }
}

/// Snapshot fixture bytes to detect unintended writes during CLI operations.
pub fn inventory(
    root: &Path,
) -> std::io::Result<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>> {
    fn scan(
        root: &Path,
        path: &Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                scan(root, &path, out)?;
            } else if kind.is_file() {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(&path)?,
                );
            }
        }
        Ok(())
    }
    let mut out = std::collections::BTreeMap::new();
    scan(root, root, &mut out)?;
    Ok(out)
}
