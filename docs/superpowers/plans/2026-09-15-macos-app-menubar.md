# macOS App Menubar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the macOS Tauri system tray with an optional multiline-menubar Quota01 app item while guaranteeing that macOS always has at least one usable app entry unless a visible floating window is intentionally left open.

**Architecture:** `menubar::update` becomes the only macOS menu-bar reconciler. It computes provider instances plus one synthetic `quota01-app` instance, persists an app-icon visibility preference, and coordinates removal behavior with `DesktopIntegration`.

**Tech Stack:** Rust, Tauri 2, Objective-C++ menubar plugin, Svelte 5, TypeScript, Vitest, Cargo tests.

**Spec:** `docs/superpowers/specs/2026-09-15-macos-app-menubar-design.md`

## Global Constraints

- macOS must not create a Tauri `TrayIcon`; Windows and Linux behavior must remain unchanged.
- The synthetic app instance id is exactly `quota01-app`.
- The setting is exactly `show_app_menubar` in Rust and `showAppMenubar` in TypeScript.
- `showAppMenubar` defaults to `true`; persisted schema version becomes `9`.
- A visible provider instance means: provider enabled, provider definition exists, that provider's menu-bar layout is enabled, and at least one pinned metric is renderable.
- `app_visible = show_app_menubar || (no_visible_provider_instances && !allow_no_menubar)`.
- Dragging a provider item out persists that provider's menu-bar layout as disabled.
- Dragging the app item out persists `show_app_menubar = false`.
- The last app item may stay removed only while a floating main window is visible; closing that window then exits the app.
- Never leave macOS running with neither a menubar entry nor a visible window.
- Follow TDD: write and run a failing test before each implementation change, then commit each task.

---

### Task 1: Pure Menubar Reconciliation Plan

**Files:**
- Modify: `src-tauri/src/menubar.rs`
- Test: `src-tauri/src/menubar.rs` (`mod tests`)

**Interfaces:**
- Consumes: `UsageViewState`, `AppSettings`, `ProviderRegistry`, and `pinned_provider_metrics`.
- Produces:
  - `const APP_MENUBAR_INSTANCE_ID: &str = "quota01-app"`
  - `struct DesiredProviderMenubar { instance_id: String, provider_id: String, provider_name: String, config: AppliedConfig }`
  - `struct MenubarPlan { provider_instances: Vec<DesiredProviderMenubar>, app_instance_visible: bool, app_forced: bool }`
  - `fn desired_provider_menubars(state: &UsageViewState, settings: &AppSettings, registry: &ProviderRegistry) -> Vec<DesiredProviderMenubar>`
  - `fn plan_menubar(provider_instances: Vec<DesiredProviderMenubar>, show_app_menubar: bool, allow_no_menubar: bool) -> MenubarPlan`
  - `enum AppRemovalAction { HideOnly, KeepWindowThenExit, ExitNow }`
  - `fn app_removal_action(provider_instances_empty: bool, floating_window_visible: bool) -> AppRemovalAction`

- [ ] **Step 1: Add failing tests for the visibility matrix**

Add these tests to `src-tauri/src/menubar.rs` inside `mod tests`:

```rust
#[test]
fn app_instance_is_visible_when_requested_or_forced() {
    let with_provider = || vec![provider_menubar("codex")];

    let requested = plan_menubar(with_provider(), true, false);
    assert!(requested.app_instance_visible);
    assert!(!requested.app_forced);

    let hidden = plan_menubar(with_provider(), false, false);
    assert!(!hidden.app_instance_visible);
    assert!(!hidden.app_forced);

    let forced = plan_menubar(Vec::new(), false, false);
    assert!(forced.app_instance_visible);
    assert!(forced.app_forced);

    let floating_exception = plan_menubar(Vec::new(), false, true);
    assert!(!floating_exception.app_instance_visible);
    assert!(!floating_exception.app_forced);
}

#[test]
fn app_removal_action_matches_window_mode_and_visibility() {
    assert_eq!(
        app_removal_action(false, false),
        AppRemovalAction::HideOnly
    );
    assert_eq!(
        app_removal_action(false, true),
        AppRemovalAction::HideOnly
    );
    assert_eq!(
        app_removal_action(true, true),
        AppRemovalAction::KeepWindowThenExit
    );
    assert_eq!(
        app_removal_action(true, false),
        AppRemovalAction::ExitNow
    );
}
```

Add a test-only helper near the existing metric helpers:

```rust
fn provider_menubar(provider_id: &str) -> DesiredProviderMenubar {
    DesiredProviderMenubar {
        instance_id: provider_id.to_owned(),
        provider_id: provider_id.to_owned(),
        provider_name: provider_id.to_owned(),
        config: instance_config(
            MenubarConfigInput {
                text: ("75%".into(), String::new()),
                lines_visible: (true, false),
                leading_icon: Some("svg"),
                tooltip: provider_id.to_owned(),
            },
            &crate::models::TaskbandLayout::default(),
            true,
        ),
    }
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib menubar::tests::app_
```

