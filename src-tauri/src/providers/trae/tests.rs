use std::time::Duration;

use crate::{
    models::{ApiKeyStatus, ProviderErrorKind},
    providers::{
        api_key::{ApiKeyStore, EnvironmentReader, SecretBackend, SecretBytes},
        test_http, UsageProvider,
    },
};

use super::{client::TraeClient, definition, TraeProvider};

const TOKEN_BODY: &str = r#"{
  "Result": {"Token":"jwt-token","ExpiredAt":"2030-09-25T20:49:00+08:00"}
}"#;
const CREDITS_BODY: &str = r#"{
  "usage_summary": {"total_amount":3250,"consumed_amount":1452.22,"consumption_ratio":0.446},
  "user_entitlement_pack_list": [
    {"display_desc":"每月登录赠送","expire_time":1790812799,"entitlement_base_info":{"product_id":208,"available_endpoint":0,"quota":{"credits_limit":500}},"usage":{"credits_amount":52.22}},
    {"display_desc":"签到奖励","expire_time":1791343210,"entitlement_base_info":{"product_id":209,"available_endpoint":1,"quota":{"credits_limit":200}},"usage":{"credits_amount":0}}
  ]
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

fn auth(session: Option<&str>) -> ApiKeyStore {
    let secrets = std::sync::Arc::new(MemorySecrets(std::sync::Mutex::new(
        session.map(|value| value.as_bytes().to_vec()),
    )));
    ApiKeyStore::with_backends(
        "trae-cn",
        "TRAE_CN_SESSION",
        secrets,
        std::sync::Arc::new(EmptyEnvironment),
    )
}

fn provider(exchange_url: &str, entitlement_url: &str, session: Option<&str>) -> TraeProvider {
    TraeProvider::with_dependencies(
        auth(session),
        TraeClient::for_test(exchange_url, entitlement_url, Duration::from_secs(1)),
    )
}

#[test]
fn definition_exposes_credit_metric_and_authentication_capability() {
    let definition = definition();
    assert_eq!(definition.id, "trae-cn");
    assert_eq!(definition.display_name, "TraeWork CN");
    assert_eq!(
        definition
            .metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        [
            "trae-cn.credits",
            "trae-cn.workCredits",
            "trae-cn.generalCredits",
            "trae-cn.nearestExpiring",
            "trae-cn.creditPackages",
            "trae-cn.status"
        ]
    );
    let work = definition
        .metrics
        .iter()
        .find(|metric| metric.id == "trae-cn.workCredits")
        .unwrap();
    let general = definition
        .metrics
        .iter()
        .find(|metric| metric.id == "trae-cn.generalCredits")
        .unwrap();
    let nearest = definition
        .metrics
        .iter()
        .find(|metric| metric.id == "trae-cn.nearestExpiring")
        .unwrap();
    assert!(
        work.default_enabled && work.default_section == crate::models::MetricSection::AlwaysVisible
    );
    assert!(
        general.default_enabled
            && general.default_section == crate::models::MetricSection::OnDemand
    );
    assert!(
        nearest.pinnable && nearest.default_section == crate::models::MetricSection::AlwaysVisible
    );
}

#[test]
fn refresh_exchanges_session_and_maps_credits() {
    let token_url = test_http::serve_once(200, &[], TOKEN_BODY);
    let credits_url = test_http::serve_once(200, &[], CREDITS_BODY);
    let provider = provider(&token_url, &credits_url, Some("session-value"));

    let snapshot = provider.refresh().unwrap();
    assert_eq!(snapshot.provider_id, "trae-cn");
    assert_eq!(snapshot.plan.as_deref(), Some("Credits"));
    assert_eq!(snapshot.quotas[0].id, "credits");
    assert_eq!(snapshot.quotas[0].used_value, Some(1452.22));
    assert_eq!(snapshot.quotas[0].limit_value, Some(3250.0));
    assert_eq!(snapshot.credit_packages.len(), 2);
    assert_eq!(
        snapshot
            .value_metrics
            .iter()
            .map(|metric| metric.id.as_str())
            .collect::<Vec<_>>(),
        ["workCredits", "generalCredits", "nearestExpiring"]
    );
    assert_eq!(snapshot.value_metrics[0].values[0].number, 200.0);
    assert_eq!(snapshot.value_metrics[1].values[0].number, 1597.78);
    assert_eq!(snapshot.value_metrics[2].values[0].number, 447.78);
}

#[test]
fn missing_session_is_an_authentication_error() {
    let error = provider("http://127.0.0.1:1", "http://127.0.0.1:1", None)
        .refresh()
        .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
}

#[test]
fn expired_session_is_an_authentication_error() {
    let token_url = test_http::serve_once(401, &[], r#"{"ResponseMetadata":{}}"#);
    let error = provider(&token_url, "http://127.0.0.1:1", Some("expired"))
        .refresh()
        .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
}

#[test]
fn expired_entitlement_token_is_an_authentication_error() {
    let token_url = test_http::serve_once(200, &[], TOKEN_BODY);
    let credits_url = test_http::serve_once(401, &[], r#"{"code":1001}"#);
    let error = provider(&token_url, &credits_url, Some("session-value"))
        .refresh()
        .unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Authentication);
}

#[test]
fn session_status_is_reported() {
    let provider = provider("http://127.0.0.1:1", "http://127.0.0.1:1", Some("session"));
    assert_eq!(
        provider.session_status().unwrap().unwrap(),
        ApiKeyStatus::Saved
    );
    assert!(provider.webview_auth().is_some());
}
