use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;

fn main() {
    let path = std::env::args_os().nth(1).unwrap();
    let _file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap();
    println!("ready");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
}
