use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::models::WindowMode;

#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxSessionType {
    X11,
    Wayland,
    Unknown,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDesktop {
    Gnome,
    Kde,
    Other,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeEntryOutcome {
    MenuEntry,
    FloatingWindow,
    Exit,
}

#[derive(Debug, Clone)]
pub struct DesktopIntegration {
    tray_available: Arc<AtomicBool>,
    floating_window: Arc<AtomicBool>,
    platform_label: Option<String>,
}

impl DesktopIntegration {
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            let session = parse_session_type(std::env::var("XDG_SESSION_TYPE").ok().as_deref());
            let desktop = parse_desktop(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());
            let tray_available = status_notifier_host_available();
            linux_integration(session, desktop, tray_available)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self {
                tray_available: Arc::new(AtomicBool::new(true)),
                floating_window: Arc::new(AtomicBool::new(false)),
                platform_label: None,
            }
        }
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::SeqCst)
    }

    pub fn is_floating(&self) -> bool {
        self.floating_window.load(Ordering::SeqCst)
    }

    pub fn exits_on_close(&self) -> bool {
        self.is_floating() && !self.tray_available()
    }

    pub fn apply_window_mode(&self, mode: WindowMode) -> bool {
        let floating = !self.tray_available() || mode == WindowMode::Floating;
        self.set_floating(floating);
        floating
    }

    #[cfg(any(not(target_os = "macos"), test))]
    pub fn disable_tray(&self) -> bool {
        let changed = self.tray_available.swap(false, Ordering::SeqCst);
        self.set_floating(true);
        changed
    }

    pub fn set_menu_entry_available(&self, available: bool) {
        self.tray_available.store(available, Ordering::SeqCst);
        if !available {
            self.set_floating(true);
        }
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn ensure_runtime_entry(
        &self,
        has_menu_entry: bool,
        floating_window_visible: bool,
        show_floating_window: impl FnOnce() -> bool,
    ) -> RuntimeEntryOutcome {
        self.set_menu_entry_available(has_menu_entry);
        if has_menu_entry {
            RuntimeEntryOutcome::MenuEntry
        } else if floating_window_visible || show_floating_window() {
            RuntimeEntryOutcome::FloatingWindow
        } else {
            RuntimeEntryOutcome::Exit
        }
    }

    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn ensure_runtime_entry_or_exit(
        &self,
        has_menu_entry: bool,
        floating_window_visible: bool,
        show_floating_window: impl FnOnce() -> bool,
        exit: impl FnOnce(),
    ) -> RuntimeEntryOutcome {
        let outcome = self.ensure_runtime_entry(
            has_menu_entry,
            floating_window_visible,
            show_floating_window,
        );
        if outcome == RuntimeEntryOutcome::Exit {
            exit();
        }
        outcome
    }

    pub(crate) fn set_floating(&self, floating: bool) {
        self.floating_window.store(floating, Ordering::SeqCst);
    }

    pub fn platform_summary(&self) -> Option<String> {
        self.platform_label.as_ref().map(|label| {
            let mode = if self.tray_available() {
                "StatusNotifier tray"
            } else {
                "standalone window"
            };
            format!("{label} · {mode}")
        })
    }
}

