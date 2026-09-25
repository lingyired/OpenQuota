use serde_json::json;

use super::login::{DeviceCodeLogin, LoginPoll, WorkBuddyLoginError, LOGIN_TTL_SECONDS};
use crate::providers::test_http;

/// `poll`/`cancel` 的测试一律用 `register_for_test` 直接登记一个待授权尝试：
/// `test_http::serve_once` 只接受一次连接，先 `start()` 再 `poll()` 的写法会让
/// 第二次请求打到已经关闭的服务器上，从而因为错误的原因通过。
#[test]
fn start_returns_the_state_and_authorization_url() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"state": "st-1", "authUrl": "https://example.test/auth?state=st-1"}})
            .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);

    let challenge = login.start().unwrap();

    assert_eq!(
        challenge.verification_uri,
        "https://example.test/auth?state=st-1"
    );
    assert_eq!(challenge.expires_in, LOGIN_TTL_SECONDS);
    assert!(!challenge.login_id.is_empty());
    assert_ne!(
        challenge.login_id, "st-1",
        "the generated login id must stay distinct from the server-issued state"
    );
}

#[test]
fn start_accepts_an_alternate_authorization_url_key() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"state": "st-1", "auth_url": "https://example.test/b"}})
            .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);

    assert_eq!(
        login.start().unwrap().verification_uri,
        "https://example.test/b"
    );
}

/// 授权地址来自响应体，不是编译期常量：只有 https 才能交给系统默认处理器，
/// 否则一个被劫持的响应就能让应用在本机打开任意 scheme。
#[test]
fn start_rejects_an_authorization_url_that_is_not_https() {
    for uri in [
        "http://example.test/auth",
        "file:///etc/passwd",
        "//example.test/auth",
    ] {
        let server = test_http::serve_once(
            200,
            &[],
            &json!({"code": 0, "data": {"state": "st-1", "authUrl": uri}}).to_string(),
        );
        let login = DeviceCodeLogin::for_test(&server);

        assert!(
            matches!(login.start(), Err(WorkBuddyLoginError::InvalidResponse)),
            "a non-https authorization URL must never become a challenge: {uri}"
        );
    }
}

#[test]
fn start_without_a_state_is_an_invalid_response() {
    let server = test_http::serve_once(200, &[], &json!({"code": 0, "data": {}}).to_string());
    let login = DeviceCodeLogin::for_test(&server);

    assert!(matches!(
        login.start(),
        Err(WorkBuddyLoginError::InvalidResponse)
    ));
}

#[test]
fn start_without_an_authorization_url_is_an_invalid_response() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"state": "st-1"}}).to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);

    assert!(matches!(
        login.start(),
        Err(WorkBuddyLoginError::InvalidResponse)
    ));
}

#[test]
fn start_reports_a_transport_failure_as_connection() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");

    assert!(matches!(
        login.start(),
        Err(WorkBuddyLoginError::Connection)
    ));
}

#[test]
fn polling_before_authorization_stays_pending() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 12153, "msg": "pending"}).to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    assert!(matches!(login.poll(&login_id), LoginPoll::Pending));
}

