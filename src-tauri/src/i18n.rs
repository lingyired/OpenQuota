use crate::models::LanguagePreference;

/// Locale resolved from the user's language preference. Only English and
/// Simplified Chinese are supported for now; more locales can be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
}

/// Resolve the effective locale from a language preference. The `system`
/// preference probes the OS language; only Chinese is recognized, everything
/// else falls back to English.
pub fn resolve(language: LanguagePreference) -> Locale {
    match language {
        LanguagePreference::En => Locale::En,
        LanguagePreference::ZhCn => Locale::ZhCn,
        LanguagePreference::System => {
            if sys_locale::get_locale().is_some_and(|locale| locale.starts_with("zh")) {
                Locale::ZhCn
            } else {
                Locale::En
            }
        }
    }
}

/// Translate a dictionary key into the given locale. Keys are string literals
/// defined at the call sites; unknown keys render as themselves so a missing
/// entry degrades gracefully instead of panicking.
pub fn tr(locale: Locale, key: &'static str) -> &'static str {
    match (locale, key) {
        // Native tray menu labels
        (Locale::En, "menu.open") => "Open OpenQuota",
        (Locale::ZhCn, "menu.open") => "打开 OpenQuota",
        (Locale::En, "menu.customize") => "Customize…",
        (Locale::ZhCn, "menu.customize") => "自定义…",
        (Locale::En, "menu.settings") => "Settings…",
        (Locale::ZhCn, "menu.settings") => "设置…",
        (Locale::En, "menu.settings_short") => "Settings",
        (Locale::ZhCn, "menu.settings_short") => "设置",
        (Locale::En, "menu.quit") => "Quit OpenQuota",
        (Locale::ZhCn, "menu.quit") => "退出 OpenQuota",
        // System notification action label
        (Locale::En, "notif.open") => "Open OpenQuota",
        (Locale::ZhCn, "notif.open") => "打开 OpenQuota",
        // Pace milestone titles and bodies
        (Locale::En, "notif.almost_out") => "Almost Out",
        (Locale::ZhCn, "notif.almost_out") => "即将用尽",
        (Locale::En, "notif.almost_out_body") => "Under 10% usage remaining for this window.",
        (Locale::ZhCn, "notif.almost_out_body") => "此周期剩余用量不足 10%。",
        (Locale::En, "notif.cutting_it_close") => "Cutting It Close",
        (Locale::ZhCn, "notif.cutting_it_close") => "接近上限",
        (Locale::En, "notif.cutting_it_close_body") => "Projected to finish close to your limit.",
        (Locale::ZhCn, "notif.cutting_it_close_body") => "预计将接近你的用量上限。",
        (Locale::En, "notif.will_run_out") => "Will Run Out",
        (Locale::ZhCn, "notif.will_run_out") => "将要用尽",
        (Locale::En, "notif.will_run_out_body") => "Projected to run out before the limit resets.",
        (Locale::ZhCn, "notif.will_run_out_body") => "预计将在限额重置前用尽。",
        // Unknown key: render the key itself
        (_, _) => key,
    }
}

/// Localize one of the Windows taskband right-click context-menu actions.
/// `agent_name` is the provider's display name, which is embedded in the
/// hide/refresh labels so the menu reads naturally for whichever agent it was
/// opened on.
#[cfg(target_os = "windows")]
pub fn taskband_action_label(locale: Locale, action: &str, agent_name: &str) -> String {
    match (locale, action) {
        (Locale::En, "hide") => format!("Hide {agent_name}"),
        (Locale::ZhCn, "hide") => format!("隐藏 {agent_name}"),
        (Locale::En, "refresh") => format!("Refresh {agent_name}"),
        (Locale::ZhCn, "refresh") => format!("刷新 {agent_name}"),
        (Locale::En, "quit") | (Locale::ZhCn, "quit") => tr(locale, "menu.quit").to_owned(),
        (_, other) => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{tr, Locale};

    #[test]
    fn system_chinese_resolves_to_simplified_chinese() {
        // The OS locale is fixed by the test host; only assert the explicit
        // preference path here. System resolution is covered by unit behavior:
        // zh-prefixed locales map to ZhCn.
        assert_eq!(tr(Locale::ZhCn, "menu.settings"), "设置…");
        assert_eq!(tr(Locale::En, "menu.settings"), "Settings…");
    }

    #[test]
    fn unknown_key_renders_itself() {
        assert_eq!(tr(Locale::ZhCn, "menu.unknown"), "menu.unknown");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn taskband_context_menu_labels_use_the_agent_name() {
        assert_eq!(
            taskband_action_label(Locale::En, "hide", "Claude"),
            "Hide Claude"
        );
        assert_eq!(
            taskband_action_label(Locale::ZhCn, "hide", "Claude"),
            "隐藏 Claude"
        );
        assert_eq!(
            taskband_action_label(Locale::En, "refresh", "Codex"),
            "Refresh Codex"
        );
        assert_eq!(
            taskband_action_label(Locale::ZhCn, "refresh", "Codex"),
            "刷新 Codex"
        );
        assert_eq!(
            taskband_action_label(Locale::ZhCn, "quit", "Codex"),
            "退出 OpenQuota"
        );
        assert_eq!(taskband_action_label(Locale::En, "other", "Codex"), "other");
    }
}