#[cfg(any(target_os = "linux", test))]
fn linux_integration(
    session: LinuxSessionType,
    desktop: LinuxDesktop,
    tray_available: bool,
) -> DesktopIntegration {
    let desktop = match desktop {
        LinuxDesktop::Gnome => "GNOME",
        LinuxDesktop::Kde => "KDE Plasma",
        LinuxDesktop::Other => "Linux desktop",
    };
    let session = match session {
        LinuxSessionType::X11 => "X11",
        LinuxSessionType::Wayland => "Wayland",
        LinuxSessionType::Unknown => "unknown session",
    };
    DesktopIntegration {
        tray_available: Arc::new(AtomicBool::new(tray_available)),
        floating_window: Arc::new(AtomicBool::new(!tray_available)),
        platform_label: Some(format!("{desktop} · {session}")),
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn parse_desktop(value: Option<&str>) -> LinuxDesktop {
    let names = value
        .unwrap_or_default()
        .split(':')
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if names.iter().any(|name| name.contains("gnome")) {
        LinuxDesktop::Gnome
    } else if names
        .iter()
        .any(|name| name.contains("kde") || name.contains("plasma"))
    {
        LinuxDesktop::Kde
    } else {
        LinuxDesktop::Other
    }
}

#[cfg(any(target_os = "linux", test))]
pub fn parse_session_type(value: Option<&str>) -> LinuxSessionType {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("x11") => LinuxSessionType::X11,
        Some("wayland") => LinuxSessionType::Wayland,
        _ => LinuxSessionType::Unknown,
    }
}

#[cfg(target_os = "linux")]
fn status_notifier_host_available() -> bool {
    match std::env::var("QUOTA01_LINUX_TRAY_HOST").as_deref() {
        Ok("available") => return true,
        Ok("unavailable") => return false,
        _ => {}
    }
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&connection) else {
        return false;
    };
    proxy.list_names().is_ok_and(|names| {
        names
            .iter()
            .any(|name| name.as_str() == "org.kde.StatusNotifierWatcher")
    })
}

#[cfg(target_os = "linux")]
pub fn status_notifier_monitor_forced_off() -> bool {
    matches!(
        std::env::var("QUOTA01_LINUX_TRAY_HOST").as_deref(),
        Ok("available" | "unavailable")
    )
}