#[test]
fn polling_after_authorization_returns_the_session() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {
                "accessToken": "access-1",
                "refreshToken": "refresh-1",
                "tokenType": "Bearer",
                "domain": "www.codebuddy.cn",
                "expiresAt": 1_800_000_000_000i64,
                "refreshExpiresAt": 1_800_600_000_000i64
            }
        })
        .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(session.access_token, "access-1");
            assert_eq!(session.refresh_token.as_deref(), Some("refresh-1"));
            assert_eq!(session.token_type, "Bearer");
            assert_eq!(session.domain, "www.codebuddy.cn");
            assert_eq!(session.expires_at, Some(1_800_000_000_000));
            assert_eq!(session.refresh_expires_at, Some(1_800_600_000_000));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn polling_merges_the_account_profile_into_the_session() {
    // 换 token 与取资料是两次请求，所以这里必须用 `serve_sequence`：
    // `serve_once` 只接受一次连接，资料请求会打到已关闭的端口上。
    let server = test_http::serve_sequence(&[
        (
            200,
            &json!({
                "code": 0,
                "data": {
                    "accessToken": "access-1",
                    "refreshToken": "refresh-1",
                    "domain": "www.codebuddy.cn"
                }
            })
            .to_string(),
        ),
        (
            200,
            &json!({
                "code": 0,
                "data": {
                    "uid": "uid-1",
                    "nickname": "Ling",
                    "email": "ling@example.com",
                    "enterpriseId": "ent-1"
                }
            })
            .to_string(),
        ),
    ]);
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(session.access_token, "access-1");
            assert_eq!(session.uid.as_deref(), Some("uid-1"));
            assert_eq!(session.nickname.as_deref(), Some("Ling"));
            assert_eq!(session.email.as_deref(), Some("ling@example.com"));
            assert_eq!(session.enterprise_id.as_deref(), Some("ent-1"));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn polling_accepts_snake_case_profile_spellings() {
    let server = test_http::serve_sequence(&[
        (
            200,
            &json!({
                "code": 0,
                "data": {"accessToken": "access-1", "domain": "codebuddy.cn"}
            })
            .to_string(),
        ),
        (
            200,
            &json!({
                "code": 0,
                "data": {"nick_name": "Ling", "enterprise_id": "ent-1"}
            })
            .to_string(),
        ),
    ]);
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(session.nickname.as_deref(), Some("Ling"));
            assert_eq!(session.enterprise_id.as_deref(), Some("ent-1"));
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn a_failed_profile_fetch_keeps_the_token_only_session() {
    let server = test_http::serve_sequence(&[
        (
            200,
            &json!({
                "code": 0,
                "data": {"accessToken": "access-1", "domain": "www.codebuddy.cn"}
            })
            .to_string(),
        ),
        (500, r#"{"code":500,"msg":"boom"}"#),
    ]);
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(
                session.access_token, "access-1",
                "a working token must never be discarded over a profile hiccup"
            );
            assert_eq!(session.uid, None);
            assert_eq!(session.nickname, None);
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn a_profile_body_that_is_not_json_keeps_the_token_only_session() {
    let server = test_http::serve_sequence(&[
        (
            200,
            &json!({
                "code": 0,
                "data": {"accessToken": "access-1", "domain": "www.codebuddy.cn"}
            })
            .to_string(),
        ),
        (200, "<html>blocked</html>"),
    ]);
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            assert_eq!(session.access_token, "access-1");
            assert_eq!(session.uid, None);
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn the_profile_request_carries_the_bearer_token_and_the_domain() {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        time::Duration,
    };

    /// 读到请求头结束为止；`serve_sequence` 只回响应、看不到请求原文。
    fn read_request(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            }
        }
        String::from_utf8_lossy(&buffer).to_string()
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for index in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let request = read_request(&mut stream);
            let body = if index == 0 {
                json!({"code": 0, "data": {"accessToken": "access-1", "domain": "www.codebuddy.cn"}})
                    .to_string()
            } else {
                json!({"code": 0, "data": {"uid": "uid-1"}}).to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = sender.send((index, request));
        }
    });

    let login = DeviceCodeLogin::for_test(&base_url);
    let login_id = login.register_for_test("st-1");
    assert!(matches!(login.poll(&login_id), LoginPoll::Ready(_)));

    let mut requests = Vec::new();
    for _ in 0..2 {
        requests.push(
            receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("both the token and the profile request should arrive"),
        );
    }
    let profile = requests
        .iter()
        .find(|(index, _)| *index == 1)
        .map(|(_, request)| request)
        .expect("the profile request is the second one");
    let profile = profile.to_lowercase();

    assert!(
        profile.contains("/v2/plugin/login/account?state=st-1"),
        "the profile request must target the account endpoint with the server state: {profile}"
    );
    assert!(
        profile.contains("authorization: bearer access-1"),
        "the profile request must carry the freshly issued token: {profile}"
    );
    assert!(
        profile.contains("x-domain: www.codebuddy.cn"),
        "the profile request must carry the verified domain: {profile}"
    );
}

/// 客户端不跟随重定向：换 token 的请求带着服务端 state，取资料的请求还带着刚签发的
/// bearer token，一个 3xx 就足以把凭据转发到域名白名单从未批准的主机。
/// 重定向目标是一台「只要被访问就返回一份可用 token」的服务器：
/// 一旦客户端跟随重定向，这次轮询会变成 Ready，而不是停留在 Pending。
#[test]
fn a_redirect_response_is_not_followed() {
    let target = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {"accessToken": "leaked", "domain": "www.codebuddy.cn"}
        })
        .to_string(),
    );
    let server = test_http::serve_once(
        302,
        &[(
            "Location",
            &format!("{target}/v2/plugin/auth/token?state=st-1"),
        )],
        "",
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    assert!(
        matches!(login.poll(&login_id), LoginPoll::Pending),
        "a redirect must be a plain non-success response, never a followed request"
    );
}

