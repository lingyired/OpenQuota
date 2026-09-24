use std::{collections::HashMap, sync::Arc, time::Duration};

use super::{keychain_access, ProviderRegistry};

const CREDENTIAL_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialProbeStatus {
    Detected,
    Absent,
    Unknown,
}

pub type CredentialProbeResults = HashMap<String, CredentialProbeStatus>;

/// Probes local provider credentials without blocking Tauri's setup thread.
///
/// Each provider owns its credential source and availability check. The probes run on separate
/// blocking workers because they may consult the operating-system credential store or verify that
/// credentials grant access to the provider.
pub async fn detect_local_credentials(
    registry: Arc<ProviderRegistry>,
    provider_ids: &[String],
) -> CredentialProbeResults {
    detect_local_credentials_with_timeout(registry, provider_ids, CREDENTIAL_PROBE_TIMEOUT).await
}

/// Classifies a finished probe.
///
/// A provider that reads another application's Keychain entry reports `Unknown` while access is
/// ungranted: only its file-backed sources were visible, so "absent" would be a guess. `Unknown`
/// leaves the stored state untouched, which keeps the automatic fallback from enabling a card
/// that has nothing to show yet.
fn classify_probe(
    detected: bool,
    accesses_system_keychain: bool,
    granted: bool,
) -> CredentialProbeStatus {
    if detected {
        CredentialProbeStatus::Detected
    } else if accesses_system_keychain && !granted {
        CredentialProbeStatus::Unknown
    } else {
        CredentialProbeStatus::Absent
    }
}

