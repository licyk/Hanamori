//! Native Windows coverage of replacement while an installed executable is running.
use super::{installed, Catalog};
use crate::support::{copy_executable, exec_probe, success};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn replacement_closes_only_users_of_the_old_installation_after_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(false, false);
    success(catalog.install(&temp).output().unwrap());
    let root = installed(&temp).path;
    let other = root.with_file_name(format!(
        "{}-other",
        root.file_name().unwrap().to_string_lossy()
    ));
    std::fs::create_dir(&other).unwrap();
    let spawn = |directory: &std::path::Path, report: &str| {
        let program = directory.join("worker.exe");
        copy_executable(exec_probe(), &program);
        Running(
            Command::new(&program)
                // Restart Manager must not close the test runner's shared console.
                .creation_flags(0x08000000)
                .current_dir(temp.path())
                .arg(temp.path().join(report))
                .arg("0")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };
    let mut held = spawn(&root, "held-report");
    let mut unrelated = spawn(&other, "other-report");

    success(catalog.install(&temp).arg("--dry-run").output().unwrap());
    success(catalog.install(&temp).output().unwrap());
    assert!(held.0.try_wait().unwrap().is_none());

    let broken = Catalog::new(true, false);
    let failed = broken.install(&temp).arg("--reinstall").output().unwrap();
    assert!(!failed.status.success());
    assert!(held.0.try_wait().unwrap().is_none());
    assert!(root.join("worker.exe").is_file());

    let output = catalog.install(&temp).arg("--reinstall").output().unwrap();
    let messages = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    success(output);
    assert!(messages.contains(&format!("PID {}", held.0.id())));
    assert!(held.0.try_wait().unwrap().is_some());
    assert!(unrelated.0.try_wait().unwrap().is_none());
    assert!(!root.join("worker.exe").exists());
    assert_eq!(installed(&temp).path, root);
}
