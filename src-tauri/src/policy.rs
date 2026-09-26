use std::time::Duration;

pub const FAST_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub const SLOW_REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);
pub const SELECTED_PROVIDER_REFRESH_AFTER: Duration = Duration::from_secs(5 * 60);
pub const FAILURE_RETRY_BACKOFF: Duration = Duration::from_secs(60);

pub fn refresh_interval_for_provider(
    enabled: bool,
    has_native_instance: bool,
    notifications_enabled: bool,
) -> Option<Duration> {
    if !enabled {
        None
    } else if has_native_instance || notifications_enabled {
        Some(FAST_REFRESH_INTERVAL)
    } else {
        Some(SLOW_REFRESH_INTERVAL)
    }
}

pub fn stale_after(refresh_interval: Duration) -> chrono::Duration {
    chrono::Duration::from_std(refresh_interval.saturating_mul(2))
        .expect("refresh interval fits chrono duration")
}

pub fn selected_provider_is_due(last_attempt_age: Option<Duration>) -> bool {
    last_attempt_age.is_none_or(|age| age >= SELECTED_PROVIDER_REFRESH_AFTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_intervals_follow_enabled_instance_and_notification_state() {
        assert_eq!(refresh_interval_for_provider(false, true, true), None);
        assert_eq!(
            refresh_interval_for_provider(true, true, false),
            Some(FAST_REFRESH_INTERVAL)
        );
        assert_eq!(
            refresh_interval_for_provider(true, false, true),
            Some(FAST_REFRESH_INTERVAL)
        );
        assert_eq!(
            refresh_interval_for_provider(true, false, false),
            Some(SLOW_REFRESH_INTERVAL)
        );
    }

    #[test]
    fn stale_threshold_is_two_times_the_provider_interval() {
        assert_eq!(
            stale_after(FAST_REFRESH_INTERVAL),
            chrono::Duration::minutes(10)
        );
        assert_eq!(
            stale_after(SLOW_REFRESH_INTERVAL),
            chrono::Duration::minutes(30)
        );
    }

    #[test]
    fn selected_background_provider_refreshes_after_five_minutes_only() {
        assert!(selected_provider_is_due(None));
        assert!(!selected_provider_is_due(Some(Duration::from_secs(299))));
        assert!(selected_provider_is_due(Some(Duration::from_secs(300))));
        assert!(selected_provider_is_due(Some(Duration::from_secs(301))));
    }
}
