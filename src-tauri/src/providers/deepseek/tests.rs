use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use chrono::Local;

use crate::{
    models::{ApiKeyStatus, MetricValueKind, ProviderErrorKind},
    providers::{
        api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes},
        normalize_default_pins, test_http, UsageProvider, WebviewAuth, WebviewCredentialSource,
    },
};

use super::{client::DeepSeekClient, definition, DeepSeekProvider};

const SUMMARY_BODY: &str = r#"{
  "code": 0,
  "msg": "",
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "normal_wallets": [
        {"balance": 100.0, "currency": "CNY"},
        {"balance": 2.0, "currency": "USD"}
      ],
      "bonus_wallets": [
        {"balance": 10.0, "currency": "CNY"}
      ],
      "total_costs": [
        {"currency": "CNY", "amount": 45.50},
        {"currency": "USD", "amount": 1.25}
      ]
    }
  }
}"#;

const COST_BODY: &str = r#"{
  "code": 0,
  "msg": "",
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "start": 1757001600,
      "end": 1757088000,
      "bucket": 86400,
      "models": ["deepseek-chat"],
      "data": [
        {
          "currency": "CNY",
          "series": [
            {"api_key": "first", "model": "deepseek-chat", "buckets": [{"time": 1757001600, "cost": "0.1"}]},
            {"api_key": "second", "model": "deepseek-chat", "buckets": [{"time": 1757001600, "cost": "0.2003"}]}
          ]
        },
        {
          "currency": "USD",
          "series": [
            {"api_key": "first", "model": "deepseek-chat", "buckets": [{"time": 1757001600, "cost": "0.0004"}]}
          ]
        }
      ]
    }
  }
}"#;

struct MemorySecrets(std::sync::Mutex<Option<Vec<u8>>>);

impl SecretBackend for MemorySecrets {
    fn read(&self, _account: &str) -> Result<Option<SecretBytes>, String> {
        Ok(self.0.lock().unwrap().clone().map(SecretBytes::new))
    }

    fn write(&self, _account: &str, value: &[u8]) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(value.to_vec());
        Ok(())
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

struct EmptyEnvironment;

impl EnvironmentReader for EmptyEnvironment {
    fn value(&self, _name: &str) -> Option<String> {
        None
    }
}

fn auth(token: Option<&str>) -> ApiKeyStore {
    let secrets = std::sync::Arc::new(MemorySecrets(std::sync::Mutex::new(
        token.map(|value| value.as_bytes().to_vec()),
    )));
    ApiKeyStore::with_backends(
        "deepseek",
        "DEEPSEEK_USER_TOKEN",
        secrets,
        std::sync::Arc::new(EmptyEnvironment),
    )
}

fn provider(base_url: &str, token: Option<&str>) -> DeepSeekProvider {
    DeepSeekProvider::with_dependencies(
        auth(token),
        DeepSeekClient::for_test(
            &format!("{base_url}/api/v0/users/get_user_summary"),
            &format!("{base_url}/api/v0/usage/by_api_key/cost"),
            Duration::from_secs(1),
        ),
    )
}

struct TestServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    handle: thread::JoinHandle<usize>,
    expected: usize,
}

impl TestServer {
    fn new(
        expected: usize,
        mut handler: impl FnMut(&str) -> (u16, String) + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut handled = 0;
            while handled < expected && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(_) => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                captured.lock().unwrap().push(request.clone());
                let (status, body) = handler(&request);
                write_response(&mut stream, status, &body);
                handled += 1;
            }
            handled
        });
        Self {
            base_url: format!("http://{address}"),
            requests,
            handle,
            expected,
        }
    }

    fn provider(&self, token: Option<&str>) -> DeepSeekProvider {
        provider(&self.base_url, token)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    fn finish(self) {
        assert_eq!(self.handle.join().unwrap(), self.expected);
    }
}

fn read_request(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0_u8; 1024];
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
}

