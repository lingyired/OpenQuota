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
