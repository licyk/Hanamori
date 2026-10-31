// A native stand-in for profile discovery, validating the argument protocol on both OSes.
use std::io::Write;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(&args[..4], &["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]);
    assert_eq!(args.len(), 5);
    assert!(args[4].contains("$PROFILE.CurrentUserAllHosts"));
    assert!(std::env::var_os("BASH_ENV").is_none());
    assert!(std::env::var_os("ENV").is_none());
    match std::env::var("HANAMORI_TEST_PROBE_MODE").unwrap_or_default().as_str() {
        "failure" => { eprintln!("private startup value"); std::process::exit(9); }
        "oversized" => { let _ = std::io::stdout().write_all(&vec![b'x'; 2 * 1024 * 1024]); }
        "invalid" => println!("private startup value"),
        _ => println!("{}", std::env::var("HANAMORI_TEST_PROFILE_RESPONSE").unwrap()),
    }
}