fn write_response(stream: &mut impl Write, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        429 => "Too Many Requests",
        _ => "Test Response",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("request line should include a path")
}

fn query_param<'a>(path: &'a str, name: &str) -> Option<&'a str> {
    path.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn header<'a>(request: &'a str, expected_name: &str) -> Option<&'a str> {
    request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(expected_name)
            .then_some(value.trim())
    })
}

fn metric<'a>(
    snapshot: &'a crate::models::ProviderSnapshot,
    id: &str,
) -> &'a crate::models::ValueMetric {
    snapshot
        .value_metrics
        .iter()
        .find(|metric| metric.id == id)
        .unwrap_or_else(|| panic!("missing metric {id}"))
}

#[test]
fn definition_exposes_three_user_token_metrics_and_defaults_to_balance_and_today_spend() {
    let mut definition = definition();
    normalize_default_pins(&mut definition.metrics);

    assert_eq!(definition.id, "deepseek");
    assert_eq!(
        definition
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        [
            "deepseek.balance",
            "deepseek.todaySpend",
            "deepseek.totalSpend"
        ]
    );
    assert!(definition.metrics.iter().all(|metric| metric.pinnable));
    let pinned = definition
        .metrics
        .iter()
        .filter(|metric| metric.default_pinned)
        .map(|metric| metric.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(pinned, ["deepseek.balance", "deepseek.todaySpend"]);
}

#[test]
fn refresh_maps_balance_total_spend_and_today_spend_by_currency() {
    let server = TestServer::new(2, |request| {
        if request_path(request).contains("get_user_summary") {
            (200, SUMMARY_BODY.into())
        } else {
            (200, COST_BODY.into())
        }
    });
    let snapshot = server.provider(Some("user-token")).refresh().unwrap();

    assert_eq!(snapshot.provider_id, "deepseek");
    assert_eq!(snapshot.plan.as_deref(), Some("Available"));

    let balance = metric(&snapshot, "balance");
    assert_eq!(balance.label, "Balance");
    assert_eq!(balance.values.len(), 2);
    assert_eq!(balance.values[0].number, 110.0);
    assert_eq!(balance.values[0].kind, MetricValueKind::Currency);
    assert_eq!(balance.values[0].label.as_deref(), Some("CNY"));
    assert_eq!(balance.values[1].number, 2.0);
    assert_eq!(balance.values[1].label.as_deref(), Some("USD"));

    let total_spend = metric(&snapshot, "totalSpend");
    assert_eq!(total_spend.label, "Total Spend");
    assert_eq!(total_spend.values[0].number, 45.5);
    assert_eq!(total_spend.values[0].label.as_deref(), Some("CNY"));

    let today_spend = metric(&snapshot, "todaySpend");
    assert_eq!(today_spend.label, "Today Spend");
    assert_eq!(today_spend.values[0].number, 0.3003);
    assert_eq!(today_spend.values[0].kind, MetricValueKind::Currency);
    assert_eq!(today_spend.values[0].label.as_deref(), Some("CNY"));
    assert_eq!(today_spend.values[1].number, 0.0004);
    assert_eq!(today_spend.values[1].label.as_deref(), Some("USD"));

    server.finish();
}

#[test]
fn today_spend_reports_zero_for_currencies_without_a_bucket() {
    let empty_cost = r#"{
      "code": 0,
      "data": {
        "biz_code": 0,
        "biz_data": {"start": 0, "end": 86400, "bucket": 86400, "data": []}
      }
    }"#;
    let server = TestServer::new(2, |request| {
        if request_path(request).contains("get_user_summary") {
            (200, SUMMARY_BODY.into())
        } else {
            (200, empty_cost.into())
        }
    });

    let snapshot = server.provider(Some("user-token")).refresh().unwrap();
    let today_spend = metric(&snapshot, "todaySpend");
    assert_eq!(today_spend.values.len(), 2);
    assert!(today_spend.values.iter().all(|value| value.number == 0.0));
    assert_eq!(today_spend.values[0].label.as_deref(), Some("CNY"));
    assert_eq!(today_spend.values[1].label.as_deref(), Some("USD"));

    server.finish();
}

