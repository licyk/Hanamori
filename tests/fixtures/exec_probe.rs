// Standalone native child used by exec integration tests; compiled with the test toolchain.
use std::ffi::OsStr;
use std::io::Read;

fn encode(value: &OsStr) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    };
    #[cfg(windows)]
    let bytes = {
        use std::os::windows::ffi::OsStrExt;
        value
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() {
    let arguments: Vec<_> = std::env::args_os().collect();
    let mut report = String::new();
    for value in &arguments[3..] {
        report.push_str(&format!("arg {}\n", encode(value)));
    }
    for (name, value) in std::env::vars_os() {
        report.push_str(&format!("env {} {}\n", encode(&name), encode(&value)));
    }
    report.push_str(&format!(
        "cwd {}\n",
        encode(std::env::current_dir().unwrap().as_os_str())
    ));
    std::fs::write(&arguments[1], report).unwrap();
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    use std::io::Write;
    std::io::stdout().write_all(&input).unwrap();
    println!("child stdout");
    eprintln!("child stderr");
    std::process::exit(arguments[2].to_str().unwrap().parse().unwrap());
}
