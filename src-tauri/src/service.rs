use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use chrono::Utc;

use crate::{
    models::{
        MetricSource, ProviderErrorKind, ProviderSnapshot, ProviderViewState, SnapshotSource,
    },
    policy::{
        refresh_interval_for_provider, stale_after, FAILURE_RETRY_BACKOFF, FAST_REFRESH_INTERVAL,
        SELECTED_PROVIDER_REFRESH_AFTER,
    },
    providers::{
        http::ProviderHttpClientFactory, ProviderError, ProviderRefresh, ProviderRegistry,
        ProviderRequestContext,
    },
    settings::SettingsService,
    storage::Storage,
};

// Cursor can make several bounded requests in sequence; allow its full healthy network budget
// before quarantining the synchronous provider worker.
const PROVIDER_REFRESH_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn resolve_proxy_url(
    value: Option<String>,
    use_proxy: bool,
) -> Result<Option<reqwest::Url>, String> {
    let value = crate::settings::normalize_proxy_url(value)?;
    let Some(value) = value.filter(|_| use_proxy) else {
        return Ok(None);
    };
    reqwest::Url::parse(&value)
        .map(Some)
        .map_err(|_| "Proxy URL could not be parsed after normalization.".to_owned())
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageViewState {
    pub providers: BTreeMap<String, ProviderViewState>,
    pub last_full_refresh_at: Option<chrono::DateTime<Utc>>,
    /// 最近一次**真的拿到数据**的刷新落地时间，批次和单点都算。
    ///
    /// 与 `last_full_refresh_at` 不同：那个是「最近一次尝试」，成功失败都前进；
    /// 这个是「最近一次成功」，一次都没成功时保持不动。界面顶部用它回答「我看到的
    /// 数字是不是最新的」，所以它绝不能在一次失败之后前进——那会把一个陈旧的数字
    /// 伪装成刚更新过。
    pub last_successful_refresh_at: Option<chrono::DateTime<Utc>>,
    /// 当前是否有任何一个启用中的 provider 处于失败状态。
    ///
    /// 每次刷新（批次或单点）落地后按 provider 状态重算，而不是只在批次收尾时定一次。
    /// 登录、保存密钥、菜单栏点击这些单点刷新不经过批次收尾，若只有批次能写这一位，
    /// 用户刚登录成功、数据已经到手，顶部却还挂着上一批留下的失败标记。
    pub last_refresh_failed: bool,
    pub next_refresh_at: Option<chrono::DateTime<Utc>>,
}

pub struct ProviderService {
    registry: Arc<ProviderRegistry>,
    storage: Arc<Storage>,
    states: RwLock<BTreeMap<String, ProviderViewState>>,
    refresh_flights: HashMap<String, Arc<RefreshFlight>>,
    native_instance_ids: RwLock<std::collections::HashSet<String>>,
    last_live_refresh: Mutex<HashMap<String, Instant>>,
    last_failed_refresh: Mutex<HashMap<String, Instant>>,
    last_full_refresh_at: RwLock<Option<chrono::DateTime<Utc>>>,
    last_successful_refresh_at: RwLock<Option<chrono::DateTime<Utc>>>,
    last_refresh_failed: RwLock<bool>,
    refresh_timeout: Duration,
    settings: Option<Arc<SettingsService>>,
    http_clients: Arc<ProviderHttpClientFactory>,
}

impl ProviderService {
    #[cfg(test)]
    pub fn new(registry: Arc<ProviderRegistry>, storage: Arc<Storage>) -> Self {
        Self::with_refresh_timeout_and_settings(registry, storage, PROVIDER_REFRESH_TIMEOUT, None)
    }

    pub fn new_with_settings(
        registry: Arc<ProviderRegistry>,
        storage: Arc<Storage>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self::with_refresh_timeout_and_settings(
            registry,
            storage,
            PROVIDER_REFRESH_TIMEOUT,
            Some(settings),
        )
    }

    #[cfg(test)]
    fn with_refresh_timeout(
        registry: Arc<ProviderRegistry>,
        storage: Arc<Storage>,
        refresh_timeout: Duration,
    ) -> Self {
        Self::with_refresh_timeout_and_settings(registry, storage, refresh_timeout, None)
    }

    fn with_refresh_timeout_and_settings(
        registry: Arc<ProviderRegistry>,
        storage: Arc<Storage>,
        refresh_timeout: Duration,
        settings: Option<Arc<SettingsService>>,
    ) -> Self {
        let mut states = BTreeMap::new();
        let mut refresh_flights = HashMap::new();
        for definition in &registry.catalog().providers {
            let id = definition.id.clone();
            let state = match storage.load_snapshot_for_identity(&id, registry.cache_identity(&id))
            {
                Ok(Some(snapshot)) => {
                    crate::app_debug!("cache", "loaded cached snapshot for {id}");
                    ProviderViewState::from_cache(snapshot)
                }
                Ok(None) => ProviderViewState::default(),
                Err(error) => {
                    crate::app_warn!(
                        "cache",
                        "cached snapshot for {id} could not be loaded: {error}"
                    );
                    ProviderViewState::default()
                }
            };
            states.insert(id.clone(), state);
            refresh_flights.insert(id, Arc::new(RefreshFlight::new()));
        }
        Self {
            registry,
            storage,
            states: RwLock::new(states),
            refresh_flights,
            native_instance_ids: RwLock::new(std::collections::HashSet::new()),
            last_live_refresh: Mutex::new(HashMap::new()),
            last_failed_refresh: Mutex::new(HashMap::new()),
            last_full_refresh_at: RwLock::new(None),
            last_successful_refresh_at: RwLock::new(None),
            last_refresh_failed: RwLock::new(false),
            refresh_timeout,
            settings,
            http_clients: Arc::new(ProviderHttpClientFactory::default()),
        }
    }

    pub fn state(&self) -> UsageViewState {
        let mut providers = self
            .states
            .read()
            .map(|value| value.clone())
            .unwrap_or_default();
        // 顶部那一行按「当前正在看的 provider」显示，所以每个 provider 都要带上自己
        // 的失败状态；判定与整批用的那条完全一致。
        //
        // 锁必须限定在这个块里：下面 `next_refresh_at()` 也要读这把锁，而它是不可重入
        // 的 `Mutex`，持着再进就是自锁死。
        {
            let failures = self.last_failed_refresh.lock().ok();
            for (provider_id, state) in providers.iter_mut() {
                let interval = self
                    .refresh_interval_for_provider(provider_id)
                    .unwrap_or(crate::policy::SLOW_REFRESH_INTERVAL);
                update_staleness_from_snapshot_age(state, interval);
                state.last_refresh_failed =
                    provider_refresh_failed(state, failures.as_deref(), provider_id);
            }
        }
        let last_full_refresh_at = self
            .last_full_refresh_at
            .read()
            .ok()
            .and_then(|value| value.to_owned());
        let last_successful_refresh_at = self
            .last_successful_refresh_at
            .read()
            .ok()
            .and_then(|value| value.to_owned());
        let last_refresh_failed = self
            .last_refresh_failed
            .read()
            .map(|value| *value)
            .unwrap_or(false);
        UsageViewState {
            providers,
            last_full_refresh_at,
            last_successful_refresh_at,
            last_refresh_failed,
            next_refresh_at: self.next_refresh_at(),
        }
    }

    pub fn request_context_for(&self, provider_id: &str) -> ProviderRequestContext {
        let Some(settings_service) = &self.settings else {
            return ProviderRequestContext::direct(Arc::clone(&self.http_clients));
        };
        let settings = settings_service.get();
        let use_proxy = settings
            .providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .is_some_and(|provider| provider.use_proxy);
        match resolve_proxy_url(settings.proxy_url, use_proxy) {
            Ok(proxy_url) => ProviderRequestContext {
                proxy_url,
                http_clients: Arc::clone(&self.http_clients),
            },
            Err(error) => {
                crate::app_warn!(
                    "config",
                    "proxy policy for {provider_id} could not be resolved: {error}"
                );
                ProviderRequestContext::direct(Arc::clone(&self.http_clients))
            }
        }
    }

    pub async fn refresh(self: &Arc<Self>, provider_id: &str, force: bool) -> ProviderViewState {
        self.refresh_with_interval(provider_id, force, None).await
    }

    async fn refresh_with_interval(
        self: &Arc<Self>,
        provider_id: &str,
        force: bool,
        selected_interval: Option<Duration>,
    ) -> ProviderViewState {
        if self.registry.runtime(provider_id).is_none() {
            crate::app_error!(
                "refresh",
                "refresh requested for unknown provider {provider_id}"
            );
            return ProviderViewState {
                error: Some("Unknown provider.".into()),
                error_kind: Some(ProviderErrorKind::Internal),
                last_refresh_failed: true,
                ..ProviderViewState::default()
            };
        };
        if !force && self.is_fresh_this_session(provider_id, selected_interval) {
            crate::app_debug!("refresh", "cache hit {provider_id}");
            return self.provider_state(provider_id);
        }
        if !force && self.is_in_failure_backoff(provider_id) {
            crate::app_debug!(
                "refresh",
                "backoff skip {provider_id} (failed <{}s ago)",
                FAILURE_RETRY_BACKOFF.as_secs()
            );
            return self.provider_state(provider_id);
        }
        let Some(flight) = self.refresh_flights.get(provider_id).cloned() else {
            return self.provider_state(provider_id);
        };
        let mut completed = flight.completed_tx.subscribe();
        let (target_generation, start_runner) = {
            let Ok(mut flight_state) = flight.state.lock() else {
                crate::app_error!(
                    "refresh",
                    "refresh coordination unavailable for {provider_id}"
                );
                return ProviderViewState {
                    error: Some("Provider refresh is temporarily unavailable.".into()),
                    error_kind: Some(ProviderErrorKind::Internal),
                    last_refresh_failed: true,
                    ..self.provider_state(provider_id)
                };
            };
            if !flight_state.runner_active {
                let generation = flight_state.completed_generation.saturating_add(1);
                flight_state.runner_active = true;
                flight_state.attempt_generation = Some(generation);
                flight_state.requested_generation = generation;
                (generation, true)
            } else if flight_state.attempt_generation.is_none() {
                crate::app_debug!(
                    "refresh",
                    "timed-out refresh worker is still draining for {provider_id}"
                );
                drop(flight_state);
                return self.provider_state(provider_id);
            } else if force {
                let generation = flight_state
                    .attempt_generation
                    .expect("active refresh generation must exist")
                    .saturating_add(1);
                flight_state.requested_generation =
                    flight_state.requested_generation.max(generation);
                crate::app_debug!("refresh", "queued forced follow-up for {provider_id}");
                (flight_state.requested_generation, false)
            } else {
                (
                    flight_state
                        .attempt_generation
                        .expect("active refresh generation must exist"),
                    false,
                )
            }
        };

        if start_runner {
            let service = self.clone();
            let provider_id = provider_id.to_owned();
            let runner_flight = flight.clone();
            tauri::async_runtime::spawn(async move {
                service
                    .run_refresh_flight(provider_id, runner_flight, force)
                    .await;
            });
        }

        while *completed.borrow_and_update() < target_generation {
            if completed.changed().await.is_err() {
                break;
            }
        }
        self.provider_state(provider_id)
    }

    pub async fn refresh_selected(self: &Arc<Self>, provider_id: &str) -> ProviderViewState {
        if self.provider_state(provider_id).refreshing
            || self.is_in_failure_backoff(provider_id)
            || !crate::policy::selected_provider_is_due(self.last_attempt_age(provider_id))
        {
            return self.provider_state(provider_id);
        }
        self.refresh_with_interval(provider_id, false, Some(SELECTED_PROVIDER_REFRESH_AFTER))
            .await
    }

    async fn run_refresh_flight(
        self: Arc<Self>,
        provider_id: String,
        flight: Arc<RefreshFlight>,
        initial_force: bool,
    ) {
        let Some(provider) = self.registry.runtime(&provider_id) else {
            return;
        };
        let mut force = initial_force;
        loop {
            let generation = flight
                .state
                .lock()
                .ok()
                .and_then(|state| state.attempt_generation)
                .unwrap_or_default();
            let started = Instant::now();
            let tag = format!("plugin:{provider_id}");
            crate::app_info!(&tag, "refresh start (force={force})");
            self.update_state(&provider_id, |state| {
                state.refreshing = true;
                state.error = None;
                state.error_kind = None;
                state.last_attempt_at = Some(Utc::now());
            });
            let worker_provider = provider.clone();
            let context = self.request_context_for(&provider_id);
            let mut worker = tauri::async_runtime::spawn_blocking(move || {
                worker_provider.refresh_for_service_with_context(&context)
            });
            let mut late_worker = None;
            let refresh_result = match tokio::time::timeout(self.refresh_timeout, &mut worker).await
            {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => {
                    crate::app_error!(&tag, "refresh worker stopped unexpectedly");
                    Err(ProviderError::new(
                        ProviderErrorKind::Internal,
                        "Provider refresh stopped unexpectedly.",
                    ))
                }
                Err(_) => {
                    crate::app_warn!(
                        &tag,
                        "refresh timed out after {}ms; late result will be discarded",
                        self.refresh_timeout.as_millis()
                    );
                    late_worker = Some(worker);
                    Err(ProviderError::new(
                        ProviderErrorKind::Network,
                        "Provider refresh timed out.",
                    ))
                }
            };
            let refresh_result = refresh_result.and_then(|refresh| {
                validate_snapshot(&self.registry, &provider_id, refresh.snapshot.clone())?;
                Ok(refresh)
            });
            match &refresh_result {
                Ok(_) => {
                    crate::app_info!(&tag, "refresh end ({}ms)", started.elapsed().as_millis())
                }
                Err(error) => crate::app_warn!(
                    &tag,
                    "refresh failed ({}ms, kind={:?}): {error}",
                    started.elapsed().as_millis(),
                    error.kind()
                ),
            }
            let state = self.apply_refresh_result(&provider_id, refresh_result);
            if state.error.is_none() {
                if let Ok(mut last) = self.last_live_refresh.lock() {
                    last.insert(provider_id.clone(), Instant::now());
                }
                if let Ok(mut failures) = self.last_failed_refresh.lock() {
                    failures.remove(&provider_id);
                }
            } else if let Ok(mut failures) = self.last_failed_refresh.lock() {
                failures.insert(provider_id.clone(), Instant::now());
            }
            self.note_refresh_outcome(&state);

            let run_follow_up = if let Some(worker) = late_worker {
                let completed_generation = if let Ok(mut flight_state) = flight.state.lock() {
                    let completed = flight_state.requested_generation.max(generation);
                    flight_state.completed_generation = completed;
                    flight_state.attempt_generation = None;
                    completed
                } else {
                    generation.saturating_add(1)
                };
                flight.completed_tx.send_replace(completed_generation);

                match worker.await {
                    Ok(_) => crate::app_debug!(
                        &tag,
                        "timed-out refresh worker finished; discarded late result"
                    ),
                    Err(_) => crate::app_debug!(
                        &tag,
                        "timed-out refresh worker stopped; discarded late failure"
                    ),
                }

                if let Ok(mut flight_state) = flight.state.lock() {
                    flight_state.runner_active = false;
                    flight_state.requested_generation = flight_state.completed_generation;
                }
                false
            } else {
                let run_follow_up = if let Ok(mut flight_state) = flight.state.lock() {
                    settle_completed_generation(&mut flight_state, generation)
                } else {
                    false
                };
                flight.completed_tx.send_replace(generation);
                run_follow_up
            };
            if !run_follow_up {
                return;
            }
            force = true;
        }
    }

    #[cfg(test)]
    async fn refresh_enabled(
        self: &Arc<Self>,
        provider_ids: &[String],
        force: bool,
    ) -> UsageViewState {
        self.refresh_enabled_with_progress(provider_ids, force, |_| {})
            .await
    }

    pub async fn refresh_enabled_with_progress<F>(
        self: &Arc<Self>,
        provider_ids: &[String],
        force: bool,
        mut on_progress: F,
    ) -> UsageViewState
    where
        F: FnMut(&UsageViewState) + Send,
    {
        let started = Instant::now();
        crate::app_info!(
            "refresh",
            "batch start ({} providers, force={force})",
            provider_ids.len()
        );
        let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
        for provider_id in provider_ids {
            let service = self.clone();
            let provider_id = provider_id.clone();
            let completed_tx = completed_tx.clone();
            tauri::async_runtime::spawn(async move {
                let state = service.refresh(&provider_id, force).await;
                let _ = completed_tx.send(state);
            });
        }
        drop(completed_tx);

        let mut succeeded = 0;
        let mut failed = 0;
        let mut completed = 0;
        while let Some(state) = completed_rx.recv().await {
            completed += 1;
            if state.error.is_none() {
                succeeded += 1;
            } else {
                failed += 1;
            }
            let current = self.state();
            on_progress(&current);
        }
        failed += provider_ids.len().saturating_sub(completed);
        // 只有真的拿到数据才推进「最近更新成功」时间：整批全失败时保持旧值，顶部
        // 于是会显示上一次真正成功的时刻，而不是假装刚刚更新过。一个 provider 都没
        // 被要求刷新时（空批次）同样不算成功，否则空转也会刷新这个时间。
        if succeeded > 0 {
            self.note_refresh_succeeded();
        }
        // 空批次既没成功也没失败，保留上一次的结论；有结果时按各 provider 的现况重
        // 算，与单点刷新同一套判定。
        if completed > 0 {
            self.note_refresh_failure_flag();
        }
        crate::app_info!(
            "refresh",
            "batch end ({}ms, {succeeded} ok / {failed} failed)",
            started.elapsed().as_millis()
        );
        self.state()
    }

    #[cfg(test)]
    async fn refresh_all(self: &Arc<Self>, provider_ids: &[String], force: bool) -> UsageViewState {
        self.refresh_all_with_progress(provider_ids, force, |_| {})
            .await
    }

    pub async fn refresh_all_with_progress<F>(
        self: &Arc<Self>,
        provider_ids: &[String],
        force: bool,
        on_progress: F,
    ) -> UsageViewState
    where
        F: FnMut(&UsageViewState) + Send,
    {
        self.refresh_enabled_with_progress(provider_ids, force, on_progress)
            .await;
        if let Ok(mut completed_at) = self.last_full_refresh_at.write() {
            *completed_at = Some(Utc::now());
        }
        self.state()
    }

    fn provider_state(&self, provider_id: &str) -> ProviderViewState {
        let mut state = self
            .states
            .read()
            .ok()
            .and_then(|states| states.get(provider_id).cloned())
            .unwrap_or_default();
        let interval = self
            .refresh_interval_for_provider(provider_id)
            .unwrap_or(crate::policy::SLOW_REFRESH_INTERVAL);
        update_staleness_from_snapshot_age(&mut state, interval);
        state.last_refresh_failed = {
            let failures = self.last_failed_refresh.lock().ok();
            provider_refresh_failed(&state, failures.as_deref(), provider_id)
        };
        state
    }

    fn apply_refresh_result(
        &self,
        provider_id: &str,
        result: Result<ProviderRefresh, ProviderError>,
    ) -> ProviderViewState {
        let account_error = result
            .as_ref()
            .ok()
            .and_then(|refresh| refresh.account.as_ref())
            .is_some_and(|account| {
                self.settings.as_ref().is_some_and(|settings| {
                    settings
                        .activate_account(account.family, account.provider_id, &account.identity)
                        .is_err()
                })
            });
        let cache_error = !account_error
            && result.as_ref().ok().is_some_and(|refresh| {
                self.storage
                    .save_snapshot_for_identity(
                        &refresh.snapshot,
                        refresh.cache_identity.as_deref(),
                    )
                    .is_err()
            });
        if account_error {
            crate::app_warn!(
                "config",
                "account settings for {provider_id} could not be updated"
            );
        } else if cache_error {
            crate::app_warn!("cache", "snapshot for {provider_id} could not be persisted");
        } else if result.is_ok() {
            crate::app_debug!("cache", "snapshot for {provider_id} persisted");
        }
        let result = if account_error {
            Err(ProviderError::new(
                ProviderErrorKind::Storage,
                "The refreshed account state could not be saved.",
            ))
        } else {
            result.map(|refresh| refresh.snapshot)
        };
        self.update_state(provider_id, |state| {
            merge_refresh_result(state, result);
            if cache_error {
                state.error = Some(
                    "Usage refreshed, but the last successful snapshot could not be cached.".into(),
                );
                state.error_kind = Some(ProviderErrorKind::Storage);
            }
        });
        self.provider_state(provider_id)
    }

    /// 单点刷新落地后，同步顶部那一行依赖的两个事实。
    ///
    /// 批次收尾（`refresh_enabled_with_progress`）也会写这两个字段，但单点刷新压根
    /// 不经过批次收尾：登录、保存或删除密钥、菜单栏点击都走这里。少了这一步，用户
    /// 刚登录成功、数据已经在屏幕上，顶部却还挂着上一批留下的「上次刷新失败」。
    fn note_refresh_outcome(&self, state: &ProviderViewState) {
        if state.error.is_none() {
            self.note_refresh_succeeded();
        }
        self.note_refresh_failure_flag();
    }

    /// 记下「刚刚真的拿到过数据」。整批全失败或空批次都不会走到这里。
    fn note_refresh_succeeded(&self) {
        if let Ok(mut completed_at) = self.last_successful_refresh_at.write() {
            *completed_at = Some(Utc::now());
        }
    }

    /// 按各 provider 的现况重算失败标记，批次和单点共用这一条判定。
    fn note_refresh_failure_flag(&self) {
        // 先把结论算出来再拿写锁，别在持锁期间去读 states。
        let failed = self.any_enabled_provider_failed();
        if let Ok(mut failed_flag) = self.last_refresh_failed.write() {
            *failed_flag = failed;
        }
    }

    /// 屏幕上是否还有某个启用中的 provider 停在失败状态。
    ///
    /// 按每个 provider 的现况重算，而不是沿用「上一批有没有失败」这个结论：单点刷
    /// 新不知道上一批的结果，但它看得见谁现在有数据、谁没有。这样登录成功后自己那
    /// 一条立刻转好，而其他还没修好的 provider 依旧把这一行留成失败——一次局部成
    /// 功不会顺手把别人的问题一起藏掉。
    ///
    /// 单条判定见 [`provider_refresh_failed`]，顶部按选中 provider 显示时用的是同一
    /// 条规则。
    fn any_enabled_provider_failed(&self) -> bool {
        let enabled = self
            .settings
            .as_ref()
            .map(|settings| settings.enabled_provider_ids());
        // 先拿这把锁再读 states，顺序固定，避免和别的路径反向嵌套。
        let failures = self.last_failed_refresh.lock().ok();
        let Ok(states) = self.states.read() else {
            return false;
        };
        states.iter().any(|(provider_id, state)| {
            let is_enabled = enabled
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|id| id == provider_id));
            is_enabled && provider_refresh_failed(state, failures.as_deref(), provider_id)
        })
    }

    fn is_fresh_this_session(
        &self,
        provider_id: &str,
        selected_interval: Option<Duration>,
    ) -> bool {
        let last_success = self
            .last_live_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied());
        let last_failure = self
            .last_failed_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied());
        match (last_success, last_failure) {
            (_, Some(failure)) if last_success.is_none_or(|success| failure > success) => {
                failure.elapsed() < FAILURE_RETRY_BACKOFF
            }
            (Some(success), _) => {
                success.elapsed()
                    < selected_interval.unwrap_or_else(|| {
                        self.refresh_interval_for_provider(provider_id)
                            .unwrap_or(FAST_REFRESH_INTERVAL)
                    })
            }
            (None, _) => false,
        }
    }

    fn is_in_failure_backoff(&self, provider_id: &str) -> bool {
        self.last_failed_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied())
            .is_some_and(|instant| instant.elapsed() < FAILURE_RETRY_BACKOFF)
    }

    pub(crate) fn last_attempt_at(&self, provider_id: &str) -> Option<Instant> {
        let success = self
            .last_live_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied());
        let failure = self
            .last_failed_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied());
        match (success, failure) {
            (Some(success), Some(failure)) => Some(success.max(failure)),
            (Some(success), None) => Some(success),
            (None, failure) => failure,
        }
    }

    pub(crate) fn last_failure_at(&self, provider_id: &str) -> Option<Instant> {
        self.last_failed_refresh
            .lock()
            .ok()
            .and_then(|value| value.get(provider_id).copied())
    }

    pub(crate) fn refresh_interval_for_provider(&self, provider_id: &str) -> Option<Duration> {
        let Some(settings_service) = &self.settings else {
            return Some(FAST_REFRESH_INTERVAL);
        };
        let settings = settings_service.get();
        let enabled = settings
            .providers
            .iter()
            .any(|provider| provider.id == provider_id && provider.enabled);
        let has_native_instance = self.has_native_instance(&settings, provider_id);
        refresh_interval_for_provider(
            enabled,
            has_native_instance,
            settings.notifications.almost_out
                || settings.notifications.cutting_it_close
                || settings.notifications.will_run_out,
        )
    }

    fn has_native_instance(
        &self,
        settings: &crate::models::AppSettings,
        provider_id: &str,
    ) -> bool {
        #[cfg(target_os = "macos")]
        {
            let _ = settings;
            self.native_instance_ids
                .read()
                .is_ok_and(|ids| ids.contains(provider_id))
        }
        #[cfg(target_os = "windows")]
        {
            let _ = settings;
            self.native_instance_ids
                .read()
                .is_ok_and(|ids| ids.contains(provider_id))
        }
        #[cfg(target_os = "linux")]
        {
            let _ = settings;
            self.native_instance_ids
                .read()
                .is_ok_and(|ids| ids.contains(provider_id))
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            let _ = (settings, provider_id);
            false
        }
    }

    pub(crate) fn set_native_instance_ids(&self, provider_ids: impl IntoIterator<Item = String>) {
        if let Ok(mut ids) = self.native_instance_ids.write() {
            *ids = provider_ids.into_iter().collect();
        }
    }

    fn last_attempt_age(&self, provider_id: &str) -> Option<Duration> {
        self.last_attempt_at(provider_id)
            .map(|instant| instant.elapsed())
    }

    fn next_refresh_at(&self) -> Option<chrono::DateTime<Utc>> {
        let enabled_ids = if let Some(settings) = &self.settings {
            settings.enabled_provider_ids()
        } else {
            self.states.read().ok()?.keys().cloned().collect()
        };
        let monotonic_now = Instant::now();
        let utc_now = Utc::now();
        enabled_ids
            .iter()
            .filter_map(|provider_id| {
                let interval = self.refresh_interval_for_provider(provider_id)?;
                let attempt = self.last_attempt_at(provider_id);
                let failure = self.last_failure_at(provider_id);
                let due = if failure.is_some() && failure == attempt {
                    failure? + FAILURE_RETRY_BACKOFF
                } else {
                    attempt.map_or(monotonic_now, |at| at + interval)
                };
                let remaining = due.saturating_duration_since(monotonic_now);
                Some(utc_now + chrono::Duration::from_std(remaining).ok()?)
            })
            .min()
    }

    fn update_state(&self, provider_id: &str, update: impl FnOnce(&mut ProviderViewState)) {
        if let Ok(mut states) = self.states.write() {
            update(states.entry(provider_id.to_owned()).or_default());
        }
    }
}