Expected: compilation fails because `plan_menubar`, `app_removal_action`, `DesiredProviderMenubar`, and `AppRemovalAction` do not exist.

- [ ] **Step 3: Add the plan data types and pure functions**

Add above `impl MenubarState`:

```rust
#[cfg(target_os = "macos")]
pub(crate) const APP_MENUBAR_INSTANCE_ID: &str = "quota01-app";

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct DesiredProviderMenubar {
    instance_id: String,
    provider_id: String,
    provider_name: String,
    config: AppliedConfig,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
struct MenubarPlan {
    provider_instances: Vec<DesiredProviderMenubar>,
    app_instance_visible: bool,
    app_forced: bool,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppRemovalAction {
    HideOnly,
    KeepWindowThenExit,
    ExitNow,
}

#[cfg(any(target_os = "macos", test))]
fn plan_menubar(
    provider_instances: Vec<DesiredProviderMenubar>,
    show_app_menubar: bool,
    allow_no_menubar: bool,
) -> MenubarPlan {
    let provider_instances_empty = provider_instances.is_empty();
    let app_forced =
        provider_instances_empty && !show_app_menubar && !allow_no_menubar;
    MenubarPlan {
        provider_instances,
        app_instance_visible: show_app_menubar || app_forced,
        app_forced,
    }
}

#[cfg(any(target_os = "macos", test))]
fn app_removal_action(
    provider_instances_empty: bool,
    floating_window_visible: bool,
) -> AppRemovalAction {
    match (provider_instances_empty, floating_window_visible) {
        (false, _) => AppRemovalAction::HideOnly,
        (true, true) => AppRemovalAction::KeepWindowThenExit,
        (true, false) => AppRemovalAction::ExitNow,
    }
}
```

- [ ] **Step 4: Extract provider instance resolution from `update`**

Move the body of the provider loop in `menubar::update` into:

```rust
#[cfg(target_os = "macos")]
fn desired_provider_menubars(
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
) -> Vec<DesiredProviderMenubar> {
    let mut desired = Vec::new();
    for provider in settings.providers.iter().filter(|provider| provider.enabled) {
        let Some(definition) = registry.definition(&provider.id) else {
            continue;
        };
        let layout = settings
            .taskband_providers
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        if !layout.enabled {
            continue;
        }
        let metrics = pinned_provider_metrics(state, provider, settings, registry);
        if metrics.is_empty() {
            continue;
        }
        let (top_value, bottom_value, bottom_visible) = metric_lines(&metrics);
        let provider_name = settings.provider_display_name(definition).to_owned();
        let config = instance_config(
            MenubarConfigInput {
                text: (top_value, bottom_value),
                lines_visible: (true, bottom_visible),
                leading_icon: provider_icon_svg(&provider.id),
                tooltip: provider_name.clone(),
            },
            &layout,
            true,
        );
        desired.push(DesiredProviderMenubar {
            instance_id: sanitize_instance_id(&provider.id),
            provider_id: provider.id.clone(),
            provider_name,
            config,
        });
    }
    desired
}
```

- [ ] **Step 5: Run the focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib menubar::tests::app_
```

Expected: both tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/menubar.rs
git commit -m "refactor: model macOS menubar reconciliation"
```

---

### Task 2: Settings and View Contracts

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/test/appFixtures.ts`
- Modify: `src/lib/settingsController.test.ts`
- Modify: `src/lib/CustomizeProviderDetail.session.test.ts`
- Modify: `src/lib/reorderComponents.test.ts`
- Test: `src-tauri/src/models.rs`, `src-tauri/src/settings.rs`

**Interfaces:**
- Consumes: `MenubarPlan` and provider resolution from Task 1.
- Produces:
  - `AppSettings.show_app_menubar: bool`
  - `SettingsViewState.app_menubar_forced: bool`
  - TypeScript `AppSettings.showAppMenubar: boolean`
  - TypeScript `SettingsViewState.appMenubarForced: boolean`
  - `SettingsService::view_state(..., app_menubar_forced: bool)`

- [ ] **Step 1: Write failing Rust settings tests**

Add to `src-tauri/src/models.rs` tests:

```rust
#[test]
fn app_menubar_defaults_to_visible() {
    assert!(AppSettings::default().show_app_menubar);
}

