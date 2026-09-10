# Provider Dashboard Tabs Design

## Goal

Add a compact provider switcher to the top of the main dashboard popup. The switcher lets a user move between the all-provider dashboard and an individual provider dashboard. Opening the popup from a provider-specific taskband or menubar item selects that provider in the switcher.

## Interaction

- The dashboard tab list contains an `All` tab followed by enabled providers in the existing settings order.
- `All` maps to `focusedProviderId = null` and keeps the current multi-provider dashboard, including total spend when enabled.
- A provider tab maps to its provider id and keeps the existing filtered dashboard behavior, showing only that provider's full detail.
- Each provider tab shows the provider icon, localized/custom display name, and up to two pinned metric readings. Pinned metrics are selected from the provider layout, independent of whether the metric is also enabled in the dashboard, because a pinned metric may intentionally remain available to taskband or menubar surfaces while hidden from the main card.
- Missing snapshots, missing metric values, and provider errors render stable compact placeholders rather than changing the tab dimensions. Provider errors retain the existing warning indicator and accessible status text.
- The tab list is horizontally scrollable when it does not fit. The active tab uses the existing accent color and a restrained underline/border treatment consistent with the popup.
- Tabs use `role="tablist"` and `role="tab"`, `aria-selected`, and an accessible label. Pointer activation selects immediately. Left/right arrow keys move focus and activate the adjacent tab; Home/End move to the first/last tab. Enter and Space activate the focused tab.
- Selecting a provider from the switcher updates the existing focused-provider state and triggers the current panel fitting behavior. Hiding the popup continues to clear the focused provider and return to All.

## Components and data flow

Create a focused `ProviderTabs.svelte` component under `src/lib`. It receives the current `UsageViewState`, `AppSettings`, `ProviderCatalogIndex`, the selected provider id, and an `onSelect` callback. It derives enabled providers in settings order and resolves each pinned metric through the catalog and the provider snapshot.

Add a small pure formatter/helper for tab readings if the existing row formatters cannot be reused without rendering full metric cards. It must use the same `usageDisplay` preference and localization helpers as the full metric surface. The helper should return a display string and an accessible label, with `--` for unavailable data.

Render `ProviderTabs` only on the dashboard screen, above the dashboard content. Keep the source of truth in `App.svelte`: tab selection calls a handler that updates `focusedProviderId`, scrolls the content to the top, and schedules the existing window fit. Pass the same state into `Dashboard`, so taskband-originated selection and tab-originated selection share filtering behavior.

## Error handling and compatibility

No backend or persistence changes are required. Unknown, disabled, or removed provider ids are ignored by the tab list; the existing App effect clears an invalid focused id. The existing `focusedProviderId` taskband event remains the entry point for provider-specific popup opening.

The Vite target includes Safari 13, so the strip must have a functional flex/overflow fallback. Modern CSS enhancements should be additive and must not be required for tab selection or readable values. Respect the existing density, theme, RTL, and reduced-motion settings.

## Testing

- Unit-test the pure tab summary resolver/formatter for pinned metric order, quota/value/status sources, user display mode, unavailable snapshots, and more than two pinned metrics.
- Add component or App tests that verify All and provider tabs, `aria-selected`, provider summary values, pointer selection, keyboard navigation, and the existing taskband-open event selecting the matching provider.
- Verify that selecting All restores total spend and all provider sections, while selecting a provider hides total spend and other provider sections.
- Run `pnpm check`, the focused Vitest tests, the full frontend test suite, lint, and production build. Inspect the rendered popup at narrow and wide widths for clipping and layout shifts.

## Scope

This change is limited to dashboard navigation and presentation. It does not alter provider detection, refresh scheduling, pinned metric persistence, taskband rendering, or settings screen navigation.
