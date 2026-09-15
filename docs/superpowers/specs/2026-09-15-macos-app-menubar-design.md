# macOS App Menubar Design

## Goal

Replace the macOS Tauri system-tray icon with a native `multiline-menubar` instance owned by Quota01. The app icon is optional when provider menubar instances already provide an entry point, but the app must always leave at least one usable menu-bar entry unless a visible floating window remains open.

## Architecture

- macOS must not create a Tauri `TrayIcon`.
- `menubar::update` becomes the single macOS reconciler for both provider instances and one synthetic app instance.
- The app instance uses the stable id `quota01-app`, displays only the existing Quota01 mark, and has both text lines hidden.
- Left-click toggles the main window and positions a popup below the clicked app item. Right-click shows the existing Settings and Quit actions for Quota01.
- Windows and Linux keep the existing Tauri tray and taskband behavior.

`tray_presentation::update` delegates directly to `menubar::update` on macOS and does not access a Tauri tray. `install_tray` is compiled and called only on non-macOS platforms.

## Visibility Model

Add `show_app_menubar: bool` to `AppSettings`, defaulting to `true`. Persist it as schema version 9 and expose it as `showAppMenubar` in the frontend settings contract.

The reconciler first resolves the desired provider instances using the existing rules:

- provider is enabled;
- provider definition exists;
- the provider's menu-bar display toggle is enabled;
- at least one pinned metric is currently renderable.

The app instance is visible when:

```text
show_app_menubar || provider_instances.is_empty()
```

This produces four cases:

| Provider instances | `show_app_menubar` | App instance |
| --- | --- | --- |
| present | true | visible |
| present | false | hidden |
| absent | true | visible |
| absent | false | forced visible |

The persisted setting is not rewritten when the app instance is forced visible. The backend exposes an `app_menubar_forced` boolean in `SettingsViewState`, and the settings UI explains that Quota01 must remain visible because no provider menu-bar item is available.

## Settings UI

On macOS, add a General settings row labeled "Show Quota01 menu bar icon". The control remains editable in every valid state. When `app_menubar_forced` is true, show a secondary explanation that the icon is currently required because no provider icon is visible.

The setting is hidden on Windows and Linux because those platforms continue using their existing tray/taskband model. All required labels and explanations must exist in every supported locale.

## Reconciliation and Removal

The reconciliation code must return a testable plan containing the desired provider instance ids and the effective app-instance visibility. Applying the plan performs plugin calls and updates `MenubarState`.

### Provider Item Removed by the User

When the plugin emits a provider instance removal event:

1. Set that provider's `taskband_providers[id].enabled` to `false` using the existing revision-checked settings mutation.
2. Emit `settings-state` so the settings UI reflects the change.
3. Re-run menubar reconciliation immediately.
4. If this leaves no provider instances, create/show the Quota01 app instance according to the minimum-one rule.

Dragging a provider item out therefore has the same durable effect as switching off that provider's menu-bar display option.

### App Item Removed by the User

When the Quota01 app instance is removed:

1. Persist `show_app_menubar = false`.
2. If any provider instance remains, keep the app instance hidden and continue running.
3. If the app instance was the last menu-bar item:
   - If the main floating window is currently visible, allow the no-menu-bar state and mark the app as exit-on-close.
   - If the mode is popup, or the floating window is hidden, exit the app after a short delay so the native removal gesture can finish.

The delayed exit uses the same approximately 200 ms pattern already used by the application's native menu actions.

## Runtime Entry Availability

`DesktopIntegration` must distinguish between a running floating window and an available menu-bar entry. On macOS, reconciliation updates its entry-availability state whenever the number of effective menu-bar instances changes.

When no menu-bar instance exists:

- a visible floating window keeps the app alive, but closing or dismissing that window exits the app;
- popup mode exits immediately because no persistent UI remains.

When a menu-bar instance is created again, entry availability returns to true and normal close behavior is restored.

## Failure Handling

Plugin errors must be logged rather than ignored. If the app instance is required but cannot be created:

- first try to restore or recreate the app instance;
- if creation still fails, show the main window in floating mode and mark menu-bar entry availability as false;
- never leave the macOS process running with neither a menu-bar entry nor a visible window.

If a provider instance cannot be created, reconciliation continues for the remaining providers and normal refresh/error logging applies.

## Components

- `src-tauri/src/models.rs`: add `show_app_menubar`, default it to true, bump schema version, and extend `SettingsViewState` with `app_menubar_forced`.
- `src-tauri/src/settings.rs`: normalize/persist the new field and expose the forced state through `view_state`.
- `src-tauri/src/menubar.rs`: own the app instance, application menu, click behavior, removal semantics, reconciliation plan, and related tests.
- `src-tauri/src/tray_presentation.rs`: stop touching Tauri tray APIs on macOS.
- `src-tauri/src/lib.rs`: do not install a Tauri tray on macOS; preserve existing Windows/Linux setup.
- `src-tauri/src/desktop_integration.rs`: track runtime menu-bar entry availability and drive `exits_on_close` correctly.
- `src/lib/types.ts`: add `showAppMenubar` and `appMenubarForced` contracts.
- `src/lib/SettingsScreen.svelte`: add the macOS-only switch and forced-visible explanation.
- i18n message files: add the settings label, explanation, and app context-menu strings for all supported languages.

## Testing

Add deterministic unit tests for the reconciliation plan:

- all four provider/app visibility combinations;
- providers that are disabled, have disabled menu-bar layout, or have no renderable pinned metrics;
- provider removal disables the provider's menu-bar layout;
- the last-provider removal forces the app instance visible;
- app removal chooses hide, exit-on-close, or immediate exit correctly.

Add settings and contract tests for:

- missing `showAppMenubar` deserializes to true;
- schema version 9 normalization;
- Rust-to-TypeScript field presence;
- `app_menubar_forced` only when the app icon is forced by the minimum-one rule.

Add frontend tests for the macOS settings row, the explanation shown while forced, persistence through the settings controller, and all new localization keys.

Run the full Rust and frontend verification suites, then perform a macOS smoke test covering:

1. fresh launch with providers present and `showAppMenubar = true`;
2. turning the app icon off while provider icons remain;
3. dragging a provider icon out and confirming its menu-bar setting turns off;
4. disabling the final provider and confirming the app icon appears even when its setting is off;
5. dragging out the final app icon in popup mode and confirming a clean exit;
6. repeating with a visible floating window and confirming the app stays alive until that window closes.

## Compatibility and Scope

- Existing settings receive `showAppMenubar = true` by default, preserving the visible entry point after migration.
- Windows and Linux tray behavior is unchanged.
- This design does not change provider refresh logic, metric selection, pinned metric persistence, or general window layout.
- The previous macOS Tauri-tray removal workaround is superseded and should not remain as a second app entry point.
