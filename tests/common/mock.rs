//! A tiny in-process stand-in for Open Cloud, for testing `--cloud` without
//! network access or a key (tests/cloud_mock.rs).
//!
//! It answers each request with whatever the test's handler returns, and
//! records every request so the test can check exactly what studio-run sent.
//! What the responses look like follows docs/cloud-behavior.md, which the
//! weekly real-API runs re-check.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// Path and query, e.g. `/cloud/v2/universes/1/...?view=FLAT`.
    pub target: String,
    /// Header names in lowercase.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("JSON request body")
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, body: serde_json::Value) -> Response {
        Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
        }
    }

    pub fn bytes(body: impl Into<Vec<u8>>) -> Response {
        Response {
            status: 200,
            headers: vec![("Content-Type".into(), "application/octet-stream".into())],
            body: body.into(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.into(), value.into()));
        self
    }
}

type Handler = dyn Fn(&Request) -> Response + Send + Sync;

pub struct MockApi {
    /// `http://127.0.0.1:<port>`: pass as STUDIO_RUN_TEST_API_BASE.
    pub base: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl MockApi {
    /// Starts a server on a free loopback port. It runs until the test process exits.
    pub fn start(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> MockApi {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let handler: Arc<Handler> = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let recorded = recorded.clone();
                let handler = handler.clone();
                std::thread::spawn(move || serve(stream, &recorded, &*handler));
            }
        });
        MockApi { base, requests }
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    /// Requests whose method is `method` and whose path ends with `suffix`.
    pub fn matching(&self, method: &str, suffix: &str) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == method && r.path().ends_with(suffix))
            .collect()
    }
}

fn serve(stream: TcpStream, recorded: &Mutex<Vec<Request>>, handler: &Handler) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
    {
        read_chunked(&mut reader)
    } else {
        let len: usize = headers
            .get("content-length")
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; len];
        let _ = reader.read_exact(&mut body);
        body
    };
    let request = Request {
        method,
        target,
        headers,
        body,
    };
    let response = handler(&request);
    recorded.lock().unwrap().push(request);

    let mut out = format!(
        "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.body.len()
    );
    for (k, v) in &response.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    let mut stream = stream;
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

fn read_chunked(reader: &mut impl BufRead) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line).unwrap_or(0) == 0 {
            break;
        }
        let size = usize::from_str_radix(size_line.trim().split(';').next().unwrap_or("0"), 16)
            .unwrap_or(0);
        if size == 0 {
            let mut end = String::new();
            let _ = reader.read_line(&mut end);
            break;
        }
        let mut chunk = vec![0; size];
        let _ = reader.read_exact(&mut chunk);
        body.extend_from_slice(&chunk);
        let mut crlf = [0; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    body
}