async fn detect_local_credentials_with_timeout(
    registry: Arc<ProviderRegistry>,
    provider_ids: &[String],
    timeout: Duration,
) -> CredentialProbeResults {
    let mut probes = Vec::with_capacity(provider_ids.len());
    for provider_id in provider_ids {
        let Some(runtime) = registry.runtime(provider_id) else {
            continue;
        };
        let provider_id = provider_id.clone();
        let probe_provider_id = provider_id.clone();
        let probe = tauri::async_runtime::spawn(async move {
            let worker_provider_id = probe_provider_id.clone();
            let worker = tauri::async_runtime::spawn_blocking(move || {
                classify_probe(
                    runtime.has_local_credentials() || runtime.has_local_installation(),
                    runtime.accesses_system_keychain(),
                    keychain_access::is_granted(&worker_provider_id),
                )
            });
            match tokio::time::timeout(timeout, worker).await {
                Ok(Ok(status)) => status,
                Ok(Err(_)) => {
                    crate::app_warn!(
                        "providers",
                        "credential probe worker for {probe_provider_id} stopped unexpectedly"
                    );
                    CredentialProbeStatus::Unknown
                }
                Err(_) => {
                    crate::app_warn!(
                        "providers",
                        "credential probe for {probe_provider_id} reached its time limit"
                    );
                    CredentialProbeStatus::Unknown
                }
            }
        });
        probes.push((provider_id, probe));
    }

    let mut results = HashMap::with_capacity(probes.len());
    for (provider_id, probe) in probes {
        let status = probe.await.unwrap_or_else(|_| {
            crate::app_warn!(
                "providers",
                "credential probe task for {provider_id} stopped unexpectedly"
            );
            CredentialProbeStatus::Unknown
        });
        results.insert(provider_id, status);
    }
    results
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use crate::{
        models::{
            MetricDefinition, MetricSection, MetricSource, ProviderDefinition, ProviderSnapshot,
        },
        providers::{ProviderError, UsageProvider},
    };

    use super::{
        classify_probe, detect_local_credentials, detect_local_credentials_with_timeout,
        CredentialProbeStatus, ProviderRegistry,
    };

    struct ProbeProvider {
        definition: ProviderDefinition,
        detected: bool,
        barrier: Arc<Barrier>,
    }

    impl UsageProvider for ProbeProvider {
        fn definition(&self) -> ProviderDefinition {
            self.definition.clone()
        }

        fn has_local_credentials(&self) -> bool {
            self.barrier.wait();
            self.detected
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }
    }

    fn provider(id: &str, detected: bool, barrier: Arc<Barrier>) -> Arc<dyn UsageProvider> {
        Arc::new(ProbeProvider {
            definition: ProviderDefinition {
                id: id.into(),
                display_name: id.into(),
                short_name: id.into(),
                fallback_enabled: true,
                local_usage_source_note: None,
                links: vec![],
                metrics: vec![MetricDefinition::new(
                    format!("{id}.session"),
                    "Session",
                    MetricSource::Quota {
                        source_id: "session".into(),
                        session_window: true,
                    },
                    true,
                    true,
                    MetricSection::AlwaysVisible,
                    true,
                    Some("S"),
                    None,
                )],
            },
            detected,
            barrier,
        })
    }

    #[test]
    fn ungranted_keychain_providers_report_unknown_instead_of_absent() {
        assert_eq!(
            classify_probe(true, true, false),
            CredentialProbeStatus::Detected
        );
        assert_eq!(
            classify_probe(false, true, false),
            CredentialProbeStatus::Unknown
        );
        assert_eq!(
            classify_probe(false, true, true),
            CredentialProbeStatus::Absent
        );
        assert_eq!(
            classify_probe(false, false, false),
            CredentialProbeStatus::Absent
        );
    }

    #[test]
    fn credential_probes_run_concurrently_and_return_only_hits() {
        let barrier = Arc::new(Barrier::new(2));
        let registry = Arc::new(
            ProviderRegistry::new(vec![
                provider("first", true, barrier.clone()),
                provider("second", false, barrier),
            ])
            .unwrap(),
        );
        let ids = vec!["first".to_owned(), "second".to_owned()];

        let detected = tauri::async_runtime::block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                detect_local_credentials(registry, &ids),
            )
            .await
            .expect("credential probes should overlap")
        });

        assert_eq!(
            detected.get("first"),
            Some(&CredentialProbeStatus::Detected)
        );
        assert_eq!(detected.get("second"), Some(&CredentialProbeStatus::Absent));
    }

    struct InstalledProvider {
        definition: ProviderDefinition,
    }

    impl UsageProvider for InstalledProvider {
        fn definition(&self) -> ProviderDefinition {
            self.definition.clone()
        }

        fn has_local_credentials(&self) -> bool {
            false
        }

        fn has_local_installation(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }
    }

    #[test]
    fn installed_apps_count_as_local_detection() {
        let definition = provider("trae-cn", false, Arc::new(Barrier::new(1))).definition();
        let registry = Arc::new(
            ProviderRegistry::new(vec![Arc::new(InstalledProvider { definition })]).unwrap(),
        );
        let ids = vec!["trae-cn".to_owned()];

        let detected = tauri::async_runtime::block_on(detect_local_credentials(registry, &ids));

        assert_eq!(
            detected.get("trae-cn"),
            Some(&CredentialProbeStatus::Detected)
        );
    }

    struct SlowProvider {
        definition: ProviderDefinition,
    }

    impl UsageProvider for SlowProvider {
        fn definition(&self) -> ProviderDefinition {
            self.definition.clone()
        }

        fn has_local_credentials(&self) -> bool {
            std::thread::sleep(std::time::Duration::from_millis(100));
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }
    }

    #[test]
    fn stalled_credential_probe_does_not_hold_up_detection() {
        let definition = provider("slow", false, Arc::new(Barrier::new(1))).definition();
        let registry =
            Arc::new(ProviderRegistry::new(vec![Arc::new(SlowProvider { definition })]).unwrap());
        let ids = vec!["slow".to_owned()];

        let detected = tauri::async_runtime::block_on(detect_local_credentials_with_timeout(
            registry,
            &ids,
            std::time::Duration::from_millis(10),
        ));

        assert_eq!(detected.get("slow"), Some(&CredentialProbeStatus::Unknown));
    }

    struct PanickingProvider {
        definition: ProviderDefinition,
    }

    impl UsageProvider for PanickingProvider {
        fn definition(&self) -> ProviderDefinition {
            self.definition.clone()
        }

        fn has_local_credentials(&self) -> bool {
            panic!("probe failed")
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            unreachable!()
        }
    }

    #[test]
    fn failed_credential_probe_is_unknown_instead_of_absent() {
        let definition = provider("failed", false, Arc::new(Barrier::new(1))).definition();
        let registry = Arc::new(
            ProviderRegistry::new(vec![Arc::new(PanickingProvider { definition })]).unwrap(),
        );
        let ids = vec!["failed".to_owned()];

        let detected = tauri::async_runtime::block_on(detect_local_credentials_with_timeout(
            registry,
            &ids,
            std::time::Duration::from_secs(1),
        ));

        assert_eq!(
            detected.get("failed"),
            Some(&CredentialProbeStatus::Unknown)
        );
    }
}