#[test]
fn older_settings_default_the_app_menubar_to_visible() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("showAppMenubar");
    let settings: AppSettings = serde_json::from_value(value).unwrap();
    assert!(settings.show_app_menubar);
}
```

Add to `src-tauri/src/settings.rs`:

```rust
#[test]
fn normalization_marks_schema_nine() {
    let catalog = ProviderRegistry::from_definitions(Vec::new()).unwrap();
    let mut settings = AppSettings::default();
    normalize(&catalog, &mut settings, &HashSet::new());
    assert_eq!(settings.schema_version, 9);
}
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib app_menubar_defaults_to_visible
cargo test --manifest-path src-tauri/Cargo.toml --lib older_settings_default_the_app_menubar_to_visible
cargo test --manifest-path src-tauri/Cargo.toml --lib normalization_marks_schema_nine
```

Expected: compilation fails because `show_app_menubar` does not exist.

- [ ] **Step 3: Add the Rust fields and defaults**

In `AppSettings`:

```rust
pub show_app_menubar: bool,
```

In `AppSettings::default()`:

```rust
show_app_menubar: true,
schema_version: 9,
```

In `normalize_with_persisted_accounts`:

```rust
settings.schema_version = 9;
```

Update the existing schema migration assertion in `src-tauri/src/settings.rs` from `8` to `9`.

In `SettingsViewState`:

```rust
pub app_menubar_forced: bool,
```

- [ ] **Step 4: Wire forced-state calculation**

Add to `menubar.rs`:

```rust
#[cfg(target_os = "macos")]
pub(crate) fn app_menubar_forced(
    state: &UsageViewState,
    settings: &AppSettings,
    registry: &ProviderRegistry,
    allow_no_menubar: bool,
) -> bool {
    plan_menubar(
        desired_provider_menubars(state, settings, registry),
        settings.show_app_menubar,
        allow_no_menubar,
    )
    .app_forced
}
```

Change `SettingsService::view_state` to accept the value and populate it:

```rust
pub fn view_state(
    &self,
    notification_permission: impl Into<String>,
    integration_error: Option<String>,
    tray_available: bool,
    platform_summary: Option<String>,
    app_menubar_forced: bool,
) -> SettingsViewState
```

Add `app_menubar_forced,` to the `SettingsViewState` literal after `platform_summary`.

In `commands/settings.rs::settings_view_state`, compute:

```rust
#[cfg(target_os = "macos")]
let app_menubar_forced = crate::menubar::app_menubar_forced(
    &app.state::<Arc<ProviderService>>().state(),
    &service.get(),
    service.registry(),
    false,
);
#[cfg(not(target_os = "macos"))]
let app_menubar_forced = false;
```

Pass it as the new last argument to `service.view_state(...)`. Update all test call sites with `false`. Task 4 replaces the temporary `false` runtime exception argument with `MenubarState::allows_no_menubar()`.

Replace the direct `settings.view_state(...)` call in `request_notification_permission` with `settings_view_state(&app, &settings)` so it uses the same forced-state calculation.

- [ ] **Step 5: Update TypeScript contracts and fixtures**

In `src/lib/types.ts`:

```ts
showAppMenubar: boolean;
```

inside `AppSettings`, and:

```ts
appMenubarForced: boolean;
```

inside `SettingsViewState`.

Update every fixture in the listed test files:

```ts
showAppMenubar: true,
appMenubarForced: false,
schemaVersion: 9,
```

- [ ] **Step 6: Run focused Rust and frontend tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib app_menubar_defaults_to_visible
cargo test --manifest-path src-tauri/Cargo.toml --lib older_settings_default_the_app_menubar_to_visible
cargo test --manifest-path src-tauri/Cargo.toml --lib normalization_marks_schema_nine
corepack pnpm exec vitest run src/lib/settingsController.test.ts src/lib/CustomizeProviderDetail.session.test.ts src/lib/reorderComponents.test.ts
corepack pnpm check
```

Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/settings.rs src-tauri/src/commands/settings.rs src-tauri/src/menubar.rs src/lib/types.ts src/test/appFixtures.ts src/lib/settingsController.test.ts src/lib/CustomizeProviderDetail.session.test.ts src/lib/reorderComponents.test.ts
git commit -m "feat: add app menubar visibility setting"
```

---

### Task 3: Synthetic App Item, Click, and Menu

**Files:**
- Modify: `src-tauri/src/menubar.rs`
- Modify: `src-tauri/src/window.rs`
- Test: `src-tauri/src/menubar.rs`, `src-tauri/src/window.rs`

**Interfaces:**
- Consumes: `APP_MENUBAR_INSTANCE_ID`, `MenubarPlan`, `AppliedConfig`, `MultilineMenubarExt`.
- Produces:
  - `fn app_instance_config(visible: bool) -> AppliedConfig`
  - `fn app_context_menu_items(locale: Locale) -> (Vec<MenuItemDescriptor>, String)`
  - `MenubarState::apply_instance(...) -> Result<(), String>`
  - `MenubarState::register_app_click_listener(app)`
  - `MenubarState::register_app_context_menu(app, locale)`
  - `window::toggle_main_window_below_menu_bar_item(window, anchor)`

- [ ] **Step 1: Write failing tests for the app config and menu**

Add:

```rust
#[cfg(target_os = "macos")]
#[test]
fn app_instance_is_icon_only_and_uses_the_app_mark() {
    let config = app_instance_config(true);
    assert_eq!(config.text, (String::new(), String::new()));
    assert_eq!(config.lines_visible, (false, false));
    assert!(config.leading_icon.is_some());
    assert_eq!(config.tooltip, "Quota01");
}