#[cfg(target_os = "linux")]
pub fn wait_for_status_notifier_loss() -> Result<(), String> {
    const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";

    let connection = zbus::blocking::Connection::session()
        .map_err(|error| format!("session bus unavailable: {error}"))?;
    let proxy = zbus::blocking::fdo::DBusProxy::new(&connection)
        .map_err(|error| format!("session bus proxy unavailable: {error}"))?;
    let changes = proxy
        .receive_name_owner_changed_with_args(&[(0, WATCHER_NAME)])
        .map_err(|error| format!("watcher subscription failed: {error}"))?;

    let available = proxy
        .list_names()
        .map_err(|error| format!("watcher snapshot failed: {error}"))?
        .iter()
        .any(|name| name.as_str() == WATCHER_NAME);
    if !available {
        return Ok(());
    }

    for change in changes {
        let arguments = change
            .args()
            .map_err(|error| format!("watcher signal invalid: {error}"))?;
        if arguments.name().as_str() == WATCHER_NAME && arguments.new_owner().as_ref().is_none() {
            return Ok(());
        }
    }
    Err("watcher signal stream ended".to_owned())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{
        parse_desktop, parse_session_type, LinuxDesktop, LinuxSessionType, RuntimeEntryOutcome,
    };
    use crate::models::WindowMode;

    #[test]
    fn recognizes_x11_and_wayland_sessions_case_insensitively() {
        assert_eq!(parse_session_type(Some("x11")), LinuxSessionType::X11);
        assert_eq!(
            parse_session_type(Some(" Wayland ")),
            LinuxSessionType::Wayland
        );
        assert_eq!(parse_session_type(Some("tty")), LinuxSessionType::Unknown);
        assert_eq!(parse_session_type(None), LinuxSessionType::Unknown);
    }

    #[test]
    fn recognizes_gnome_and_kde_desktop_name_lists() {
        assert_eq!(parse_desktop(Some("ubuntu:GNOME")), LinuxDesktop::Gnome);
        assert_eq!(parse_desktop(Some("KDE")), LinuxDesktop::Kde);
        assert_eq!(parse_desktop(Some("plasma:wayland")), LinuxDesktop::Kde);
        assert_eq!(parse_desktop(Some("sway")), LinuxDesktop::Other);
    }

    #[test]
    fn platform_summary_explains_the_linux_fallback_mode() {
        let integration =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Gnome, false);
        assert_eq!(
            integration.platform_summary().as_deref(),
            Some("GNOME · Wayland · standalone window")
        );
    }

    #[test]
    fn window_mode_is_independent_from_tray_availability() {
        let with_tray =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        assert!(with_tray.tray_available());
        assert!(!with_tray.apply_window_mode(WindowMode::Popup));
        assert!(with_tray.apply_window_mode(WindowMode::Floating));
        assert!(!with_tray.exits_on_close());

        let without_tray =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Gnome, false);
        assert!(!without_tray.tray_available());
        assert!(without_tray.apply_window_mode(WindowMode::Popup));
        assert!(without_tray.exits_on_close());
    }

    #[test]
    fn losing_the_tray_permanently_falls_back_to_a_visible_window_mode() {
        let integration = super::linux_integration(LinuxSessionType::X11, LinuxDesktop::Kde, true);
        assert!(!integration.apply_window_mode(WindowMode::Popup));

        assert!(integration.disable_tray());
        assert!(!integration.tray_available());
        assert!(integration.is_floating());
        assert!(integration.exits_on_close());
        assert_eq!(
            integration.platform_summary().as_deref(),
            Some("KDE Plasma · X11 · standalone window")
        );
        assert!(!integration.disable_tray());
    }

    #[test]
    fn losing_the_menu_entry_makes_a_floating_window_exit_on_close() {
        let integration =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        assert!(integration.apply_window_mode(WindowMode::Floating));
        integration.set_menu_entry_available(false);
        assert!(integration.is_floating());
        assert!(integration.exits_on_close());

        integration.set_menu_entry_available(true);
        assert!(!integration.exits_on_close());
    }
    #[test]
    fn runtime_entry_falls_back_to_a_floating_window_when_no_menu_item_remains() {
        let integration =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        let fallback_calls = Cell::new(0);

        let outcome = integration.ensure_runtime_entry(false, false, || {
            fallback_calls.set(fallback_calls.get() + 1);
            true
        });

        assert_eq!(outcome, RuntimeEntryOutcome::FloatingWindow);
        assert_eq!(fallback_calls.get(), 1);
        assert!(!integration.tray_available());
        assert!(integration.is_floating());
        assert!(integration.exits_on_close());
    }

    #[test]
    fn runtime_entry_exits_when_no_menu_item_or_floating_window_can_be_shown() {
        let integration =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);

        let outcome = integration.ensure_runtime_entry(false, false, || false);

        assert_eq!(outcome, RuntimeEntryOutcome::Exit);
        assert!(!integration.tray_available());
        assert!(integration.is_floating());
        assert!(integration.exits_on_close());
    }

    #[test]
    fn runtime_entry_recovery_only_exits_after_the_floating_fallback_fails() {
        let recovered =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        let recovered_exit_calls = Cell::new(0);

        let outcome = recovered.ensure_runtime_entry_or_exit(
            false,
            false,
            || true,
            || recovered_exit_calls.set(recovered_exit_calls.get() + 1),
        );

        assert_eq!(outcome, RuntimeEntryOutcome::FloatingWindow);
        assert_eq!(recovered_exit_calls.get(), 0);
        assert!(!recovered.tray_available());
        assert!(recovered.is_floating());

        let unrecoverable =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        let unrecoverable_exit_calls = Cell::new(0);

        let outcome = unrecoverable.ensure_runtime_entry_or_exit(
            false,
            false,
            || false,
            || unrecoverable_exit_calls.set(unrecoverable_exit_calls.get() + 1),
        );

        assert_eq!(outcome, RuntimeEntryOutcome::Exit);
        assert_eq!(unrecoverable_exit_calls.get(), 1);
        assert!(!unrecoverable.tray_available());
        assert!(unrecoverable.is_floating());
    }

    #[test]
    fn runtime_entry_keeps_an_existing_visible_floating_window_without_reapplying() {
        let integration =
            super::linux_integration(LinuxSessionType::Wayland, LinuxDesktop::Kde, true);
        assert!(integration.apply_window_mode(WindowMode::Floating));
        let fallback_calls = Cell::new(0);

        let outcome = integration.ensure_runtime_entry(false, true, || {
            fallback_calls.set(fallback_calls.get() + 1);
            false
        });

        assert_eq!(outcome, RuntimeEntryOutcome::FloatingWindow);
        assert_eq!(fallback_calls.get(), 0);
        assert!(!integration.tray_available());
        assert!(integration.is_floating());
    }
}