#[test]
fn polling_a_response_without_a_token_stays_pending() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"accessToken": "   "}}).to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    assert!(matches!(login.poll(&login_id), LoginPoll::Pending));
}

#[test]
fn polling_a_body_that_is_not_json_stays_pending() {
    let server = test_http::serve_once(200, &[], "<html>blocked</html>");
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    assert!(matches!(login.poll(&login_id), LoginPoll::Pending));
}

#[test]
fn polling_survives_a_transport_failure() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");
    let login_id = login.register_for_test("st-1");

    assert!(
        matches!(login.poll(&login_id), LoginPoll::Pending),
        "a flaky network must not kill an attempt the TTL already bounds"
    );
}

#[test]
fn expires_in_seconds_fill_in_a_missing_expiry() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {
                "accessToken": "access-1",
                "domain": "codebuddy.cn",
                "expiresIn": 3600,
                "refreshExpiresIn": 7200
            }
        })
        .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    let before = chrono::Utc::now().timestamp_millis();
    let after = || chrono::Utc::now().timestamp_millis();
    match login.poll(&login_id) {
        LoginPoll::Ready(session) => {
            let expires_at = session
                .expires_at
                .expect("expiresIn must produce an expiry");
            assert!(
                (before + 3_600_000..=after() + 3_600_000).contains(&expires_at),
                "expiresIn must be read as seconds relative to now, got {expires_at}"
            );
            let refresh_expires_at = session
                .refresh_expires_at
                .expect("refreshExpiresIn must produce an expiry");
            assert!(
                (before + 7_200_000..=after() + 7_200_000).contains(&refresh_expires_at),
                "refreshExpiresIn must be read as seconds relative to now, got {refresh_expires_at}"
            );
        }
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[test]
fn a_domain_outside_the_workbuddy_family_is_rejected() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({
            "code": 0,
            "data": {"accessToken": "access-1", "domain": "evil.example"}
        })
        .to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    match login.poll(&login_id) {
        LoginPoll::Failed(message) => assert!(message.contains("evil.example")),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_token_without_a_domain_is_rejected() {
    let server = test_http::serve_once(
        200,
        &[],
        &json!({"code": 0, "data": {"accessToken": "access-1"}}).to_string(),
    );
    let login = DeviceCodeLogin::for_test(&server);
    let login_id = login.register_for_test("st-1");

    assert!(
        matches!(login.poll(&login_id), LoginPoll::Failed(_)),
        "an unverifiable domain must never be stored"
    );
}

#[test]
fn polling_an_unknown_login_id_fails_without_a_request() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");

    assert!(matches!(login.poll("missing"), LoginPoll::Failed(_)));
    assert!(!login.cancel("missing"));
}

#[test]
fn a_cancelled_login_stops_polling() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");
    let login_id = login.register_for_test("st-1");

    assert!(login.cancel(&login_id));
    assert!(
        matches!(login.poll(&login_id), LoginPoll::Failed(_)),
        "a cancelled attempt must not keep polling"
    );
    assert!(!login.cancel(&login_id));
}

#[test]
fn an_expired_login_stops_polling() {
    let login = DeviceCodeLogin::for_test("http://127.0.0.1:1");
    let login_id = login.register_expired_for_test("st-1");

    assert!(
        matches!(login.poll(&login_id), LoginPoll::Failed(_)),
        "an attempt past its TTL must not keep polling"
    );
    assert!(!login.cancel(&login_id));
}