#[cfg(target_os = "macos")]
#[test]
fn app_menu_contains_settings_and_quit() {
    let (items, signature) = app_context_menu_items(crate::i18n::Locale::En);
    let ids = items
        .iter()
        .filter_map(|item| match item {
            MenuItemDescriptor::Item { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(ids.contains(&"quota01-app::settings"));
    assert!(ids.contains(&"quota01-app::quit"));
    assert!(signature.contains("Settings"));
    assert!(signature.contains("Quit Quota01"));
}
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib app_instance_is_icon_only_and_uses_the_app_mark
cargo test --manifest-path src-tauri/Cargo.toml --lib app_menu_contains_settings_and_quit
```

Expected: compilation fails because the functions do not exist.

- [ ] **Step 3: Implement the app instance and menu**

Use the existing bundled tray mark in these macOS-only definitions:

```rust
#[cfg(target_os = "macos")]
const QUOTA01_MENUBAR_ICON: &str = include_str!("../../assets/quota01-tray.svg");

#[cfg(target_os = "macos")]
fn app_instance_config(visible: bool) -> AppliedConfig {
    instance_config(
        MenubarConfigInput {
            text: (String::new(), String::new()),
            lines_visible: (false, false),
            leading_icon: Some(QUOTA01_MENUBAR_ICON),
            tooltip: "Quota01".to_owned(),
        },
        &TaskbandLayout::default(),
        visible,
    )
}

#[cfg(target_os = "macos")]
fn app_context_menu_items(
    locale: crate::i18n::Locale,
) -> (Vec<MenuItemDescriptor>, String) {
    let item = |action: &str, text: String| MenuItemDescriptor::Item {
        id: format!("{APP_MENUBAR_INSTANCE_ID}::{action}"),
        text,
        accelerator: None,
        enabled: Some(true),
        disabled: None,
    };
    let settings = crate::i18n::tr(locale, "menu.settings").to_owned();
    let quit = crate::i18n::tr(locale, "menu.quit").to_owned();
    let items = vec![
        item(MENU_ACTION_SETTINGS, settings.clone()),
        MenuItemDescriptor::Separator,
        item(MENU_ACTION_QUIT, quit.clone()),
    ];
    let signature = format!("{settings}\u{1}{quit}");
    (items, signature)
}
```

Add `register_app_click_listener` and `register_app_context_menu` to `MenubarState`. The click listener listens to:

```rust
format!("multiline-menubar://{APP_MENUBAR_INSTANCE_ID}//click")
```

For a left click, call `toggle_main_window_below_menu_bar_item`, falling back to `toggle_main_window` when the payload has no valid anchor.

The menu listener listens to:

```rust
format!("multiline-menubar://{APP_MENUBAR_INSTANCE_ID}//menu")
```

For `settings`, call `open_screen(app, "settings")`. For `quit`, reuse the existing delayed native-menu quit pattern.

Change `apply_instance` from discarding plugin errors to returning `Result<(), String>`:

```rust
fn apply_instance(&self, app: &AppHandle, id: &str, config: AppliedConfig) -> Result<(), String> {
    let mb = app.multiline_menubar();
    let result = (|| -> Result<(), String> {
        mb.create(id.to_string()).map_err(|error| error.to_string())?;
        mb.set_text(id.to_string(), config.text.0.clone(), config.text.1.clone())
            .map_err(|error| error.to_string())?;
        mb.set_line_visible(
            id.to_string(),
            config.lines_visible.0,
            config.lines_visible.1,
        )
        .map_err(|error| error.to_string())?;
        mb.set_leading_icon(id.to_string(), to_icon(config.leading_icon))
            .map_err(|error| error.to_string())?;
        mb.set_colors(
            id.to_string(),
            to_plugin_color(&config.top_color, id),
            to_plugin_color(&config.bottom_color, ""),
        )
        .map_err(|error| error.to_string())?;
        mb.set_bold(id.to_string(), config.top_bold, config.bottom_bold)
            .map_err(|error| error.to_string())?;
        mb.set_font_sizes(id.to_string(), config.top_size, config.bottom_size)
            .map_err(|error| error.to_string())?;
        mb.set_alignment(id.to_string(), config.top_align, config.bottom_align)
            .map_err(|error| error.to_string())?;
        mb.set_tooltip(id.to_string(), config.tooltip.clone())
            .map_err(|error| error.to_string())?;
        mb.set_visible(id.to_string(), config.visible)
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_ok() {
        self.created
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.to_string(), config);
    }
    result
}
```

Apply the same `map_err` conversion inside the existing `Some(previous)` branch. Provider reconciliation logs and continues if one provider instance fails; the app-instance failure path is completed in Task 4.

- [ ] **Step 4: Add the window toggle helper**

In `window.rs`:

```rust
#[cfg(target_os = "macos")]
pub fn toggle_main_window_below_menu_bar_item(
    window: &WebviewWindow,
    anchor: MenuBarAnchor,
) {
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if visible && !minimized {
        hide_main_window(window);
    } else {
        show_main_window_below_menu_bar_item(window, anchor);
    }
}
```

- [ ] **Step 5: Create the app item during reconciliation**

In `menubar::update`, replace the provider loop with `plan_menubar(desired_provider_menubars(...), settings.show_app_menubar, menubar.allows_no_menubar())`.

Apply provider instances from `plan.provider_instances`, then:

```rust
if plan.app_instance_visible {
    menubar.apply_instance(app, APP_MENUBAR_INSTANCE_ID, app_instance_config(true));
    menubar.register_app_click_listener(app);
    menubar.register_app_context_menu(app, locale);
    desired_ids.insert(APP_MENUBAR_INSTANCE_ID.to_owned());
}
```

- [ ] **Step 6: Run focused tests and compile**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib app_instance_is_icon_only_and_uses_the_app_mark
cargo test --manifest-path src-tauri/Cargo.toml --lib app_menu_contains_settings_and_quit
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: tests pass and the Tauri crate compiles.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/menubar.rs src-tauri/src/window.rs
git commit -m "feat: render the Quota01 app menubar item"
```

---

### Task 4: Removal Semantics and Runtime Entry Availability

**Files:**
- Modify: `src-tauri/src/menubar.rs`
- Modify: `src-tauri/src/desktop_integration.rs`
- Modify: `src-tauri/src/commands/settings.rs`
- Test: `src-tauri/src/menubar.rs`, `src-tauri/src/desktop_integration.rs`

**Interfaces:**
- Consumes: `AppRemovalAction`, `MenubarPlan`, `SettingsService::update_from_view`.
- Produces:
  - `MenubarState::allows_no_menubar() -> bool`
  - `MenubarState::set_allow_no_menubar(bool)`
  - `DesktopIntegration::set_menu_entry_available(bool)`
  - `fn disable_provider_menubar(app, provider_id)`
  - `fn handle_app_menubar_removed(app)`

- [ ] **Step 1: Write failing runtime-state tests**

In `desktop_integration.rs` tests:

```rust
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
```

In `menubar.rs` tests:

```rust
#[test]
fn provider_removal_disables_only_the_provider_menubar_layout() {
    let mut settings = AppSettings::default();
    settings
        .taskband_providers
        .insert("codex".into(), TaskbandLayout::default());
    disable_provider_layout(&mut settings, "codex");
    assert!(!settings.taskband_providers["codex"].enabled);
}
```

- [ ] **Step 2: Run the tests and verify they fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib losing_the_menu_entry_makes_a_floating_window_exit_on_close
cargo test --manifest-path src-tauri/Cargo.toml --lib provider_removal_disables_only_the_provider_menubar_layout
```

Expected: compilation fails because the methods/functions do not exist.

- [ ] **Step 3: Add runtime entry availability**

In `DesktopIntegration`:

```rust
pub fn set_menu_entry_available(&self, available: bool) {
    self.tray_available.store(available, Ordering::SeqCst);
    if !available {
        self.set_floating(true);
    }
}
```

Keep `tray_available()` as the compatibility accessor used by settings/window code.

- [ ] **Step 4: Add no-menubar runtime state**

Add to `MenubarState`:

```rust
allow_no_menubar: AtomicBool,
```

Extend the existing `std::sync` import in `menubar.rs` with `atomic::{AtomicBool, Ordering}`.

Initialize it with `AtomicBool::new(false)` and add:

```rust
fn allows_no_menubar(&self) -> bool {
    self.allow_no_menubar.load(Ordering::SeqCst)
}

fn set_allow_no_menubar(&self, value: bool) {
    self.allow_no_menubar.store(value, Ordering::SeqCst);
}
```

In reconciliation, clear the exception whenever `show_app_menubar` becomes true or a provider instance exists:

```rust
if settings.show_app_menubar || !plan.provider_instances.is_empty() {
    menubar.set_allow_no_menubar(false);
}
```

In `commands/settings.rs::settings_view_state`, replace the temporary `false` argument added in Task 2 with:

```rust
app.state::<crate::menubar::MenubarState>().allows_no_menubar()
```

- [ ] **Step 5: Implement provider drag-out as a durable disable**

Add:

```rust
fn disable_provider_layout(settings: &mut AppSettings, provider_id: &str) {
    settings
        .taskband_providers
        .entry(provider_id.to_owned())
        .or_default()
        .enabled = false;
}
```

Replace the provider remove listener body with logic equivalent to:

```rust
let service = listener_app.state::<Arc<SettingsService>>();
let mut next = service.get();
disable_provider_layout(&mut next, &provider_id);
let expected_settings = service.settings_revision();
let expected_account = service.account_revision();
if let Ok(updated) = service.update_from_view(next, expected_settings, expected_account) {
    let provider_service = listener_app.state::<Arc<ProviderService>>();
    tray_presentation::update(
        &listener_app,
        &provider_service.state(),
        &updated,
        service.registry(),
    );
    let _ = listener_app.emit(
        "settings-state",
        crate::commands::settings::settings_view_state(
            &listener_app,
            service.inner().as_ref(),
        ),
    );
}
```

The listener must clone owned values before moving them into the callback.

- [ ] **Step 6: Implement app drag-out behavior**

In the app remove listener:

1. Persist `show_app_menubar = false` with revision checks.
2. Compute provider instances with `desired_provider_menubars`.
3. Determine floating visibility with `window.is_visible() && !window.is_minimized()`.
4. Apply `app_removal_action`.

For `HideOnly`, reconcile normally.

For `KeepWindowThenExit`:

```rust
menubar.set_allow_no_menubar(true);
app.state::<DesktopIntegration>().set_menu_entry_available(false);
```

For `ExitNow`, schedule the existing delayed exit pattern:

```rust
let app = app.clone();
std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_millis(200));
    app.exit(0);
});
```

After reconciliation, call:

```rust
app.state::<DesktopIntegration>()
    .set_menu_entry_available(plan.app_instance_visible || !plan.provider_instances.is_empty());
```

- [ ] **Step 7: Handle app-instance creation failure**

When the app instance is required, call `apply_instance` and verify plugin visibility:

```rust
if plan.app_instance_visible {
    let result = menubar.apply_instance(app, APP_MENUBAR_INSTANCE_ID, app_instance_config(true));
    let visible = result.is_ok()
        && app
            .multiline_menubar()
            .is_visible(APP_MENUBAR_INSTANCE_ID.to_owned())
            .unwrap_or(false);
    if !visible {
        crate::app_warn!("menubar", "could not create the required Quota01 app item");
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            let _ = crate::window::apply_window_mode(
                &window,
                crate::models::WindowMode::Floating,
                true,
            );
        }
        app.state::<DesktopIntegration>().set_menu_entry_available(false);
    }
}
```

If the floating fallback cannot be shown, exit immediately rather than leaving a headless process.

- [ ] **Step 8: Run focused and full Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib losing_the_menu_entry_makes_a_floating_window_exit_on_close
cargo test --manifest-path src-tauri/Cargo.toml --lib provider_removal_disables_only_the_provider_menubar_layout
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/menubar.rs src-tauri/src/desktop_integration.rs src-tauri/src/commands/settings.rs
git commit -m "feat: enforce macOS menubar removal semantics"
```