#[test]
fn today_cost_request_uses_local_midnight_timezone_and_browser_headers() {
    let server = TestServer::new(2, |request| {
        if request_path(request).contains("get_user_summary") {
            (200, SUMMARY_BODY.into())
        } else {
            (200, COST_BODY.into())
        }
    });

    server
        .provider(Some("user-token"))
        .refresh()
        .expect("refresh should succeed");

    let requests = server.requests();
    let cost_request = requests
        .iter()
        .find(|request| request_path(request).contains("by_api_key/cost"))
        .expect("today cost request should be sent");
    let path = request_path(cost_request);
    let start = query_param(path, "start").unwrap().parse::<i64>().unwrap();
    let end = query_param(path, "end").unwrap().parse::<i64>().unwrap();
    let timezone = query_param(path, "tz").unwrap().parse::<i32>().unwrap();
    let now = Local::now();
    let expected_timezone = now.offset().local_minus_utc();
    let expected_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp()
        - i64::from(expected_timezone);

    assert_eq!(start, expected_start);
    assert_eq!(end, start + 86_400);
    assert_eq!(timezone, expected_timezone);
    assert_eq!(
        header(cost_request, "authorization"),
        Some("Bearer user-token")
    );
    assert_eq!(header(cost_request, "accept"), Some("application/json"));
    assert!(
        header(cost_request, "user-agent").is_some_and(|value| value.starts_with("Mozilla/5.0"))
    );

    server.finish();
}

#[test]
fn missing_token_is_typed_as_an_authentication_failure() {
    let error = provider("http://127.0.0.1:1", None).refresh().unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
}

#[test]
fn platform_authentication_codes_are_typed_as_authentication_failures() {
    for code in [40002, 40003] {
        let url = test_http::serve_once(
            200,
            &[],
            &format!(r#"{{"code":{code},"msg":"Invalid token","data":null}}"#),
        );
        let error = provider(&url, Some("bad-token")).refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Authentication);
    }
}

#[test]
fn http_forbidden_and_unauthorized_responses_are_authentication_failures() {
    for status in [401, 403] {
        let url = test_http::serve_once(status, &[], r#"{"code":40003}"#);
        let error = provider(&url, Some("bad-token")).refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::Authentication);
    }
}

#[test]
fn http_rate_limit_is_typed_as_rate_limited() {
    let url = test_http::serve_once(429, &[], "{}");
    let error = provider(&url, Some("user-token")).refresh().unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::RateLimited);
}

#[test]
fn malformed_or_business_error_envelopes_are_invalid_responses() {
    for body in [
        r#"{"code":1,"msg":"failed","data":null}"#,
        r#"{"code":0,"data":{"biz_code":1,"biz_msg":"failed","biz_data":{}}}"#,
        r#"{"code":0,"data":{"biz_code":0,"biz_msg":"","biz_data":{}}}"#,
    ] {
        let url = test_http::serve_once(200, &[], body);
        let error = provider(&url, Some("user-token")).refresh().unwrap_err();
        assert_eq!(error.kind(), ProviderErrorKind::InvalidResponse);
    }
}

#[test]
fn user_token_uses_the_deepseek_webview_session() {
    let provider = provider("http://127.0.0.1:1", Some("user-token"));
    assert_eq!(
        provider.session_status().unwrap().unwrap(),
        ApiKeyStatus::Saved
    );
    assert_eq!(
        provider.webview_auth(),
        Some(WebviewAuth {
            login_url: "https://platform.deepseek.com/sign_in".into(),
            credential: WebviewCredentialSource::LocalStorage {
                key: "userToken".into(),
            },
            window_label: "deepseek-login".into(),
        })
    );
    assert!(!provider.supports_api_key_configuration());
}
