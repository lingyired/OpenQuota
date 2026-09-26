use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicU64, Arc},
    time::{Duration, Instant},
};

use tauri::{AppHandle, Emitter};

use crate::{
    commands::settings::emit_settings_if_account_changed, notifications::finish_refresh,
    pacing::NotificationEvaluator, policy::FAILURE_RETRY_BACKOFF, service::ProviderService,
    settings::SettingsService,
};

pub fn spawn(
    app: AppHandle,
    service: Arc<ProviderService>,
    settings: Arc<SettingsService>,
    notifications: Arc<NotificationEvaluator>,
) {
    tauri::async_runtime::spawn(async move {
        let mut schedule = HashMap::new();
        loop {
            let provider_ids = settings.enabled_provider_ids();
            let intervals = provider_ids
                .iter()
                .filter_map(|provider_id| {
                    service
                        .refresh_interval_for_provider(provider_id)
                        .map(|interval| (provider_id.clone(), interval))
                })
                .collect::<HashMap<_, _>>();
            let attempts = provider_ids
                .iter()
                .filter_map(|provider_id| {
                    service
                        .last_attempt_at(provider_id)
                        .map(|at| (provider_id.clone(), at))
                })
                .collect::<HashMap<_, _>>();
            let failures = provider_ids
                .iter()
                .filter_map(|provider_id| {
                    service
                        .last_failure_at(provider_id)
                        .map(|at| (provider_id.clone(), at))
                })
                .collect::<HashMap<_, _>>();
            let due_provider_ids = due_provider_ids(
                &mut schedule,
                &intervals,
                &attempts,
                &failures,
                Instant::now(),
            );
            if !due_provider_ids.is_empty() {
                let progress_app = app.clone();
                let progress_settings = settings.clone();
                let observed_account_revision =
                    Arc::new(AtomicU64::new(settings.account_revision()));
                let progress_account_revision = observed_account_revision.clone();
                let state = service
                    .refresh_enabled_with_progress(&due_provider_ids, false, move |state| {
                        emit_settings_if_account_changed(
                            &progress_app,
                            &progress_settings,
                            &progress_account_revision,
                        );
                        let _ = progress_app.emit("usage-state", state);
                    })
                    .await;
                emit_settings_if_account_changed(&app, &settings, &observed_account_revision);
                let _ = app.emit("usage-state", &state);
                finish_refresh(&app, &state, &settings, &notifications);
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

#[derive(Debug, Clone, Copy)]
struct ScheduledRefresh {
    interval: Duration,
    due_at: Instant,
    last_attempt: Option<Instant>,
    last_failure: Option<Instant>,
}

fn due_provider_ids(
    schedule: &mut HashMap<String, ScheduledRefresh>,
    intervals: &HashMap<String, Duration>,
    attempts: &HashMap<String, Instant>,
    failures: &HashMap<String, Instant>,
    now: Instant,
) -> Vec<String> {
    let enabled = intervals.keys().collect::<HashSet<_>>();
    schedule.retain(|provider_id, _| enabled.contains(provider_id));
    let mut due = Vec::new();
    for (provider_id, interval) in intervals {
        let last_attempt = attempts.get(provider_id).copied();
        let last_failure = failures.get(provider_id).copied();
        let refresh_changed = schedule.get(provider_id).is_none_or(|entry| {
            entry.interval != *interval
                || entry.last_attempt != last_attempt
                || entry.last_failure != last_failure
        });
        if refresh_changed {
            let due_at = match (last_failure, last_attempt) {
                (Some(failure), Some(attempt)) if failure == attempt => {
                    failure + FAILURE_RETRY_BACKOFF
                }
                _ => last_attempt.map_or(now, |attempt| attempt + *interval),
            };
            schedule.insert(
                provider_id.clone(),
                ScheduledRefresh {
                    interval: *interval,
                    due_at,
                    last_attempt,
                    last_failure,
                },
            );
        }
        let entry = schedule
            .get_mut(provider_id)
            .expect("entry just reconciled");
        if entry.due_at <= now {
            due.push(provider_id.clone());
            // Suppress another dispatch while the refresh is in flight. A
            // completed attempt replaces this provisional deadline next pass.
            entry.due_at = now + entry.interval;
        }
    }
    due.sort();
    due
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::FAST_REFRESH_INTERVAL;

    fn intervals(entries: &[(&str, Duration)]) -> HashMap<String, Duration> {
        entries
            .iter()
            .map(|(id, interval)| ((*id).to_owned(), *interval))
            .collect()
    }

    #[test]
    fn new_enabled_provider_is_due_immediately_and_disabled_provider_is_removed() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        assert_eq!(
            due_provider_ids(
                &mut schedule,
                &intervals(&[
                    ("fast", FAST_REFRESH_INTERVAL),
                    ("removed", FAST_REFRESH_INTERVAL)
                ]),
                &HashMap::new(),
                &HashMap::new(),
                now,
            ),
            vec!["fast", "removed"]
        );
        assert_eq!(
            due_provider_ids(
                &mut schedule,
                &intervals(&[("fast", FAST_REFRESH_INTERVAL)]),
                &HashMap::new(),
                &HashMap::new(),
                now + Duration::from_secs(1),
            ),
            Vec::<String>::new()
        );
        assert!(!schedule.contains_key("removed"));
    }

    #[test]
    fn per_provider_due_times_and_interval_changes_are_recomputed() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        let fast = intervals(&[("fast", FAST_REFRESH_INTERVAL)]);
        assert_eq!(
            due_provider_ids(&mut schedule, &fast, &HashMap::new(), &HashMap::new(), now),
            vec!["fast"]
        );

        let last_attempt = now + Duration::from_secs(1);
        let attempts = HashMap::from([("fast".to_owned(), last_attempt)]);
        assert!(due_provider_ids(
            &mut schedule,
            &fast,
            &attempts,
            &HashMap::new(),
            last_attempt + Duration::from_secs(299)
        )
        .is_empty());
        assert_eq!(
            due_provider_ids(
                &mut schedule,
                &fast,
                &attempts,
                &HashMap::new(),
                last_attempt + FAST_REFRESH_INTERVAL
            ),
            vec!["fast"]
        );

        let slow = intervals(&[("fast", Duration::from_secs(15 * 60))]);
        assert!(due_provider_ids(
            &mut schedule,
            &slow,
            &attempts,
            &HashMap::new(),
            last_attempt + FAST_REFRESH_INTERVAL + Duration::from_secs(1)
        )
        .is_empty());
        assert_eq!(
            due_provider_ids(
                &mut schedule,
                &fast,
                &attempts,
                &HashMap::new(),
                last_attempt + Duration::from_secs(10 * 60),
            ),
            vec!["fast"]
        );
    }

    #[test]
    fn failed_provider_retries_after_the_backoff() {
        let now = Instant::now();
        let mut schedule = HashMap::new();
        let failure = now - Duration::from_secs(59);
        let attempts = HashMap::from([("failed".to_owned(), failure)]);
        let failures = attempts.clone();
        let intervals = intervals(&[("failed", FAST_REFRESH_INTERVAL)]);
        assert!(due_provider_ids(&mut schedule, &intervals, &attempts, &failures, now).is_empty());
        assert_eq!(
            due_provider_ids(
                &mut schedule,
                &intervals,
                &attempts,
                &failures,
                failure + FAILURE_RETRY_BACKOFF,
            ),
            vec!["failed"]
        );
    }
}