---

### Task 5: Remove the macOS Tauri Tray and Legacy Workaround

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/tray_presentation.rs`
- Modify: `src-tauri/src/menubar.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**
- Consumes: macOS menubar reconciliation from Tasks 1-4.
- Produces: macOS builds without any Tauri tray object or `objc2-app-kit` direct dependency.

- [ ] **Step 1: Confirm the existing tests are green before the refactor**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: PASS. This is a mechanical refactor; existing tests and platform `cfg` checks are the safety net.

- [ ] **Step 2: Make the Tauri tray paths non-macOS only**

In `lib.rs`:

```rust
#[cfg(not(target_os = "macos"))]
fn install_tray(app: &mut App) -> Result<(), Box<dyn std::error::Error>>
```

Keep the existing non-macOS function body unchanged.

In setup:

```rust
#[cfg(target_os = "macos")]
let tray_installed = true;
#[cfg(not(target_os = "macos"))]
let tray_installed = if desktop_integration.tray_available() {
    match install_tray(app) {
        Ok(()) => true,
        Err(error) => {
            app_warn!(
                "lifecycle",
                "system tray integration failed; using standalone window: {error}"
            );
            desktop_integration.disable_tray();
            let _ = app.remove_tray_by_id("quota01-tray");
            false
        }
    }
} else {
    false
};
```

