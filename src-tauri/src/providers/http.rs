use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use reqwest::blocking::Client;

#[derive(Default)]
pub struct ProviderHttpClientFactory {
    clients: Mutex<HashMap<ClientKey, Arc<Client>>>,
}

#[derive(Hash, PartialEq, Eq)]
struct ClientKey {
    provider_id: String,
    profile: String,
    proxy_url: Option<reqwest::Url>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderHttpClientError {
    #[error("proxy configuration failed for provider {provider_id}")]
    ProxyConfiguration { provider_id: String },
    #[error("HTTP client build failed for provider {provider_id}")]
    ClientBuild { provider_id: String },
}

impl ProviderHttpClientFactory {
    pub fn client(
        &self,
        provider_id: &str,
        profile: &str,
        proxy_url: Option<&reqwest::Url>,
        configure: impl FnOnce(reqwest::blocking::ClientBuilder) -> reqwest::blocking::ClientBuilder,
    ) -> Result<Arc<Client>, ProviderHttpClientError> {
        let key = ClientKey {
            provider_id: provider_id.to_owned(),
            profile: profile.to_owned(),
            proxy_url: proxy_url.cloned(),
        };
        let mut clients = self
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(client) = clients.get(&key) {
            return Ok(Arc::clone(client));
        }

        let mut builder = configure(Client::builder());
        if let Some(url) = proxy_url {
            builder = builder.proxy(reqwest::Proxy::all(url.as_str()).map_err(|_| {
                ProviderHttpClientError::ProxyConfiguration {
                    provider_id: provider_id.to_owned(),
                }
            })?);
        }
        let client =
            Arc::new(
                builder
                    .build()
                    .map_err(|_| ProviderHttpClientError::ClientBuild {
                        provider_id: provider_id.to_owned(),
                    })?,
            );

        clients.retain(|existing, _| {
            existing.provider_id != provider_id || existing.profile != profile
        });
        clients.insert(key, Arc::clone(&client));
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use reqwest::Url;

    use super::{ProviderHttpClientError, ProviderHttpClientFactory};

    #[test]
    fn provider_proxy_client_reuses_unchanged_policy() {
        let factory = ProviderHttpClientFactory::default();
        let configure_calls = Arc::new(AtomicUsize::new(0));

        let first_counter = Arc::clone(&configure_calls);
        let first = factory
            .client("openai", "quota", None, move |builder| {
                first_counter.fetch_add(1, Ordering::SeqCst);
                builder
            })
            .unwrap();
        let second_counter = Arc::clone(&configure_calls);
        let second = factory
            .client("openai", "quota", None, move |builder| {
                second_counter.fetch_add(1, Ordering::SeqCst);
                builder
            })
            .unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(configure_calls.load(Ordering::SeqCst), 1);

        let other_profile = factory
            .client("openai", "login", None, |builder| builder)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &other_profile));
    }

    #[test]
    fn provider_proxy_client_rebuilds_for_changed_url() {
        let factory = ProviderHttpClientFactory::default();
        let proxy_one = Url::parse("http://127.0.0.1:8101").unwrap();
        let proxy_two = Url::parse("http://127.0.0.1:8102").unwrap();

        let first = factory
            .client("openai", "quota", Some(&proxy_one), |builder| builder)
            .unwrap();
        let second = factory
            .client("openai", "quota", Some(&proxy_two), |builder| builder)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &second));

        let direct = factory
            .client("openai", "quota", None, |builder| builder)
            .unwrap();
        assert!(!Arc::ptr_eq(&second, &direct));

        let rebuilt_first = factory
            .client("openai", "quota", Some(&proxy_one), |builder| builder)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &rebuilt_first));
        assert!(!Arc::ptr_eq(&direct, &rebuilt_first));
    }

    #[test]
    fn provider_proxy_client_error_redacts_credentials() {
        let factory = ProviderHttpClientFactory::default();
        let proxy = Url::parse("http://proxy-user:proxy-password@proxy.invalid:8080").unwrap();

        let error = factory
            .client("credential-provider", "quota", Some(&proxy), |builder| {
                builder
                    .min_tls_version(reqwest::tls::Version::TLS_1_3)
                    .max_tls_version(reqwest::tls::Version::TLS_1_2)
            })
            .unwrap_err();
        let display = error.to_string();

        assert!(matches!(error, ProviderHttpClientError::ClientBuild { .. }));
        assert!(display.contains("credential-provider"));
        assert!(!display.contains("proxy-user"));
        assert!(!display.contains("proxy-password"));
        assert!(!display.contains("proxy.invalid"));
    }
}