struct RefreshFlight {
    state: Mutex<RefreshFlightState>,
    completed_tx: tokio::sync::watch::Sender<u64>,
}

#[derive(Default)]
struct RefreshFlightState {
    runner_active: bool,
    attempt_generation: Option<u64>,
    requested_generation: u64,
    completed_generation: u64,
}

fn settle_completed_generation(state: &mut RefreshFlightState, generation: u64) -> bool {
    state.completed_generation = generation;
    state.attempt_generation = None;
    if state.requested_generation > state.completed_generation {
        state.attempt_generation = Some(state.completed_generation.saturating_add(1));
        true
    } else {
        state.runner_active = false;
        false
    }
}

impl RefreshFlight {
    fn new() -> Self {
        let (completed_tx, _) = tokio::sync::watch::channel(0);
        Self {
            state: Mutex::new(RefreshFlightState::default()),
            completed_tx,
        }
    }
}

fn validate_snapshot(
    registry: &ProviderRegistry,
    provider_id: &str,
    snapshot: ProviderSnapshot,
) -> Result<ProviderSnapshot, ProviderError> {
    let Some(definition) = registry.definition(provider_id) else {
        return Err(snapshot_contract_error());
    };
    if snapshot.provider_id != provider_id {
        return Err(snapshot_contract_error());
    }

    let quota_sources = definition
        .metrics
        .iter()
        .filter_map(|metric| match &metric.source {
            MetricSource::Quota { source_id, .. }
            | MetricSource::QuotaOrValue { source_id, .. } => Some(source_id.as_str()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let value_sources = definition
        .metrics
        .iter()
        .filter_map(|metric| match &metric.source {
            MetricSource::Value { source_id }
            | MetricSource::QuotaOrValue { source_id, .. }
            | MetricSource::NearestCreditPackage { source_id } => Some(source_id.as_str()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let status_sources = definition
        .metrics
        .iter()
        .filter_map(|metric| match &metric.source {
            MetricSource::Status { source_id } => Some(source_id.as_str()),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    if snapshot
        .quotas
        .iter()
        .any(|quota| !quota_sources.contains(quota.id.as_str()))
        || snapshot
            .value_metrics
            .iter()
            .any(|metric| !value_sources.contains(metric.id.as_str()))
        || snapshot
            .status_metrics
            .iter()
            .any(|metric| !status_sources.contains(metric.id.as_str()))
        || has_duplicate_ids(snapshot.quotas.iter().map(|metric| metric.id.as_str()))
        || has_duplicate_ids(
            snapshot
                .value_metrics
                .iter()
                .map(|metric| metric.id.as_str()),
        )
        || has_duplicate_ids(
            snapshot
                .status_metrics
                .iter()
                .map(|metric| metric.id.as_str()),
        )
        || snapshot.quotas.iter().any(|quota| {
            (quota.format == crate::models::QuotaFormat::Count
                && quota
                    .unit
                    .as_deref()
                    .is_none_or(|unit| unit.trim().is_empty()))
                || (quota.estimated
                    && quota
                        .source_note
                        .as_deref()
                        .is_none_or(|note| note.trim().is_empty()))
        })
        || snapshot
            .status_metrics
            .iter()
            .any(|metric| metric.text.trim().is_empty() || metric.label.trim().is_empty())
    {
        return Err(snapshot_contract_error());
    }
    Ok(snapshot)
}

fn has_duplicate_ids<'a>(mut ids: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = std::collections::HashSet::new();
    ids.any(|id| !seen.insert(id))
}

fn snapshot_contract_error() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Internal,
        "Provider data does not match its registered metric contract.",
    )
}

fn merge_refresh_result(
    state: &mut ProviderViewState,
    result: Result<ProviderSnapshot, ProviderError>,
) {
    match result {
        Ok(snapshot) => {
            state.snapshot = Some(snapshot);
            state.source = SnapshotSource::Live;
            state.error = None;
            state.error_kind = None;
            state.stale = false;
        }
        Err(error) => {
            state.error_kind = Some(error.kind());
            state.error = Some(error.to_string());
        }
    }
    state.refreshing = false;
}

fn update_staleness_from_snapshot_age(state: &mut ProviderViewState, refresh_interval: Duration) {
    state.stale = state.snapshot.as_ref().is_some_and(|snapshot| {
        Utc::now().signed_duration_since(snapshot.refreshed_at) >= stale_after(refresh_interval)
    });
}

/// 单个 provider 现在算不算「失败」。
///
/// 两个信号一起看：`error` 是给用户看的失败文案，`last_failed_refresh` 是「上一次
/// 尝试失败了、还没成功过」。后者不能省——刷新开始时会把 `error` 清掉好让界面转圈，
/// 只看 `error` 的话，一个正在重试的 provider 会被误判成已经好了。
fn provider_refresh_failed(
    state: &ProviderViewState,
    failures: Option<&HashMap<String, Instant>>,
    provider_id: &str,
) -> bool {
    state.error.is_some() || failures.is_some_and(|failures| failures.contains_key(provider_id))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Condvar, Mutex,
        },
        thread,
        time::{Duration, Instant},
    };

    use chrono::Utc;
    use tempfile::tempdir;

    use super::{
        merge_refresh_result, resolve_proxy_url, update_staleness_from_snapshot_age,
        validate_snapshot, ProviderService, PROVIDER_REFRESH_TIMEOUT,
    };
    use crate::{
        models::{
            MetricDefinition, MetricSection, MetricSource, ProviderDefinition, ProviderErrorKind,
            ProviderSnapshot, ProviderViewState, QuotaFormat, QuotaWindow, SnapshotSource,
            StatusMetric, StatusTone, UsageHistory,
        },
        policy::{stale_after, FAILURE_RETRY_BACKOFF, FAST_REFRESH_INTERVAL},
        providers::{
            AccountRefresh, ProviderError, ProviderRefresh, ProviderRegistry,
            ProviderRequestContext, UsageProvider,
        },
        settings::SettingsService,
        storage::Storage,
    };

    const TEST_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn production_refresh_timeout_covers_the_longest_bounded_provider_flow() {
        assert!(PROVIDER_REFRESH_TIMEOUT >= Duration::from_secs(110));
    }

    #[test]
    fn provider_proxy_policy_is_resolved_per_provider() {
        assert_eq!(resolve_proxy_url(None, false).unwrap(), None);
        assert_eq!(
            resolve_proxy_url(Some("http://127.0.0.1:8080".into()), false).unwrap(),
            None
        );
        assert_eq!(
            resolve_proxy_url(Some("http://127.0.0.1:8080".into()), true)
                .unwrap()
                .unwrap()
                .host_str(),
            Some("127.0.0.1")
        );
        assert_eq!(resolve_proxy_url(None, true).unwrap(), None);
    }

    #[test]
    fn refresh_passes_only_the_selected_provider_proxy_policy() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let first_observed_proxy = Arc::new(Mutex::new(None));
        let second_observed_proxy = Arc::new(Mutex::new(None));
        let first = Arc::new(ProxyContextProvider {
            id: "proxy-enabled",
            observed_proxy: first_observed_proxy.clone(),
        });
        let second = Arc::new(ProxyContextProvider {
            id: "proxy-disabled",
            observed_proxy: second_observed_proxy.clone(),
        });
        let registry = Arc::new(
            ProviderRegistry::new(vec![first, second]).expect("test providers should be valid"),
        );
        let (settings, _) = SettingsService::new_deferred(storage.clone(), registry.clone())
            .expect("test settings should load");
        let mut configured = settings.get();
        configured.proxy_url = Some("http://127.0.0.1:8080".into());
        for provider in &mut configured.providers {
            provider.use_proxy = provider.id == "proxy-enabled";
        }
        settings
            .update(configured)
            .expect("test proxy policy should save");

        let service = Arc::new(ProviderService::new_with_settings(
            registry,
            storage,
            Arc::new(settings),
        ));
        let enabled_context = service.request_context_for("proxy-enabled");
        let disabled_context = service.request_context_for("proxy-disabled");
        assert!(enabled_context.proxy_url.is_some());
        assert!(disabled_context.proxy_url.is_none());
        assert!(Arc::ptr_eq(
            &enabled_context.http_clients,
            &disabled_context.http_clients
        ));
        assert!(refresh_with_test_timeout(&service, "proxy-enabled", true)
            .error
            .is_none());
        assert!(refresh_with_test_timeout(&service, "proxy-disabled", true)
            .error
            .is_none());

        assert_eq!(
            first_observed_proxy
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .as_ref()
                .and_then(reqwest::Url::host_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            second_observed_proxy.lock().unwrap().as_ref().unwrap(),
            &None
        );
    }

    struct SlowProvider {
        id: &'static str,
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        delay: Duration,
    }

    struct GatedProvider {
        id: &'static str,
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        gate: Arc<(Mutex<bool>, Condvar)>,
    }

    struct SequenceProvider {
        id: &'static str,
        calls: Arc<AtomicUsize>,
        failures_before_success: usize,
    }

    struct CredentialFailureProvider {
        id: &'static str,
        error_kind: ProviderErrorKind,
    }

    struct CredentialProvider {
        id: &'static str,
        credential: Arc<Mutex<Option<String>>>,
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        delay: Duration,
    }

    struct AccountSwitchProvider {
        identity: String,
    }

    struct ProxyContextProvider {
        id: &'static str,
        observed_proxy: Arc<Mutex<Option<Option<reqwest::Url>>>>,
    }

    impl UsageProvider for SlowProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            thread::sleep(self.delay);
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(test_snapshot(self.id))
        }
    }

    impl UsageProvider for GatedProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);

            let (released, signal) = &*self.gate;
            let released = released.lock().unwrap();
            let (released, _) = signal
                .wait_timeout_while(released, Duration::from_secs(2), |released| !*released)
                .unwrap();
            drop(released);

            self.active.fetch_sub(1, Ordering::SeqCst);
            let mut snapshot = test_snapshot(self.id);
            snapshot.plan = Some(format!("live-{call}"));
            Ok(snapshot)
        }
    }

    impl UsageProvider for SequenceProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call <= self.failures_before_success {
                Err(ProviderError::new(ProviderErrorKind::Network, "offline"))
            } else {
                Ok(test_snapshot(self.id))
            }
        }
    }

    impl UsageProvider for CredentialFailureProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            false
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            Err(ProviderError::new(
                self.error_kind,
                "Local credentials are unavailable.",
            ))
        }
    }

    impl UsageProvider for CredentialProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            self.credential.lock().unwrap().is_some()
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            let credential = self.credential.lock().unwrap().clone();
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(self.delay);
            self.active.fetch_sub(1, Ordering::SeqCst);
            let mut snapshot = test_snapshot(self.id);
            snapshot.plan = credential;
            Ok(snapshot)
        }
    }

    impl UsageProvider for AccountSwitchProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition("codex")
        }

        fn has_local_credentials(&self) -> bool {
            true
        }

        fn supports_account_names(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            Ok(test_snapshot("codex"))
        }

        fn refresh_for_service(&self) -> Result<ProviderRefresh, ProviderError> {
            self.refresh_for_service_with_context(&ProviderRequestContext::direct(
                std::sync::Arc::default(),
            ))
        }

        fn refresh_for_service_with_context(
            &self,
            _context: &ProviderRequestContext,
        ) -> Result<ProviderRefresh, ProviderError> {
            Ok(ProviderRefresh {
                snapshot: test_snapshot("codex"),
                cache_identity: Some(self.identity.clone()),
                account: Some(AccountRefresh {
                    family: "codex",
                    provider_id: "codex",
                    identity: self.identity.clone(),
                }),
            })
        }
    }

    impl UsageProvider for ProxyContextProvider {
        fn definition(&self) -> ProviderDefinition {
            test_definition(self.id)
        }

        fn has_local_credentials(&self) -> bool {
            true
        }

        fn refresh(&self) -> Result<ProviderSnapshot, ProviderError> {
            Ok(test_snapshot(self.id))
        }

        fn refresh_with_context(
            &self,
            context: &ProviderRequestContext,
        ) -> Result<ProviderSnapshot, ProviderError> {
            *self.observed_proxy.lock().unwrap() = Some(context.proxy_url.clone());
            Ok(test_snapshot(self.id))
        }
    }

    fn test_definition(id: &str) -> ProviderDefinition {
        ProviderDefinition {
            id: id.into(),
            display_name: id.into(),
            short_name: "T".into(),
            fallback_enabled: true,
            local_usage_source_note: None,
            links: vec![],
            metrics: vec![MetricDefinition::new(
                format!("{id}.session"),
                "Session",
                MetricSource::Quota {
                    source_id: "session".into(),
                    session_window: false,
                },
                true,
                true,
                MetricSection::AlwaysVisible,
                true,
                Some("S"),
                None,
            )],
        }
    }

    fn test_snapshot(provider_id: &str) -> ProviderSnapshot {
        ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: provider_id.into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        }
    }

    fn refresh_with_test_timeout(
        service: &Arc<ProviderService>,
        provider_id: &str,
        force: bool,
    ) -> ProviderViewState {
        tauri::async_runtime::block_on(async {
            tokio::time::timeout(TEST_WAIT_TIMEOUT, service.refresh(provider_id, force))
                .await
                .expect("refresh should not deadlock")
        })
    }

    fn wait_until(message: &str, predicate: impl Fn() -> bool) {
        let started = Instant::now();
        while !predicate() {
            assert!(started.elapsed() < TEST_WAIT_TIMEOUT, "{message}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn refresh_runner_is_idle(service: &ProviderService, provider_id: &str) -> bool {
        service
            .refresh_flights
            .get(provider_id)
            .and_then(|flight| flight.state.lock().ok())
            .is_some_and(|state| !state.runner_active)
    }

    #[test]
    fn failed_refresh_preserves_last_successful_snapshot_without_forcing_stale() {
        let snapshot = ProviderSnapshot {
            credit_packages: Vec::new(),
            provider_id: "codex".into(),
            plan: None,
            quotas: Vec::new(),
            value_metrics: Vec::new(),
            status_metrics: Vec::new(),
            notices: Vec::new(),
            usage: UsageHistory::default(),
            warnings: Vec::new(),
            refreshed_at: Utc::now(),
        };
        let mut state = ProviderViewState {
            snapshot: Some(snapshot.clone()),
            ..ProviderViewState::default()
        };
        merge_refresh_result(
            &mut state,
            Err(ProviderError::new(ProviderErrorKind::Network, "offline")),
        );
        assert_eq!(state.snapshot, Some(snapshot));
        assert!(!state.stale);
        assert_eq!(state.error.as_deref(), Some("offline"));
        assert_eq!(state.error_kind, Some(ProviderErrorKind::Network));
    }

    #[test]
    fn local_credential_failure_keeps_enabled_instance_and_stales_cached_snapshot_by_age() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let mut snapshot = test_snapshot("codex");
        snapshot.refreshed_at = Utc::now()
            - stale_after(crate::policy::SLOW_REFRESH_INTERVAL)
            - chrono::Duration::minutes(1);
        storage.save_snapshot(&snapshot).unwrap();
        let provider = Arc::new(CredentialFailureProvider {
            id: "codex",
            error_kind: ProviderErrorKind::CredentialsUnavailable,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let (settings, plan) =
            SettingsService::new_deferred(storage.clone(), registry.clone()).unwrap();
        settings
            .apply_credential_detection(
                &plan,
                &std::collections::HashMap::from([(
                    "codex".to_owned(),
                    crate::providers::CredentialProbeStatus::Detected,
                )]),
            )
            .unwrap();
        let service = Arc::new(ProviderService::new_with_settings(
            registry,
            storage,
            Arc::new(settings),
        ));

        let state = refresh_with_test_timeout(&service, "codex", true);

        assert_eq!(state.snapshot, Some(snapshot));
        assert!(state.stale);
        assert_eq!(
            state.error.as_deref(),
            Some("Local credentials are unavailable.")
        );
        assert_eq!(
            state.error_kind,
            Some(ProviderErrorKind::CredentialsUnavailable)
        );
        assert!(service
            .settings
            .as_ref()
            .unwrap()
            .get()
            .providers
            .iter()
            .any(|provider| provider.id == "codex" && provider.enabled));
    }

    #[test]
    fn local_credential_failure_without_snapshot_sets_error_without_staleness() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let provider = Arc::new(CredentialFailureProvider {
            id: "codex",
            error_kind: ProviderErrorKind::CredentialsUnavailable,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let state = refresh_with_test_timeout(&service, "codex", true);

        assert!(state.snapshot.is_none());
        assert!(!state.stale);
        assert_eq!(
            state.error_kind,
            Some(ProviderErrorKind::CredentialsUnavailable)
        );
    }

    #[test]
    fn cached_snapshot_staleness_is_based_on_snapshot_age() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let provider = Arc::new(SlowProvider {
            id: "cached",
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            maximum: Arc::new(AtomicUsize::new(0)),
            delay: Duration::ZERO,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());

        storage.save_snapshot(&test_snapshot("cached")).unwrap();
        let fresh_service = ProviderService::new(registry.clone(), storage.clone());
        let fresh = fresh_service.state();
        let fresh = fresh.providers.get("cached").unwrap();
        assert_eq!(fresh.source, SnapshotSource::Cache);
        assert!(!fresh.stale);

        let mut old_snapshot = test_snapshot("cached");
        old_snapshot.refreshed_at =
            Utc::now() - stale_after(FAST_REFRESH_INTERVAL) - chrono::Duration::seconds(1);
        storage.save_snapshot(&old_snapshot).unwrap();
        let old_service = ProviderService::new(registry, storage);
        let old = old_service.state();
        assert!(old.providers.get("cached").unwrap().stale);
    }

    #[test]
    fn cached_snapshot_uses_ten_or_thirty_minute_staleness_thresholds() {
        let mut state = ProviderViewState {
            snapshot: Some(test_snapshot("cached")),
            ..ProviderViewState::default()
        };
        state.snapshot.as_mut().unwrap().refreshed_at = Utc::now() - chrono::Duration::minutes(12);

        update_staleness_from_snapshot_age(&mut state, crate::policy::FAST_REFRESH_INTERVAL);
        assert!(state.stale);
        update_staleness_from_snapshot_age(&mut state, crate::policy::SLOW_REFRESH_INTERVAL);
        assert!(!state.stale);

        state.snapshot.as_mut().unwrap().refreshed_at = Utc::now() - chrono::Duration::minutes(31);
        update_staleness_from_snapshot_age(&mut state, crate::policy::SLOW_REFRESH_INTERVAL);
        assert!(state.stale);
    }

    #[test]
    fn successful_refresh_switches_account_name_and_cache_without_a_restart() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let provider = Arc::new(AccountSwitchProvider {
            identity: "bbbbbbbb22222222".into(),
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let (settings, _) =
            SettingsService::new_deferred(storage.clone(), registry.clone()).unwrap();
        let settings = Arc::new(settings);
        settings
            .activate_account("codex", "codex", "aaaaaaaa11111111")
            .unwrap();
        let mut renamed = settings.get();
        renamed.provider_names.insert("codex".into(), "GPT".into());
        settings
            .update_from_view(
                renamed,
                settings.settings_revision(),
                settings.account_revision(),
            )
            .unwrap();

        let service = Arc::new(ProviderService::new_with_settings(
            registry,
            storage.clone(),
            settings.clone(),
        ));
        let state = refresh_with_test_timeout(&service, "codex", true);

        assert!(state.error.is_none());
        assert!(!settings.get().provider_names.contains_key("codex"));
        assert!(storage
            .load_snapshot_for_identity(
                "codex",
                crate::providers::CacheIdentity::Resolved("bbbbbbbb22222222"),
            )
            .unwrap()
            .is_some());
        assert!(storage
            .load_snapshot_for_identity(
                "codex",
                crate::providers::CacheIdentity::Resolved("aaaaaaaa11111111"),
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn snapshot_contract_rejects_wrong_provider_and_unknown_sources() {
        let provider = SlowProvider {
            id: "contract",
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            maximum: Arc::new(AtomicUsize::new(0)),
            delay: Duration::ZERO,
        };
        let registry = ProviderRegistry::from_definitions(vec![provider.definition()]).unwrap();
        let snapshot = provider.refresh().unwrap();

        let mut wrong_provider = snapshot.clone();
        wrong_provider.provider_id = "other".into();
        assert!(validate_snapshot(&registry, "contract", wrong_provider).is_err());

        let mut unknown_source = snapshot;
        unknown_source.quotas.push(crate::models::QuotaWindow {
            id: "unknown".into(),
            label: "Unknown".into(),
            used_percent: 0.0,
            resets_at: None,
            period_seconds: 1,
            format: crate::models::QuotaFormat::Percent,
            used_value: None,
            limit_value: None,
            unit: None,
            estimated: false,
            source_note: None,
        });
        assert!(validate_snapshot(&registry, "contract", unknown_source).is_err());
    }

    #[test]
    fn snapshot_contract_validates_dynamic_metric_metadata_and_unique_ids() {
        let definition = ProviderDefinition {
            id: "dynamic".into(),
            display_name: "Dynamic".into(),
            short_name: "D".into(),
            fallback_enabled: true,
            local_usage_source_note: None,
            links: Vec::new(),
            metrics: vec![
                MetricDefinition::quota(
                    "dynamic.searches",
                    "Web Searches",
                    "searches",
                    false,
                    true,
                    MetricSection::AlwaysVisible,
                    true,
                    "S",
                ),
                MetricDefinition::status(
                    "dynamic.extra",
                    "Extra Usage",
                    "extra",
                    true,
                    MetricSection::OnDemand,
                    false,
                    "E",
                ),
            ],
        };
        let registry = ProviderRegistry::from_definitions(vec![definition]).unwrap();
        let mut snapshot = test_snapshot("dynamic");
        snapshot.quotas.push(QuotaWindow {
            id: "searches".into(),
            label: "Web Searches".into(),
            used_percent: 25.0,
            resets_at: None,
            period_seconds: 86_400,
            format: QuotaFormat::Count,
            used_value: Some(25.0),
            limit_value: Some(100.0),
            unit: Some("searches".into()),
            estimated: false,
            source_note: None,
        });
        snapshot.status_metrics.push(StatusMetric {
            id: "extra".into(),
            label: "Extra Usage".into(),
            text: "2500 cap".into(),
            tone: StatusTone::Positive,
            subtitle: None,
        });

        assert!(validate_snapshot(&registry, "dynamic", snapshot.clone()).is_ok());

        let mut missing_unit = snapshot.clone();
        missing_unit.quotas[0].unit = Some(" ".into());
        assert!(validate_snapshot(&registry, "dynamic", missing_unit).is_err());

        let mut missing_estimate_source = snapshot.clone();
        missing_estimate_source.quotas[0].estimated = true;
        assert!(validate_snapshot(&registry, "dynamic", missing_estimate_source).is_err());

        let mut duplicate_quota = snapshot.clone();
        duplicate_quota
            .quotas
            .push(duplicate_quota.quotas[0].clone());
        assert!(validate_snapshot(&registry, "dynamic", duplicate_quota).is_err());

        let mut unknown_status = snapshot;
        unknown_status.status_metrics[0].id = "unknown".into();
        assert!(validate_snapshot(&registry, "dynamic", unknown_status).is_err());
    }

    #[test]
    fn enabled_providers_refresh_in_parallel() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let providers = ["claude", "antigravity"]
            .into_iter()
            .map(|id| {
                Arc::new(SlowProvider {
                    id,
                    calls: calls.clone(),
                    active: active.clone(),
                    maximum: maximum.clone(),
                    delay: Duration::from_millis(75),
                }) as Arc<dyn UsageProvider>
            })
            .collect();
        let registry = Arc::new(ProviderRegistry::new(providers).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        tauri::async_runtime::block_on(
            service.refresh_enabled(&["claude".into(), "antigravity".into()], true),
        );

        assert_eq!(maximum.load(Ordering::SeqCst), 2);
        assert!(service.state().last_full_refresh_at.is_none());

        let completed = tauri::async_runtime::block_on(
            service.refresh_all(&["claude".into(), "antigravity".into()], true),
        );
        assert!(completed.last_full_refresh_at.is_some());
    }

    #[test]
    fn timed_out_refresh_quarantines_the_late_worker_and_releases_waiters() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let mut cached = test_snapshot("slow");
        cached.plan = Some("cached".into());
        storage.save_snapshot(&cached).unwrap();

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let provider = Arc::new(GatedProvider {
            id: "slow",
            calls: calls.clone(),
            active: active.clone(),
            maximum: maximum.clone(),
            gate: gate.clone(),
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::with_refresh_timeout(
            registry,
            storage.clone(),
            Duration::from_millis(250),
        ));

        let (timed_out, queued) =
            tauri::async_runtime::block_on(async {
                let first_service = service.clone();
                let first = tauri::async_runtime::spawn(async move {
                    first_service.refresh("slow", true).await
                });
                tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                    while active.load(Ordering::SeqCst) == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("first refresh should start");
                let queued_service = service.clone();
                let queued = tauri::async_runtime::spawn(async move {
                    queued_service.refresh("slow", true).await
                });
                tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                    loop {
                        let queued = service
                            .refresh_flights
                            .get("slow")
                            .and_then(|flight| flight.state.lock().ok())
                            .is_some_and(|state| state.requested_generation >= 2);
                        if queued && active.load(Ordering::SeqCst) == 1 {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("forced follow-up should be queued before timeout");

                tokio::time::timeout(Duration::from_secs(1), async {
                    (first.await.unwrap(), queued.await.unwrap())
                })
                .await
                .expect("timeout should release active and queued waiters")
            });

        for state in [&timed_out, &queued] {
            assert_eq!(state.error.as_deref(), Some("Provider refresh timed out."));
            assert_eq!(state.error_kind, Some(ProviderErrorKind::Network));
            assert!(!state.stale);
            assert_eq!(
                state
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.plan.as_deref()),
                Some("cached")
            );
        }
        {
            let flight_state = service.refresh_flights["slow"].state.lock().unwrap();
            assert!(flight_state.runner_active);
            assert_eq!(flight_state.attempt_generation, None);
            assert_eq!(flight_state.completed_generation, 2);
            assert_eq!(flight_state.requested_generation, 2);
        }

        let quarantined = tauri::async_runtime::block_on(async {
            let tasks = (0..32)
                .map(|_| {
                    let service = service.clone();
                    tauri::async_runtime::spawn(async move { service.refresh("slow", true).await })
                })
                .collect::<Vec<_>>();
            tokio::time::timeout(Duration::from_secs(1), async {
                let mut states = Vec::with_capacity(tasks.len());
                for task in tasks {
                    states.push(task.await.unwrap());
                }
                states
            })
            .await
            .expect("quarantined refresh calls should return immediately")
        });
        assert!(quarantined
            .iter()
            .all(|state| state.error == timed_out.error));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        assert_eq!(active.load(Ordering::SeqCst), 1);
        assert!(!refresh_runner_is_idle(&service, "slow"));

        let (released, signal) = &*gate;
        *released.lock().unwrap() = true;
        signal.notify_all();

        wait_until("timed-out refresh worker should drain", || {
            active.load(Ordering::SeqCst) == 0 && refresh_runner_is_idle(&service, "slow")
        });
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(refresh_runner_is_idle(&service, "slow"));
        assert_eq!(
            service
                .state()
                .providers
                .get("slow")
                .and_then(|state| state.snapshot.as_ref())
                .and_then(|snapshot| snapshot.plan.as_deref()),
            Some("cached")
        );
        assert_eq!(
            storage
                .load_snapshot("slow")
                .unwrap()
                .and_then(|snapshot| snapshot.plan),
            Some("cached".into())
        );

        let refreshed = refresh_with_test_timeout(&service, "slow", true);
        assert!(refreshed.error.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        assert_eq!(
            refreshed
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.plan.as_deref()),
            Some("live-2")
        );
    }

    #[test]
    fn cancelled_refresh_keeps_single_flight_until_blocking_worker_finishes() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(SlowProvider {
            id: "cancelled",
            calls: calls.clone(),
            active: active.clone(),
            maximum: maximum.clone(),
            delay: Duration::from_millis(180),
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::with_refresh_timeout(
            registry,
            storage,
            Duration::from_secs(1),
        ));

        let first_service = service.clone();
        let first =
            tauri::async_runtime::spawn(
                async move { first_service.refresh("cancelled", true).await },
            );
        wait_until("cancelled refresh worker should start", || {
            active.load(Ordering::SeqCst) != 0
        });
        assert_eq!(active.load(Ordering::SeqCst), 1);

        first.abort();
        let _ = tauri::async_runtime::block_on(first);
        assert!(service
            .state()
            .providers
            .get("cancelled")
            .is_some_and(|state| state.refreshing));

        let retry = refresh_with_test_timeout(&service, "cancelled", true);
        assert!(!retry.refreshing);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(retry.error.is_none());
    }

    #[test]
    fn concurrent_forced_waiters_coalesce_one_follow_up() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(SlowProvider {
            id: "lifecycle",
            calls: calls.clone(),
            active: active.clone(),
            maximum: maximum.clone(),
            delay: Duration::from_millis(80),
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        tauri::async_runtime::block_on(async {
            tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                let first_service = service.clone();
                let first = tauri::async_runtime::spawn(async move {
                    first_service.refresh("lifecycle", true).await
                });
                tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                    while active.load(Ordering::SeqCst) == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("first refresh should start");
                let second_service = service.clone();
                let second = tauri::async_runtime::spawn(async move {
                    second_service.refresh("lifecycle", true).await
                });
                let third_service = service.clone();
                let third = tauri::async_runtime::spawn(async move {
                    third_service.refresh("lifecycle", true).await
                });
                assert!(first.await.unwrap().error.is_none());
                assert!(second.await.unwrap().error.is_none());
                assert!(third.await.unwrap().error.is_none());
            })
            .await
            .expect("coalesced refreshes should not deadlock");
        });

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn forced_refresh_after_credential_change_returns_authoritative_snapshot() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let credential = Arc::new(Mutex::new(Some("old".to_owned())));
        let calls = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(CredentialProvider {
            id: "credential",
            credential: credential.clone(),
            calls: calls.clone(),
            active: active.clone(),
            maximum: maximum.clone(),
            delay: Duration::from_millis(60),
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        tauri::async_runtime::block_on(async {
            tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                let old_service = service.clone();
                let old_refresh = tauri::async_runtime::spawn(async move {
                    old_service.refresh("credential", true).await
                });
                tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                    while calls.load(Ordering::SeqCst) < 1 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("old-credential refresh should start");
                *credential.lock().unwrap() = Some("saved".to_owned());
                let saved = service.refresh("credential", true).await;
                assert_eq!(
                    saved.snapshot.and_then(|snapshot| snapshot.plan),
                    Some("saved".to_owned())
                );
                assert!(old_refresh.await.unwrap().error.is_none());

                let saved_service = service.clone();
                let saved_refresh = tauri::async_runtime::spawn(async move {
                    saved_service.refresh("credential", true).await
                });
                tokio::time::timeout(TEST_WAIT_TIMEOUT, async {
                    while calls.load(Ordering::SeqCst) < 3 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("saved-credential refresh should start");
                *credential.lock().unwrap() = None;
                let deleted = service.refresh("credential", true).await;
                assert_eq!(deleted.snapshot.and_then(|snapshot| snapshot.plan), None);
                assert!(saved_refresh.await.unwrap().error.is_none());
            })
            .await
            .expect("credential refreshes should not deadlock");
        });

        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn progress_reports_fast_provider_before_a_slow_provider_finishes() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let providers = vec![
            Arc::new(GatedProvider {
                id: "slow",
                calls: calls.clone(),
                active: active.clone(),
                maximum: maximum.clone(),
                gate: gate.clone(),
            }) as Arc<dyn UsageProvider>,
            Arc::new(SlowProvider {
                id: "fast",
                calls: calls.clone(),
                active: active.clone(),
                maximum: maximum.clone(),
                delay: Duration::ZERO,
            }) as Arc<dyn UsageProvider>,
        ];
        let registry = Arc::new(ProviderRegistry::new(providers).unwrap());
        let service = Arc::new(ProviderService::with_refresh_timeout(
            registry,
            storage.clone(),
            Duration::from_secs(3),
        ));
        let (progress_tx, progress_rx) = std::sync::mpsc::channel();
        let refresh_service = service.clone();
        let batch = tauri::async_runtime::spawn(async move {
            refresh_service
                .refresh_enabled_with_progress(
                    &["slow".into(), "fast".into()],
                    true,
                    move |state| {
                        let _ = progress_tx.send(state.clone());
                    },
                )
                .await
        });

        // This is the state the refresh loop emits to the native tray and
        // notification evaluator. It must arrive before the slow worker ends.
        let first_progress = progress_rx
            .recv_timeout(TEST_WAIT_TIMEOUT)
            .expect("fast provider completion should publish before the slow provider finishes");
        assert!(first_progress
            .providers
            .get("fast")
            .and_then(|state| state.snapshot.as_ref())
            .is_some());
        assert!(first_progress
            .providers
            .get("slow")
            .is_some_and(|state| state.refreshing));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let (released, signal) = &*gate;
        assert!(!*released.lock().unwrap());
        *released.lock().unwrap() = true;
        signal.notify_all();
        let final_state = tauri::async_runtime::block_on(batch).unwrap();
        assert!(final_state
            .providers
            .get("slow")
            .and_then(|state| state.snapshot.as_ref())
            .is_some());
        assert!(storage.load_snapshot("fast").unwrap().is_some());
        assert!(storage.load_snapshot("slow").unwrap().is_some());
    }

    #[test]
    fn failed_provider_is_backed_off_but_force_and_expiry_retry() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(SequenceProvider {
            id: "failing",
            calls: calls.clone(),
            failures_before_success: usize::MAX,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        tauri::async_runtime::block_on(service.refresh("failing", false));
        tauri::async_runtime::block_on(service.refresh("failing", false));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        tauri::async_runtime::block_on(service.refresh("failing", true));
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        service.last_failed_refresh.lock().unwrap().insert(
            "failing".into(),
            std::time::Instant::now()
                .checked_sub(FAILURE_RETRY_BACKOFF)
                .unwrap(),
        );
        tauri::async_runtime::block_on(service.refresh("failing", false));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn successful_retry_clears_failure_backoff() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(SequenceProvider {
            id: "recovering",
            calls,
            failures_before_success: 1,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        tauri::async_runtime::block_on(service.refresh("recovering", false));
        assert!(service
            .last_failed_refresh
            .lock()
            .unwrap()
            .contains_key("recovering"));

        let recovered = tauri::async_runtime::block_on(service.refresh("recovering", true));
        assert!(recovered.error.is_none());
        assert!(!service
            .last_failed_refresh
            .lock()
            .unwrap()
            .contains_key("recovering"));
    }

    /// 顶部的「最近更新」要回答的是「我看到的数字是不是最新的」，所以它必须只在
    /// 真的拿到数据时前进。整批全失败时若照常推进，用户会把一个陈旧的数字当成刚
    /// 更新的——那正是这个功能要消除的误解。
    #[test]
    fn a_batch_where_every_provider_failed_does_not_advance_the_last_successful_refresh() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let provider = Arc::new(CredentialFailureProvider {
            id: "offline",
            error_kind: ProviderErrorKind::Authentication,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let failed = tauri::async_runtime::block_on(service.refresh_all(&["offline".into()], true));

        assert!(
            failed.last_successful_refresh_at.is_none(),
            "a batch with no successful provider must not claim a successful update"
        );
        assert!(
            failed.last_refresh_failed,
            "the failure has to be visible next to the timestamp"
        );
        // 批次确实结束了，所以「尝试时间」照常前进：两者语义不同。
        assert!(failed.last_full_refresh_at.is_some());
    }

    /// 部分成功就算拿到数据：至少一个 provider 更新了，顶部时间应当前进，但失败
    /// 标记仍要亮着，好让用户知道这屏里有一部分是旧的。
    #[test]
    fn a_partially_successful_batch_advances_the_timestamp_and_still_reports_failure() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let healthy = Arc::new(SequenceProvider {
            id: "healthy",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 0,
        }) as Arc<dyn UsageProvider>;
        let broken = Arc::new(CredentialFailureProvider {
            id: "broken",
            error_kind: ProviderErrorKind::Authentication,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![healthy, broken]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let state = tauri::async_runtime::block_on(
            service.refresh_all(&["healthy".into(), "broken".into()], true),
        );

        assert!(
            state.last_successful_refresh_at.is_some(),
            "one provider did return data, so the timestamp must move"
        );
        assert!(state.last_refresh_failed);
    }

    /// 一次全成功的批次要把上一次留下的失败标记清掉，否则顶部会一直挂着一条早已
    /// 过期的警告。
    #[test]
    fn a_fully_successful_batch_clears_the_failure_flag() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let recovering = Arc::new(SequenceProvider {
            id: "recovering",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 1,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![recovering]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let failed =
            tauri::async_runtime::block_on(service.refresh_all(&["recovering".into()], true));
        assert!(failed.last_refresh_failed);
        assert!(failed.last_successful_refresh_at.is_none());

        let succeeded =
            tauri::async_runtime::block_on(service.refresh_all(&["recovering".into()], true));
        assert!(!succeeded.last_refresh_failed);
        assert!(succeeded.last_successful_refresh_at.is_some());
    }

    /// 首次登录就是这条路径：登录成功后只刷新刚登录的那一个 provider，不走批次。
    /// 如果顶部那两个字段只有批次收尾能写，用户会看到数据已经到手、顶部却还挂着上
    /// 一批留下的「上次刷新失败」。
    #[test]
    fn a_successful_single_provider_refresh_updates_the_top_row() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let provider = Arc::new(SequenceProvider {
            id: "signing-in",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 1,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![provider]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let failed =
            tauri::async_runtime::block_on(service.refresh_all(&["signing-in".into()], true));
        assert!(failed.last_refresh_failed);
        assert!(failed.last_successful_refresh_at.is_none());

        let signed_in = refresh_with_test_timeout(&service, "signing-in", true);
        assert!(
            signed_in.error.is_none(),
            "the sign-in refresh has to succeed"
        );

        let state = service.state();
        assert!(
            state.last_successful_refresh_at.is_some(),
            "data arrived, so the top row must stop calling itself stale"
        );
        assert!(
            !state.last_refresh_failed,
            "the failed batch before sign-in is no longer the current truth"
        );
    }

    /// 反面：单点成功不能顺手把别的 provider 的故障一起抹掉，否则顶部会假装整屏都
    /// 是新的。
    #[test]
    fn a_single_provider_success_does_not_hide_another_providers_failure() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let first = Arc::new(SequenceProvider {
            id: "first",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 1,
        }) as Arc<dyn UsageProvider>;
        let second = Arc::new(SequenceProvider {
            id: "second",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 1,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![first, second]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let failed = tauri::async_runtime::block_on(
            service.refresh_all(&["first".into(), "second".into()], true),
        );
        assert!(failed.last_refresh_failed);

        assert!(refresh_with_test_timeout(&service, "first", true)
            .error
            .is_none());
        let after_first = service.state();
        assert!(after_first.last_successful_refresh_at.is_some());
        assert!(
            after_first.last_refresh_failed,
            "second is still broken, so the warning has to stay up"
        );

        assert!(refresh_with_test_timeout(&service, "second", true)
            .error
            .is_none());
        assert!(!service.state().last_refresh_failed);
    }

    /// 顶部那一行按「当前正在看的 provider」显示，所以每个 provider 都要带上自己的
    /// 失败状态：看好好的那个时，不能因为另一个挂了就报失败。
    #[test]
    fn each_provider_carries_its_own_failure_flag() {
        let directory = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&directory.path().join("quota01.db")).unwrap());
        let healthy = Arc::new(SequenceProvider {
            id: "healthy",
            calls: Arc::new(AtomicUsize::new(0)),
            failures_before_success: 0,
        }) as Arc<dyn UsageProvider>;
        let broken = Arc::new(CredentialFailureProvider {
            id: "broken",
            error_kind: ProviderErrorKind::Authentication,
        }) as Arc<dyn UsageProvider>;
        let registry = Arc::new(ProviderRegistry::new(vec![healthy, broken]).unwrap());
        let service = Arc::new(ProviderService::new(registry, storage));

        let state = tauri::async_runtime::block_on(
            service.refresh_all(&["healthy".into(), "broken".into()], true),
        );

        assert!(
            !state.providers["healthy"].last_refresh_failed,
            "the healthy provider must not inherit the other one's failure"
        );
        assert!(state.providers["broken"].last_refresh_failed);
        // 整体结论仍然亮着：这屏里确实有一份是旧的。
        assert!(state.last_refresh_failed);

        // 失败后停在退避里、直接返回现况的那条早退路径也要带上标记。
        assert!(refresh_with_test_timeout(&service, "broken", false).last_refresh_failed);

        // 单点成功只影响自己那一条，别的 provider 的标记原样留着。
        assert!(!refresh_with_test_timeout(&service, "healthy", true).last_refresh_failed);
        assert!(service.state().providers["broken"].last_refresh_failed);
    }
}