In `tray_presentation::update`, keep the call to `menubar::update` and return immediately on macOS:

```rust
#[cfg(target_os = "macos")]
{
    crate::menubar::update(app, state, settings, registry);
    return;
}
```

Remove `objc2-app-kit` from the macOS target dependencies and delete `configure_main_tray_removal`, `restore_main_tray`, and the single-instance restore call from the earlier workaround.

Also move tray-only imports and constants behind `#[cfg(not(target_os = "macos"))]` so macOS builds do not compile unused Tauri tray APIs:

```rust
#[cfg(not(target_os = "macos"))]
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

#[cfg(not(target_os = "macos"))]
const TRAY_ID: &str = "quota01-tray";
```

Keep `App`, `AppHandle`, `Emitter`, and `Manager` imported unconditionally because macOS still uses them.

In `tray_presentation.rs`, add `#[cfg(any(not(target_os = "macos"), test))]` to the existing definitions of `TrayGroup`, `resolved_groups`, `usage_metric`, `format_tokens`, and `mark_icon`. Leave their bodies unchanged; this prevents macOS production builds from reporting them as dead code while keeping cross-platform tests available.

Also gate the `tauri::image::Image` import with `#[cfg(any(not(target_os = "macos"), test))]`, because it is only used by the gated raster icon path.

