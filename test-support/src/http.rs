//! Small local HTTP fixture with concurrent connections and observable request routing.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub range: Option<(usize, usize)>,
    pub headers: String,
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub length: Option<usize>,
    pub delay: Duration,
}

impl Reply {
    pub fn full(body: &[u8]) -> Self {
        Self {
            status: 200,
            headers: vec![],
            body: body.to_vec(),
            length: Some(body.len()),
            delay: Duration::ZERO,
        }
    }

    pub fn range(request: &Request, body: &[u8]) -> Self {
        if let Some((start, end)) = request.range {
            let mut reply = Self::full(&body[start..=end]);
            reply.status = 206;
            reply.headers.push((
                "Content-Range".into(),
                format!("bytes {start}-{end}/{}", body.len()),
            ));
            reply
        } else {
            Self::full(body)
        }
    }

    pub fn error(status: u16) -> Self {
        Self {
            status,
            ..Self::full(b"")
        }
    }
}

pub struct Server {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let handler = Arc::new(handler);
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let handler = handler.clone();
                let captured = captured.clone();
                workers.push(std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).unwrap_or(0) == 0 {
                            return;
                        }
                        bytes.push(byte[0]);
                    }
                    let headers = String::from_utf8(bytes).unwrap();
                    let path = headers.split_whitespace().nth(1).unwrap().to_owned();
                    let range = headers.lines().find_map(|line| {
                        let lower = line.to_ascii_lowercase();
                        let range = lower.strip_prefix("range: bytes=")?;
                        let (start, end) = range.split_once('-')?;
                        Some((start.parse().unwrap(), end.parse().unwrap()))
                    });
                    let request = Request {
                        path,
                        range,
                        headers,
                    };
                    captured.lock().unwrap().push(request.clone());
                    let reply = handler(&request);
                    std::thread::sleep(reply.delay);
                    let mut headers =
                        format!("HTTP/1.1 {} Fixture\r\nConnection: close\r\n", reply.status);
                    if let Some(length) = reply.length {
                        headers.push_str(&format!("Content-Length: {length}\r\n"));
                    }
                    for (name, value) in reply.headers {
                        headers.push_str(&format!("{name}: {value}\r\n"));
                    }
                    headers.push_str("\r\n");
                    if stream.write_all(headers.as_bytes()).is_ok() {
                        let _ = stream.write_all(&reply.body);
                    }
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url: format!("http://{address}"),
            address,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}
