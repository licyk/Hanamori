use std::os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt};
use std::path::Path;
use std::process::Command;
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::Threading::{
        CreateProcessW, GetExitCodeProcess, WaitForSingleObject, CREATE_NEW_CONSOLE,
        CREATE_NO_WINDOW, INFINITE, PROCESS_INFORMATION, STARTF_USESHOWWINDOW,
        STARTF_USESTDHANDLES, STARTUPINFOW,
    },
    UI::WindowsAndMessaging::SW_HIDE,
};

pub fn command(script: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    super::clear_environment(&mut command)
        .args(["--exact", "support::console::child", "--nocapture"])
        .env(
            "PATH",
            Path::new(&std::env::var_os("SystemRoot").unwrap()).join("System32"),
        )
        .env("HANAMORI_TEST_CONSOLE_SCRIPT", script)
        .creation_flags(CREATE_NO_WINDOW);
    command
}

#[test]
fn child() {
    let Some(script) = std::env::var_os("HANAMORI_TEST_CONSOLE_SCRIPT") else {
        return;
    };
    let program = Path::new(&std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    let application: Vec<u16> = program.as_os_str().encode_wide().chain([0]).collect();
    let mut arguments = std::ffi::OsString::from("cmd.exe /d /v:off /c \"\"");
    arguments.push(script);
    arguments.push("\"\"");
    let mut arguments: Vec<u16> = arguments.encode_wide().chain([0]).collect();
    // DOSKEY needs a real console; CREATE_NO_WINDOW would bypass the behavior under test.
    // Stable CommandExt cannot set STARTUPINFO.wShowWindow, so launch the hidden console here.
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES,
        wShowWindow: SW_HIDE as u16,
        hStdInput: std::io::stdin().as_raw_handle(),
        hStdOutput: std::io::stdout().as_raw_handle(),
        hStdError: std::io::stderr().as_raw_handle(),
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    let mut code = 0;
    unsafe {
        assert_ne!(
            CreateProcessW(
                application.as_ptr(),
                arguments.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_NEW_CONSOLE,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut process,
            ),
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        CloseHandle(process.hThread);
        assert_eq!(
            WaitForSingleObject(process.hProcess, INFINITE),
            WAIT_OBJECT_0
        );
        assert_ne!(GetExitCodeProcess(process.hProcess, &mut code), 0);
        CloseHandle(process.hProcess);
    }
    std::process::exit(code as i32);
}
