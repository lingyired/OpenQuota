use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

pub const TIMEOUT_TEST_CLIENT_LIMIT: Duration = Duration::from_millis(100);
pub const TIMEOUT_TEST_RESPONSE_DELAY: Duration = Duration::from_secs(1);

pub fn serve_once(status: u16, headers: &[(&str, &str)], body: &str) -> String {
    serve_once_after(Duration::ZERO, status, headers, body)
}

pub fn serve_once_after(
    delay: Duration,
    status: u16,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let headers = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let body = body.to_owned();

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        thread::sleep(delay);
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            429 => "Too Many Requests",
            _ => "Test Response",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });

    format!("http://{address}")
}

/// 按顺序在同一个监听端口上服务多个响应，每个连接一个。
///
/// 一次设备码轮询会先换 token、再取账号资料，两次请求必须打到同一个 base URL；
/// `serve_once` 只接受一次连接，所以这类多请求流程用它测不出来。
/// 已有的 `serve_once` / `serve_once_after` 保持原样：其他 provider 的测试都共用它们。
pub fn serve_sequence(responses: &[(u16, &str)]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let responses = responses
        .iter()
        .map(|(status, body)| (*status, (*body).to_owned()))
        .collect::<Vec<_>>();

    thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let reason = match status {
                200 => "OK",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                429 => "Too Many Requests",
                _ => "Test Response",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    format!("http://{address}")
}

pub fn serve_once_capturing_request(
    status: u16,
    body: &str,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let body = body.to_owned();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("mock request should connect");
        let request = capture_request_headers(&mut stream);
        write_capture_response(&mut stream, status, &body);
        request
    });

    (format!("http://{address}"), server)
}

pub fn serve_sequence_capturing_requests(
    responses: &[(u16, &str)],
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    let responses = responses
        .iter()
        .map(|(status, body)| (*status, (*body).to_owned()))
        .collect::<Vec<_>>();
    let server = thread::spawn(move || {
        let mut requests = Vec::with_capacity(responses.len());
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().expect("mock request should connect");
            requests.push(capture_request_headers(&mut stream));
            write_capture_response(&mut stream, status, &body);
        }
        requests
    });

    (format!("http://{address}"), server)
}

fn capture_request_headers(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk).unwrap_or(0);
        if count == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..count]);
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            request.truncate(end + 4);
            break;
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn write_capture_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        429 => "Too Many Requests",
        _ => "Test Response",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[cfg(test)]
mod tests {
    use reqwest::blocking::Client;

    use super::{serve_once_capturing_request, serve_sequence_capturing_requests};

    #[test]
    fn capture_helper_returns_request_headers() {
        let (url, server) = serve_once_capturing_request(200, "captured");

        let body = Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(url)
            .send()
            .unwrap()
            .text()
            .unwrap();
        let request = server.join().unwrap();

        assert_eq!(body, "captured");
        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        assert!(request.to_ascii_lowercase().contains("\r\nhost:"));
    }

    #[test]
    fn sequence_capture_helper_returns_each_request() {
        let (url, server) = serve_sequence_capturing_requests(&[(200, "first"), (200, "second")]);
        let client = Client::builder().no_proxy().build().unwrap();

        assert_eq!(client.get(&url).send().unwrap().text().unwrap(), "first");
        assert_eq!(client.get(&url).send().unwrap().text().unwrap(), "second");
        let requests = server.join().unwrap();

        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.starts_with("GET / HTTP/1.1\r\n")));
    }
}
