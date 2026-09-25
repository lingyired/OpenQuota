use std::{
    io::{Read, Write},
    net::TcpListener,
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
