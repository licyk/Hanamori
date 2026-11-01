use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

pub struct FileUser(Child);

impl FileUser {
    pub fn new(path: &Path) -> Self {
        static PROGRAM: OnceLock<tempfile::TempDir> = OnceLock::new();
        let program = super::compiled(&PROGRAM, "file_holder");
        let mut command = Command::new(program);
        super::clear_environment(&mut command);
        let mut child = Self(
            command
                .arg(path)
                // Restart Manager must not close the test runner's shared console.
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let mut ready = String::new();
        BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        child
    }

    pub fn is_running(&mut self) -> bool {
        self.0.try_wait().unwrap().is_none()
    }
}

impl Drop for FileUser {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