- [ ] **Step 3: Verify no macOS Tauri tray references remain**

```bash
rg -n 'configure_main_tray_removal|restore_main_tray|objc2-app-kit' src-tauri/src src-tauri/Cargo.toml
```

Expected: no matches.

- [ ] **Step 4: Build and run the platform tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: pass on macOS; Windows/Linux tray code still compiles under its cfg.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/tray_presentation.rs src-tauri/src/menubar.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "refactor: remove the macOS Tauri system tray"
```

---

### Task 6: Settings UI and Localization

**Files:**
- Modify: `src/lib/SettingsScreen.svelte`
- Modify: `src/lib/uiLanguage.test.ts`
- Modify: `src/lib/i18n/messages/en.ts`
- Modify: `src/lib/i18n/messages/zh-CN.ts`
- Modify: `src/lib/i18n/messages/zh-TW.ts`
- Modify: `src/lib/i18n/messages/es.ts`
- Modify: `src/lib/i18n/messages/pt-BR.ts`
- Modify: `src/lib/i18n/messages/ja.ts`
- Modify: `src/lib/i18n/messages/ko.ts`
- Modify: `src/lib/i18n/messages/de.ts`
- Modify: `src/lib/i18n/messages/fr.ts`
- Modify: `src/lib/i18n/messages/ru.ts`
- Modify: `src/lib/i18n/messages/hi.ts`
- Modify: `src/lib/i18n/messages/ar.ts`
- Modify: `src/lib/i18n/messages/it.ts`
- Modify: `src/lib/i18n/messages/pl.ts`
- Modify: `src/lib/i18n/messages/tr.ts`
- Modify: `src/lib/i18n/messages/vi.ts`

**Interfaces:**
- Consumes: `settings.showAppMenubar`, `settingsView.appMenubarForced`, existing `platform` prop.
- Produces: macOS General settings row and forced-visible explanation.

- [ ] **Step 1: Add a failing UI language test**

In `src/lib/uiLanguage.test.ts`, add to the settings-label test:

```ts
expect(settings).toContain("$tStore('settings.showAppMenubar')");
expect(settings).toContain("$tStore('settings.appMenubarForced')");
expect(en.settings.showAppMenubar).toBe('Show Quota01 Menu Bar Icon');
expect(en.settings.appMenubarForced).toBe(
  'No provider menu bar icons are visible, so Quota01 stays visible.',
);
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
corepack pnpm exec vitest run src/lib/uiLanguage.test.ts
```

Expected: assertions fail because the strings do not exist.

- [ ] **Step 3: Add the settings row**

In `SettingsScreen.svelte`, inside General and under the existing general rows:

```svelte
{#if platform === 'macos'}
  <label class="setting-row">
    <span>
      <b>{$tStore('settings.showAppMenubar')}</b>
      {#if settingsView.appMenubarForced}
        <small>{$tStore('settings.appMenubarForced')}</small>
      {/if}
    </span>
    <input
      type="checkbox"
      checked={settings.showAppMenubar}
      onchange={(event) => patch({ showAppMenubar: event.currentTarget.checked })}
    />
  </label>
{/if}
```

The existing `.setting-row` and `.setting-row small` styles keep the control on the same row and render the explanation below the label.

- [ ] **Step 4: Add exact translations**

Add `showAppMenubar` and `appMenubarForced` with these values:

| Locale | `showAppMenubar` | `appMenubarForced` |
| --- | --- | --- |
| en | Show Quota01 Menu Bar Icon | No provider menu bar icons are visible, so Quota01 stays visible. |
| zh-CN | 显示 Quota01 菜单栏图标 | 当前没有 Provider 菜单栏图标，因此 Quota01 会保持显示。 |
| zh-TW | 顯示 Quota01 選單列圖示 | 目前沒有 Provider 選單列圖示，因此 Quota01 會保持顯示。 |
| es | Mostrar icono de Quota01 en la barra de menús | No hay iconos de proveedor visibles, así que Quota01 permanece visible. |
| pt-BR | Mostrar ícone do Quota01 na barra de menus | Não há ícones de provedor visíveis, então o Quota01 permanece visível. |
| ja | Quota01 のメニューバーアイコンを表示 | Provider のメニューバーアイコンがないため、Quota01 は表示されたままになります。 |
| ko | Quota01 메뉴 막대 아이콘 표시 | 표시되는 Provider 메뉴 막대 아이콘이 없어 Quota01 아이콘이 계속 표시됩니다. |
| de | Quota01-Symbol in der Menüleiste anzeigen | Es sind keine Provider-Symbole sichtbar, daher bleibt Quota01 sichtbar. |
| fr | Afficher l’icône Quota01 dans la barre des menus | Aucune icône de fournisseur n’est visible, donc Quota01 reste visible. |
| ru | Показывать значок Quota01 в строке меню | Значки Provider не отображаются, поэтому Quota01 остаётся видимым. |
| hi | Quota01 मेनू बार आइकन दिखाएँ | कोई Provider मेनू बार आइकन दिखाई नहीं दे रहा, इसलिए Quota01 दिखाई देता रहेगा। |
| ar | إظهار أيقونة Quota01 في شريط القوائم | لا تظهر أي أيقونات Provider، لذا تظل أيقونة Quota01 ظاهرة. |
| it | Mostra l’icona Quota01 nella barra dei menu | Nessuna icona Provider è visibile, quindi Quota01 rimane visibile. |
| pl | Pokaż ikonę Quota01 na pasku menu | Brak widocznych ikon Provider, więc Quota01 pozostaje widoczny. |
| tr | Quota01 menü çubuğu simgesini göster | Görünür Provider menü çubuğu simgesi olmadığından Quota01 görünür kalır. |
| vi | Hiện biểu tượng Quota01 trên thanh menu | Không có biểu tượng Provider nào hiển thị, nên Quota01 vẫn hiển thị. |

- [ ] **Step 5: Run frontend tests**

```bash
corepack pnpm exec vitest run src/lib/uiLanguage.test.ts src/lib/i18n.test.ts src/App.test.ts src/App.customization.test.ts
corepack pnpm check
```

Expected: pass. Fix any fixture expecting exact `SettingsViewState` fields.

- [ ] **Step 6: Commit**

```bash
git add src/lib/SettingsScreen.svelte src/lib/uiLanguage.test.ts src/lib/i18n/messages
git commit -m "feat: expose the macOS app menubar setting"
```

---

### Task 7: Full Verification and macOS Smoke Test

**Files:**
- Verify only; modify code only if a test exposes a defect.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: release-ready verification evidence and manual macOS confirmation.

- [ ] **Step 1: Run the complete automated verification path**

```bash
corepack pnpm verify:versions
corepack pnpm verify:contracts
corepack pnpm verify:frontend
corepack pnpm verify:rust
```

Expected: every command exits `0`.

- [ ] **Step 2: Build the macOS app bundle**

```bash
corepack pnpm tauri build --bundles app
```

Expected: `src-tauri/target/release/bundle/macos/Quota01.app` is produced.

- [ ] **Step 3: Run the manual smoke test**

Quit the existing app, then open the newly built bundle. Verify:

1. With providers and `showAppMenubar = true`, both provider and Quota01 items appear.
2. Turning off `showAppMenubar` hides only the Quota01 item while provider items remain.
3. `⌘` dragging a provider item out turns that provider's menu-bar switch off and does not recreate it after refresh.
4. Disabling the final provider makes the Quota01 item appear even when `showAppMenubar` is off.
5. In popup mode, dragging out the final Quota01 item exits the app cleanly.
6. With a visible floating window, dragging out the final Quota01 item keeps the app alive; closing the floating window exits it.
7. Relaunching the app creates the expected entry again; there is no Tauri system-tray icon alongside the plugin item.

- [ ] **Step 4: Commit any verification fixes**

If verification required changes:

```bash
git add -u
git commit -m "fix: verify macOS app menubar behavior"
```

If no files changed, do not create an empty commit.

## Plan Self-Review

- Spec coverage: architecture, visibility matrix, settings, provider/app removal, floating-window exception, failure handling, contracts, localization, and smoke tests all map to Tasks 1-7.
- Type consistency: `show_app_menubar`, `showAppMenubar`, `app_menubar_forced`, `appMenubarForced`, `MenubarPlan`, and `APP_MENUBAR_INSTANCE_ID` use one spelling throughout.
- No placeholders: every implementation step names exact files, functions, tests, commands, and expected results.
