use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, header::HeaderValue, StatusCode};
use serde_json::Value;

use crate::providers::ProviderRequestContext;

use super::TraeError;

const CN_EXCHANGE_URL: &str = "https://api.trae.cn/cloudide/api/v3/common/GetUserToken";
const CN_CREDITS_URL: &str = "https://api.trae.cn/trae/api/v2/pay/user_current_entitlement_list";
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

#[derive(Debug)]
pub struct EndpointResponse {
    pub status: StatusCode,
    pub body: Value,
}

pub struct TraeClient {
    timeout: Duration,
    exchange_url: String,
    credits_url: String,
}

impl TraeClient {
    pub fn new() -> Result<Self, TraeError> {
        Self::with_endpoints(CN_EXCHANGE_URL, CN_CREDITS_URL, Duration::from_secs(15))
    }

    fn with_endpoints(
        exchange_url: &str,
        credits_url: &str,
        timeout: Duration,
    ) -> Result<Self, TraeError> {
        Ok(Self {
            timeout,
            exchange_url: exchange_url.to_owned(),
            credits_url: credits_url.to_owned(),
        })
    }

    fn http_client(&self, context: &ProviderRequestContext) -> Result<Arc<Client>, TraeError> {
        context
            .http_clients
            .client(
                "trae-cn",
                "default",
                context.proxy_url.as_ref(),
                |builder| {
                    builder
                        .connect_timeout(Duration::from_secs(8))
                        .timeout(self.timeout)
                        .user_agent(BROWSER_USER_AGENT)
                },
            )
            .map_err(|_| TraeError::ConnectionFailed)
    }

    pub fn exchange_token(
        &self,
        context: &ProviderRequestContext,
        session: &str,
    ) -> Result<EndpointResponse, TraeError> {
        let client = self.http_client(context)?;
        let cookie = HeaderValue::from_str(&format!("X-Cloudide-Session={session}"))
            .map_err(|_| TraeError::InvalidResponse)?;
        let response = client
            .post(&self.exchange_url)
            .header("Cookie", cookie)
            .header("Accept", "application/json")
            .json(&serde_json::json!({}))
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "trae token exchange failed (transport)");
                TraeError::ConnectionFailed
            })?;
        self.decode(response, "token exchange")
    }

    pub fn fetch_credits(
        &self,
        context: &ProviderRequestContext,
        token: &str,
    ) -> Result<EndpointResponse, TraeError> {
        let client = self.http_client(context)?;
        let authorization = HeaderValue::from_str(&format!("Cloud-IDE-JWT {token}"))
            .map_err(|_| TraeError::InvalidResponse)?;
        let response = client
            .post(&self.credits_url)
            .header("authorization", authorization)
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "require_usage": true,
                "full_data": true
            }))
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "trae credits request failed (transport)");
                TraeError::ConnectionFailed
            })?;
        self.decode(response, "credits")
    }

    fn decode(
        &self,
        response: reqwest::blocking::Response,
        endpoint: &str,
    ) -> Result<EndpointResponse, TraeError> {
        let started = std::time::Instant::now();
        let status = response.status();
        crate::app_debug!(
            "http",
            "trae {endpoint} HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let text = response.text().map_err(|_| TraeError::InvalidResponse)?;
        let body = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(EndpointResponse { status, body })
    }
}

#[cfg(test)]
impl TraeClient {
    pub fn for_test(exchange_url: &str, credits_url: &str, timeout: Duration) -> Self {
        Self::with_endpoints(exchange_url, credits_url, timeout).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    use reqwest::Url;

    use super::TraeClient;
    use crate::providers::{http::ProviderHttpClientFactory, test_http, ProviderRequestContext};

    fn capture_once() -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![0_u8; 8192];
            let count = stream.read(&mut request).unwrap();
            sender
                .send(String::from_utf8_lossy(&request[..count]).into_owned())
                .unwrap();
            let body = r#"{"Result":{"Token":"jwt-token"},"usage_summary":{"total_amount":1,"consumed_amount":0}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    #[test]
    fn token_exchange_sends_the_session_cookie_without_putting_it_in_the_body() {
        let (url, request) = capture_once();
        let client = TraeClient::for_test(&url, &url, Duration::from_secs(1));

        client
            .exchange_token(
                &ProviderRequestContext::direct(Arc::default()),
                "session-secret",
            )
            .unwrap();

        let request = request.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(request.starts_with("POST / HTTP/1.1"));
        assert!(request.contains("cookie: X-Cloudide-Session=session-secret"));
        assert!(!request.contains("\"session-secret\""));
        assert!(request.ends_with("{}"));
    }

    #[test]
    fn credits_request_uses_the_exact_two_key_body_and_cloud_ide_jwt_header() {
        let (url, request) = capture_once();
        let client = TraeClient::for_test(&url, &url, Duration::from_secs(1));

        client
            .fetch_credits(&ProviderRequestContext::direct(Arc::default()), "jwt-token")
            .unwrap();

        let request = request.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(request.contains("authorization: Cloud-IDE-JWT jwt-token"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"require_usage": true, "full_data": true})
        );
    }

    #[test]
    fn provider_proxy_remaining_route_trae() {
        let (url, request) =
            test_http::serve_once_capturing_request(200, r#"{"Result":{"Token":"jwt-token"}}"#);
        let proxy_url = url.replacen("http://", "http://proxy-user:proxy-pass@", 1);
        let context = ProviderRequestContext {
            proxy_url: Some(Url::parse(&proxy_url).unwrap()),
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        };
        let client = TraeClient::for_test(&url, &url, Duration::from_secs(1));

        client.exchange_token(&context, "session-secret").unwrap();

        let request = request.join().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("post http://"));
        assert!(request.contains("proxy-authorization: basic "));
        assert!(request.contains("cookie: x-cloudide-session=session-secret"));
        assert!(request.contains("accept: application/json"));
        assert!(request.contains("content-type: application/json"));
        assert!(request.contains("user-agent: mozilla/5.0 (macintosh; intel mac os x 10_15_7)"));
    }
}
