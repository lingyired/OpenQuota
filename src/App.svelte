<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import {
    beginPanelResize,
    dismissMainWindow,
    dismissSettingsWindow,
    getBootstrapState,
    getLogPath,
    getPanelHeightMode,
    getPanelResizeEdge,
    lockPanelResizeAxis,
    onOpenScreen,
    onSettingsWorkspaceSelection,
    onMainWindowHidden,
    onSettingsState,
    onRequestLeaveSettings,
    onTaskbandOpen,
    onUpdateProgress,
    onUsageState,
    openProviderLink as openProviderLinkCommand,
    openSettingsWindow,
    openNotificationSettings as openSystemNotificationSettings,
    openLogFolder as openSystemLogFolder,
    quitApplication,
    refreshSelectedProviderIfDue,
    refreshProviderUsage,
    refreshUsage,
    requestNotificationPermission,
    resetAllSettings as resetAllSettingsCommand,
    resetCustomization as resetCustomizationCommand,
    resetProviderCustomization as resetProviderCustomizationCommand,
    setPanelHeightAutomatic,
    setPanelHeightManual,
    type PanelHeightMode,
    type PanelResizeEdge,
  } from './lib/backend';
  import ConfirmationSheet from './lib/ConfirmationSheet.svelte';
  import { restoreCustomization } from './lib/customizationHistory';
  import ProviderDataView from './lib/ProviderDataView.svelte';
  import Dashboard, { type DashboardProps } from './lib/Dashboard.svelte';
  import Icon from './lib/Icon.svelte';
  import { createListenerRegistry } from './lib/listenerRegistry';
  import { emptyProviderCatalog, ProviderCatalogIndex } from './lib/metrics';
  import Quota01Mark from './lib/Quota01Mark.svelte';
  import ProviderRail from './lib/ProviderRail.svelte';
  import { desktopPlatform } from './lib/platform';
  import { cancelActiveReorder } from './lib/pointerReorder';
  import { withProviderName } from './lib/providerNames';
  import RenameProviderSheet from './lib/RenameProviderSheet.svelte';
  import {
    buildProviderShareRows,
    renderProviderShareCard,
    renderTotalSpendShareCard,
  } from './lib/shareCard';
  import SettingsWorkspace from './lib/SettingsWorkspace.svelte';
  import { SettingsController } from './lib/settingsController.svelte';
  import type { SpendProjection } from './lib/totalSpend';
  import type { AppSettings, UsageViewState } from './lib/types';
  import { UpdateController, nextUpdateLabel } from './lib/updateController.svelte';
  import { automaticUpdateDelay, UPDATE_CHECK_INTERVAL_MS } from './lib/updateSchedule';
  import { createWindowController, type AppScreen } from './lib/windowController';
  import { setLanguage, t, tBackendStore, tStore } from './lib/i18n';
  import { locale } from 'svelte-i18n';

  type Screen = AppScreen;
  const appVersion = import.meta.env.APP_VERSION;
  const isSettingsWindow = getCurrentWindow().label === 'settings';
  const emptyView: UsageViewState = { providers: {} };

  let viewState = $state<UsageViewState>(emptyView);
  let catalog = $state<ProviderCatalogIndex>(emptyProviderCatalog);
  // The popup selection survives hiding the window and is restored on reopen.
  const lastProviderStorageKey = 'quota01.lastSelectedProviderId';
  let currentProviderId = $state<string | null>(readLastSelectedProvider());
  let taskbandProviderId = $state<string | null>(null);
  // Each native window has one permanent page: the popup is always the dashboard,
  // while the Settings window owns the provider, settings, and usage-preview workspace.
  const screen: Screen = isSettingsWindow ? 'settings' : 'dashboard';
  let settingsWorkspaceRequest = $state<{ target: Screen; revision: number }>({
    target: 'settings',
    revision: 0,
  });
  let now = $state(Date.now());
  let settingsError = $state<string | null>(null);
  let automaticUpdatesReady = $state(false);
  let systemReducedMotion = $state(false);
  let customizationHistory = $state<AppSettings[]>([]);
  let customizationGestureStart: AppSettings | null = null;
  let reordering = $state(false);
  let confirmationMessage = $state<string | null>(null);
  let resetConfirmationOpen = $state(false);
  let settingsResetConfirmationOpen = $state(false);
  let leaveSettingsConfirmationOpen = $state(false);
  let leavingSettings = $state(false);
  let resettingCustomization = $state(false);
  let resettingAllSettings = $state(false);
  let resettingProviderId = $state<string | null>(null);
  let shareTimer: ReturnType<typeof setTimeout> | undefined;
  const providerStates = $derived(Object.values(viewState.providers));
  const anyRefreshing = $derived(providerStates.some((state) => state.refreshing));
  const lastFullRefresh = $derived(viewState.lastFullRefreshAt ?? undefined);
  const nextRefreshAt = $derived(
    viewState.nextRefreshAt ??
      (lastFullRefresh
        ? new Date(Date.parse(lastFullRefresh) + 5 * 60_000).toISOString()
        : undefined),
  );
  const currentLocale = $derived($locale);
  const nextUpdateLabelText = $derived.by(() => {
    void currentLocale;
    return nextUpdateLabel(nextRefreshAt, now);
  });
  const platform = desktopPlatform();
  const settingsController = new SettingsController((message) => (settingsError = message));
  const settingsState = $derived(settingsController.state);
  const enabledProviderIds = $derived(
    settingsState?.settings.providers
      .filter((provider) => provider.enabled && catalog.provider(provider.id))
      .map((provider) => provider.id) ?? [],
  );
  const selectedProviderId = $derived(
    currentProviderId && enabledProviderIds.includes(currentProviderId)
      ? currentProviderId
      : (enabledProviderIds[0] ?? null),
  );
  const reducedMotion = $derived(
    systemReducedMotion || Boolean(settingsState?.settings.reduceAnimations),
  );
  const floatingWindow = $derived(
    !isSettingsWindow &&
      !!settingsState &&
      (!settingsState.trayAvailable || settingsState.settings.windowMode === 'floating'),
  );
  const providerDisplayName = (id: string) =>
    catalog.displayName(id, settingsState?.settings.providerNames);
  const updates = new UpdateController();
  const dashboardProps: DashboardProps = $derived({
    viewState,
    catalog,
    renamableProviderIds: settingsState?.renamableProviderIds ?? [],
    settings: settingsState?.settings ?? ({} as AppSettings),
    now,
    onSettingsChange: saveSettings,
    onCustomizationChange: saveCustomization,
    onReorderStart: beginCustomizationGesture,
    onReorderEnd: endCustomizationGesture,
    onCustomize: () => navigate('customize'),
    onOpenProviderCustomize: (id: string) => void openProviderCustomization(id),
    onRenameProvider: openRenameProvider,
    onShare: shareProvider,
    onShareTotal: shareTotalSpend,
    onRefresh: refreshProvider,
    onRefreshIfDue: refreshProviderIfDue,
    onRefreshAll: refresh,
    statusProviderId: selectedProviderId,
    onOpenProviderLink: openProviderLink,
    onContentMorph: beginContentMorph,
    reducedMotion,
    updateStatus: updates.status,
    installingUpdate: updates.installing,
    updateProgress: updates.progress,
    updateError: updates.error,
    onInstallUpdate: () => updates.install(),
    onOpenUpdatePage: () => updates.openDownloadPage(),
  });

  let resizeEdge = $state<PanelResizeEdge>(platform === 'windows' ? 'top' : 'bottom');
  const renderedResizeEdge = $derived(
    isSettingsWindow ? null : floatingWindow ? 'bottom' : resizeEdge,
  );
  let panelHeightMode = $state<PanelHeightMode>('automatic');
  let panelHeightModeRequest = 0;
  let panelHeightModeMutation: Promise<void> = Promise.resolve();
  let renameCard = $state<{ id: string; initialValue: string } | null>(null);
  let lastResizeGripPointerAt = Number.NEGATIVE_INFINITY;
  let panelResizeOperation: Promise<void> | null = null;
  const windowController = createWindowController({
    screen: () => screen,
    independentSettingsWindow: () => isSettingsWindow,
    refreshing: () => anyRefreshing,
    reordering: () => reordering,
    fixedHeight: () =>
      platform === 'macos' && taskbandProviderId !== null && screen === 'dashboard',
    automatic: () => panelHeightMode === 'automatic',
    reducedMotion: () => reducedMotion,
    onError: (message) => (settingsError = message),
  });

  $effect(() => {
    if (!settingsState) return;
    setLanguage(settingsState.settings.language);
  });

  $effect(() => {
    const root = document.documentElement;
    root.toggleAttribute('data-reduced-motion', reducedMotion);
    const activeLocale = currentLocale ?? 'en';
    root.lang = activeLocale;
    root.dir = activeLocale === 'ar' ? 'rtl' : 'ltr';
    if (!settingsState) return;
    if (settingsState.settings.theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = settingsState.settings.theme;
    root.dataset.density = settingsState.settings.density;
  });

  $effect(() => {
    if (!settingsState) return;
    if (selectedProviderId !== currentProviderId) currentProviderId = selectedProviderId;
    try {
      if (selectedProviderId)
        window.localStorage.setItem(lastProviderStorageKey, selectedProviderId);
      else window.localStorage.removeItem(lastProviderStorageKey);
    } catch {
      // The popup still chooses the first enabled provider when browser storage is unavailable.
    }
  });

  $effect(() => {
    if (!automaticUpdatesReady || !settingsState?.settings.autoCheckUpdates) return;
    const delay = automaticUpdateDelay(settingsState.settings.lastUpdateCheckAt);
    let interval: ReturnType<typeof setInterval> | undefined;
    const timer = setTimeout(() => {
      void checkForUpdates();
      interval = setInterval(() => void checkForUpdates(), UPDATE_CHECK_INTERVAL_MS);
    }, delay);
    return () => {
      clearTimeout(timer);
      if (interval) clearInterval(interval);
    };
  });

  function readLastSelectedProvider(): string | null {
    try {
      return window.localStorage.getItem(lastProviderStorageKey);
    } catch {
      return null;
    }
  }

  function scheduleWindowFit() {
    windowController.scheduleFit();
  }

  function beginContentMorph() {
    windowController.beginContentMorph();
  }

  function closeMainWindow() {
    if (
      (platform === 'macos' || platform === 'windows') &&
      settingsState?.trayAvailable === false
    ) {
      if (screen === 'dashboard') navigate('settings');
      else leaveSettingsConfirmationOpen = true;
      return;
    }
    if (requiresLeaveSettingsConfirmation()) {
      leaveSettingsConfirmationOpen = true;
      return;
    }
    resetTransientUi();
    navigate('dashboard');
    void dismissMainWindow();
  }
  function requiresLeaveSettingsConfirmation() {
    return (
      screen !== 'dashboard' &&
      (platform === 'macos' || platform === 'windows') &&
      settingsState?.trayAvailable === false
    );
  }
  function requestLeaveSettings() {
    if (isSettingsWindow) {
      if (requiresLeaveSettingsConfirmation()) leaveSettingsConfirmationOpen = true;
      else void dismissSettingsWindow();
      return;
    }
    closeMainWindow();
  }
  async function confirmLeaveSettings() {
    leavingSettings = true;
    await settingsController.waitForPendingMutations();
    await quitApplication();
  }
  function cancelLeaveSettings() {
    leaveSettingsConfirmationOpen = false;
    leavingSettings = false;
  }
  function closeTransientLayers() {
    resetConfirmationOpen = false;
    settingsResetConfirmationOpen = false;
    renameCard = null;
    confirmationMessage = null;
  }
  function resetTransientUi() {
    closeTransientLayers();
    resettingCustomization = false;
    resettingAllSettings = false;
    resettingProviderId = null;
    scrollScreenToTop();
  }
  function quitApp() {
    void quitApplication();
  }
  function navigate(next: Screen) {
    if (next === 'dashboard') {
      if (isSettingsWindow) requestLeaveSettings();
      return;
    }
    if (!isSettingsWindow) {
      closeTransientLayers();
      void openSettingsWindow(next).catch(
        () => (settingsError = t('app.errors.backendUnavailable')),
      );
      return;
    }
    cancelActiveReorder();
    closeTransientLayers();
    settingsWorkspaceRequest = {
      target: next,
      revision: settingsWorkspaceRequest.revision + 1,
    };
  }
  function openProviderCustomization(providerId: string) {
    navigate(`provider:${providerId}`);
  }
  async function focusTaskbandProvider(providerId: string) {
    navigate('dashboard');
    await selectDashboardProvider(providerId);
    taskbandProviderId = providerId;
    void refreshProviderIfDue(providerId);
  }
  async function selectDashboardProvider(providerId: string) {
    taskbandProviderId = null;
    currentProviderId = providerId;
    await tick();
    scrollScreenToTop();
    // The filtered dashboard has a different natural height; fit immediately instead of relying on
    // the platform webview to emit a later resize-observer pass.
    scheduleWindowFit();
  }

  function scrollScreenToTop() {
    for (const selector of ['.content', '.screen-stage']) {
      const element = document.querySelector<HTMLElement>(selector);
      if (element && typeof element.scrollTo === 'function') element.scrollTo({ top: 0 });
      else if (element) element.scrollTop = 0;
    }
  }
  function saveSettings(next: AppSettings) {
    settingsError = null;
    const windowModeChanged = settingsState?.settings.windowMode !== next.windowMode;
    if (windowModeChanged) beginContentMorph();
    const save = settingsController.save(next);
    if (windowModeChanged) void save.finally(updatePanelResizeEdge);
  }

  function cloneSettings(value: AppSettings): AppSettings {
    return JSON.parse(JSON.stringify(value)) as AppSettings;
  }
  function showConfirmation(message: string) {
    confirmationMessage = message;
    if (shareTimer) clearTimeout(shareTimer);
    shareTimer = setTimeout(() => (confirmationMessage = null), 1800);
  }
  function saveCustomization(next: AppSettings) {
    const current = settingsState;
    if (!current) return;
    if (customizationGestureStart) {
      settingsController.setDraftSettings(next);
      settingsError = null;
      return;
    }
    customizationHistory = [...customizationHistory.slice(-19), cloneSettings(current.settings)];
    saveSettings(next);
  }
  function openRenameProvider(providerId: string) {
    const current = settingsState;
    if (!current) return;
    renameCard = {
      id: providerId,
      initialValue: current.settings.providerNames[providerId] ?? '',
    };
  }
  async function closeRenameProvider() {
    const providerId = renameCard?.id;
    renameCard = null;
    if (!providerId) return;
    await tick();
    const provider = [...document.querySelectorAll<HTMLElement>('[data-provider-id]')].find(
      (element) => element.dataset.providerId === providerId,
    );
    provider?.querySelector<HTMLElement>('[data-reorder-touch-handle]')?.focus();
  }
  function renameProvider(name: string) {
    const current = settingsState;
    if (!current || !renameCard) return;
    const changed = withProviderName(current.settings, renameCard.id, name);
    if (changed !== current.settings) saveSettings(changed);
    void closeRenameProvider();
  }
  function beginCustomizationGesture() {
    if (!settingsState) return;
    if (!customizationGestureStart) {
      customizationGestureStart = cloneSettings(settingsState.settings);
      settingsController.beginDraft();
    }
    reordering = true;
    scheduleWindowFit();
  }
  function endCustomizationGesture(moved: boolean, cancelled = false) {
    const current = settingsState;
    if (!current) {
      settingsController.endDraft();
      return;
    }
    const start = customizationGestureStart;
    const final = current.settings;
    customizationGestureStart = null;
    reordering = false;
    if (start && moved && cancelled) settingsController.setDraftSettings(start);
    else if (start && moved) {
      customizationHistory = [...customizationHistory.slice(-19), start];
      saveSettings(final);
    }
    settingsController.endDraft();
    queueMicrotask(scheduleWindowFit);
  }
  function undoCustomization() {
    const current = settingsState;
    const previous = customizationHistory.at(-1);
    if (!current || !previous) return;
    customizationHistory = customizationHistory.slice(0, -1);
    saveSettings(restoreCustomization(current.settings, previous));
  }
  async function refresh() {
    if (anyRefreshing) return;
    viewState = {
      ...viewState,
      providers: Object.fromEntries(
        Object.entries(viewState.providers).map(([id, state]) => [
          id,
          { ...state, refreshing: true },
        ]),
      ),
    };
    try {
      viewState = await refreshUsage();
    } catch {
      viewState = {
        ...viewState,
        providers: Object.fromEntries(
          Object.entries(viewState.providers).map(([id, state]) => [
            id,
            { ...state, refreshing: false },
          ]),
        ),
      };
      settingsError = t('app.errors.providerRefresh');
    }
  }
  async function refreshProvider(providerId: string) {
    const current = viewState.providers[providerId];
    if (!current || current.refreshing) return;
    viewState = {
      ...viewState,
      providers: {
        ...viewState.providers,
        [providerId]: { ...current, refreshing: true },
      },
    };
    try {
      viewState = await refreshProviderUsage(providerId);
    } catch {
      const failed = viewState.providers[providerId];
      if (failed) {
        viewState = {
          ...viewState,
          providers: {
            ...viewState.providers,
            [providerId]: { ...failed, refreshing: false },
          },
        };
      }
      settingsError = t('app.errors.providerUsageRefresh', {
        provider: providerDisplayName(providerId),
      });
    }
  }
  async function refreshProviderIfDue(providerId: string) {
    try {
      const nextState = await refreshSelectedProviderIfDue(providerId);
      if (nextState && typeof nextState === 'object' && nextState.providers) viewState = nextState;
    } catch {
      // Selection-triggered refresh is opportunistic. The existing snapshot and
      // manual refresh path remain available when a provider is unavailable.
    }
  }
  function openProviderLink(providerId: string, linkIndex: number) {
    void openProviderLinkCommand(providerId, linkIndex).catch(() => {});
  }
  function requestCustomizationReset() {
    resetConfirmationOpen = true;
  }
  async function confirmCustomizationReset() {
    const current = settingsState;
    if (!current || resettingCustomization) return;
    resettingCustomization = true;
    const previous = cloneSettings(current.settings);
    try {
      await settingsController.runMutation((expectedSettingsRevision, expectedAccountRevision) =>
        resetCustomizationCommand(expectedSettingsRevision, expectedAccountRevision),
      );
      customizationHistory = [...customizationHistory.slice(-19), previous];
    } catch {
      settingsError = t('app.errors.customizationReset');
    } finally {
      resettingCustomization = false;
      resetConfirmationOpen = false;
    }
  }
  async function resetProviderCustomization(providerId: string) {
    const current = settingsState;
    if (!current || resettingProviderId) return;
    const provider = current.settings.providers.find((item) => item.id === providerId);
    if (!provider) return;
    const previous = cloneSettings(current.settings);
    resettingProviderId = providerId;
    try {
      await settingsController.runMutation((expectedSettingsRevision, expectedAccountRevision) =>
        resetProviderCustomizationCommand(
          providerId,
          expectedSettingsRevision,
          expectedAccountRevision,
        ),
      );
      customizationHistory = [...customizationHistory.slice(-19), previous];
    } catch {
      settingsError = t('app.errors.providerCustomizationReset', {
        provider: providerDisplayName(providerId),
      });
    } finally {
      resettingProviderId = null;
    }
  }
  async function confirmAllSettingsReset() {
    if (!settingsState || resettingAllSettings) return;
    const windowModeChanged = settingsState.settings.windowMode !== 'popup';
    if (windowModeChanged) beginContentMorph();
    resettingAllSettings = true;
    try {
      await panelHeightModeMutation;
      await settingsController.runMutation((expectedSettingsRevision, expectedAccountRevision) =>
        resetAllSettingsCommand(expectedSettingsRevision, expectedAccountRevision),
      );
      customizationHistory = [];
      updatePanelHeightMode();
      updatePanelResizeEdge();
      settingsError = null;
      showConfirmation(t('app.confirmationsShort.allSettingsRestored'));
    } catch {
      settingsError = t('app.errors.allSettingsReset');
      updatePanelHeightMode();
    } finally {
      resettingAllSettings = false;
      settingsResetConfirmationOpen = false;
    }
  }
  async function copyCanvas(canvas: HTMLCanvasElement, fallback: string) {
    const blob = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (value) => (value ? resolve(value) : reject(new Error('PNG unavailable'))),
        'image/png',
      ),
    );
    if (typeof ClipboardItem !== 'undefined' && navigator.clipboard.write) {
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
    } else {
      await navigator.clipboard.writeText(fallback);
    }
    showConfirmation(t('app.confirmationsShort.copied'));
  }
  async function shareProvider(providerId: string) {
    const current = settingsState;
    if (!current) return;
    const card = document.querySelector<HTMLElement>(`[data-provider-id="${providerId}"]`);
    if (!card) return;
    const provider = viewState.providers[providerId]?.snapshot;
    const layout = current.settings.providers.find((item) => item.id === providerId);
    if (!provider || !layout) return;
    const snapshot = [providerDisplayName(providerId), card.innerText.trim()].join('\n');
    try {
      const rows = buildProviderShareRows(catalog, provider, layout, current.settings, now);
      const canvas = renderProviderShareCard(catalog, {
        providerId,
        providerNames: current.settings.providerNames,
        plan: provider.plan,
        rows,
      });
      await copyCanvas(canvas, snapshot);
    } catch {
      settingsError = t('app.errors.screenshotCopy');
    }
  }
  async function shareTotalSpend(projection: SpendProjection) {
    const current = settingsState;
    if (!current) return false;
    const card = document.querySelector<HTMLElement>('[data-total-spend]');
    if (!card) return false;
    try {
      const canvas = renderTotalSpendShareCard(catalog, {
        projection,
        providerNames: current.settings.providerNames,
        metric: current.settings.totalSpendMetric,
        period: current.settings.totalSpendPeriod,
      });
      await copyCanvas(canvas, card.innerText.trim());
      return true;
    } catch {
      settingsError = t('app.errors.totalSpendScreenshotCopy');
      return false;
    }
  }
  async function copyLogPath() {
    const path = await getLogPath();
    await navigator.clipboard.writeText(path);
    showConfirmation(t('app.confirmationsShort.logPathCopied'));
  }
  async function openLogFolder() {
    await openSystemLogFolder();
  }
  function ownsEnterKey(target: EventTarget | null) {
    if (!(target instanceof Element)) return false;
    return (
      target.closest(
        'button, a, input, select, textarea, summary, [contenteditable], [role="button"], [role="menuitem"], [role="option"], [role="combobox"]',
      ) !== null
    );
  }
  function handleWindowPointerDown(event: PointerEvent) {
    if (
      event.target instanceof Element &&
      event.target.closest('.floating-chrome__drag') !== null
    ) {
      handleFloatingWindowPointerDown(event);
    }
  }
  function updatePanelResizeEdge() {
    if (isSettingsWindow || !('__TAURI_INTERNALS__' in window)) return;
    void getPanelResizeEdge()
      .then((edge) => (resizeEdge = edge))
      .catch(() => undefined);
  }
  function updatePanelHeightMode() {
    if (!('__TAURI_INTERNALS__' in window)) return;
    const request = ++panelHeightModeRequest;
    void getPanelHeightMode()
      .then((mode) => {
        if (request !== panelHeightModeRequest) return;
        panelHeightMode = mode;
        if (mode === 'automatic') scheduleWindowFit();
      })
      .catch(() => undefined);
  }
  function acceptPanelHeightMode(mode: PanelHeightMode) {
    panelHeightModeRequest += 1;
    panelHeightMode = mode;
  }
  function handlePanelResizePointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const pointerAt = event.timeStamp;
    const repeatedPress = event.detail > 1 || pointerAt - lastResizeGripPointerAt <= 400;
    lastResizeGripPointerAt = repeatedPress ? Number.NEGATIVE_INFINITY : pointerAt;
    if (repeatedPress) {
      const activeResize = panelResizeOperation;
      void (async () => {
        if (activeResize) await activeResize;
        await changePanelHeightMode('automatic');
      })();
      return;
    }
    const operation = (async () => {
      try {
        await panelHeightModeMutation;
        const edge = await beginPanelResize();
        resizeEdge = edge;
        // The native begin command has already persisted the current height as manual. Mirroring it
        // here stops any in-flight frontend auto-fit without waiting for the first resize event.
        acceptPanelHeightMode('manual');
        // TODO(macOS): Tao 0.35 reports native resize dragging as unsupported and Tauri currently
        // swallows that runtime error. Re-test after Tauri/Tao upgrades; add an AppKit fallback if
        // upstream support is still unavailable.
        await getCurrentWindow().startResizeDragging(edge === 'top' ? 'North' : 'South');
      } catch {
        settingsError = t('app.errors.panelResize');
      } finally {
        await lockPanelResizeAxis().catch(() => undefined);
        updatePanelHeightMode();
      }
    })();
    panelResizeOperation = operation;
    void operation.finally(() => {
      if (panelResizeOperation === operation) panelResizeOperation = null;
    });
  }
  function handleFloatingWindowPointerDown(event: PointerEvent) {
    if (event.button !== 0 || !('__TAURI_INTERNALS__' in window)) return;
    event.preventDefault();
    void getCurrentWindow()
      .startDragging()
      .catch(() => (settingsError = t('app.errors.windowMove')));
  }
  async function changePanelHeightMode(mode: PanelHeightMode) {
    if (!('__TAURI_INTERNALS__' in window)) return;
    const request = ++panelHeightModeRequest;
    const operation = panelHeightModeMutation.then(() =>
      mode === 'automatic' ? setPanelHeightAutomatic() : setPanelHeightManual(),
    );
    panelHeightModeMutation = operation.catch(() => undefined);
    try {
      await operation;
      if (request === panelHeightModeRequest) updatePanelHeightMode();
    } catch {
      if (request !== panelHeightModeRequest) return;
      settingsError = t('app.errors.panelHeightMode');
      updatePanelHeightMode();
    }
  }
  async function requestNotifications() {
    if (!settingsState) return;
    try {
      const permissionState = await requestNotificationPermission();
      settingsController.acceptExternalState(permissionState);
    } catch {
      settingsError = t('app.errors.notificationPermission');
    }
  }
  async function openNotificationSettings() {
    try {
      await openSystemNotificationSettings();
    } catch {
      settingsError = t('app.errors.notificationSettings');
    }
  }
  async function checkForUpdates(manual = false) {
    if (!settingsState) return;
    await updates.check(
      manual,
      (checkedAt) => {
        if (!settingsState) return;
        saveSettings({ ...settingsState.settings, lastUpdateCheckAt: checkedAt });
      },
      showConfirmation,
    );
  }

  onMount(() => {
    const motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    const updateMotionPreference = () => {
      systemReducedMotion = motionQuery.matches;
      scheduleWindowFit();
    };
    updateMotionPreference();
    motionQuery.addEventListener('change', updateMotionPreference);
    const refreshWindowState = () => {
      void settingsController.refreshIfIdle();
      if (!isSettingsWindow && screen === 'dashboard' && selectedProviderId)
        void refreshProviderIfDue(selectedProviderId);
      updatePanelResizeEdge();
      updatePanelHeightMode();
      scheduleWindowFit();
    };
    updatePanelResizeEdge();
    updatePanelHeightMode();
    window.addEventListener('focus', refreshWindowState);

    const popover = document.querySelector<HTMLElement>('.popover');
    const resizeObserver =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(scheduleWindowFit);
    const observePanelParts = () => {
      resizeObserver?.disconnect();
      document
        .querySelectorAll<HTMLElement>(
          '.floating-chrome, .screen-page, .screen-header, .footer, .notice',
        )
        .forEach((element) => resizeObserver?.observe(element));
      scheduleWindowFit();
    };
    const mutationObserver = new MutationObserver(observePanelParts);
    if (popover) {
      mutationObserver.observe(popover, { childList: true, subtree: true, characterData: true });
    }
    observePanelParts();
    const handleKeydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        requestLeaveSettings();
      } else if (event.key === 'Enter' && screen === 'dashboard' && !ownsEnterKey(event.target)) {
        event.preventDefault();
        navigate('customize');
      } else if ((event.ctrlKey || event.metaKey) && event.key === ',') {
        event.preventDefault();
        if (screen !== 'dashboard') requestLeaveSettings();
        else navigate('settings');
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'r') {
        event.preventDefault();
        void refresh();
      } else if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === 'z' &&
        !(event.target instanceof HTMLInputElement) &&
        !(event.target instanceof HTMLTextAreaElement)
      ) {
        event.preventDefault();
        undoCustomization();
      } else if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === 'q' &&
        !(event.target instanceof HTMLInputElement) &&
        !(event.target instanceof HTMLTextAreaElement)
      ) {
        event.preventDefault();
        quitApp();
      }
    };
    document.addEventListener('keydown', handleKeydown);
    const clock = window.setInterval(() => (now = Date.now()), 30_000);
    const listeners = createListenerRegistry(() => {
      settingsError ??= t('app.errors.eventBridge');
    });
    listeners.add(onUsageState((state) => (viewState = state)));
    listeners.add(
      onSettingsState((state) => {
        settingsController.acceptExternalState(state);
      }),
    );
    listeners.add(
      onRequestLeaveSettings(() => {
        if (!isSettingsWindow) return;
        requestLeaveSettings();
      }),
    );
    listeners.add(
      onOpenScreen((target) => {
        if (!isSettingsWindow && target !== 'dashboard') {
          void openSettingsWindow(target).catch(
            () => (settingsError = t('app.errors.backendUnavailable')),
          );
          return;
        }
        if (target === 'dashboard') {
          if (isSettingsWindow) return;
          if (requiresLeaveSettingsConfirmation()) leaveSettingsConfirmationOpen = true;
          else {
            navigate('dashboard');
            if (screen === 'dashboard' && selectedProviderId)
              void refreshProviderIfDue(selectedProviderId);
          }
        }
      }),
    );
    listeners.add(
      onSettingsWorkspaceSelection((target) => {
        if (!isSettingsWindow || target === 'dashboard') return;
        if (target === 'settings' || target === 'customize' || target.startsWith('provider:')) {
          navigate(target as Exclude<Screen, 'dashboard'>);
        }
      }),
    );
    listeners.add(
      onTaskbandOpen((providerId) => {
        if (!isSettingsWindow) void focusTaskbandProvider(providerId);
      }),
    );
    listeners.add(
      onMainWindowHidden(() => {
        if (isSettingsWindow) return;
        taskbandProviderId = null;
        resetTransientUi();
        navigate('dashboard');
      }),
    );
    listeners.add(
      onUpdateProgress((progress) => {
        updates.setProgress(progress);
      }),
    );
    void getBootstrapState()
      .then((state) => {
        catalog = new ProviderCatalogIndex(state.catalog);
        viewState = state.usage;
        settingsController.setState(state.settings);
        automaticUpdatesReady = true;
      })
      .catch(() => (settingsError = t('app.errors.backendUnavailable')));
    return () => {
      document.removeEventListener('keydown', handleKeydown);
      window.clearInterval(clock);
      windowController.dispose();
      motionQuery.removeEventListener('change', updateMotionPreference);
      window.removeEventListener('focus', refreshWindowState);
      document.documentElement.removeAttribute('data-reduced-motion');
      mutationObserver.disconnect();
      resizeObserver?.disconnect();
      listeners.dispose();
    };
  });
</script>

<svelte:head><meta name="color-scheme" content="light dark" /></svelte:head>
<svelte:window onpointerdown={handleWindowPointerDown} />

<main
  class="popover"
  class:popover--floating={floatingWindow}
  class:popover--macos={floatingWindow && platform === 'macos'}
  aria-label={isSettingsWindow ? $tStore('settings.title') : $tStore('app.usageDashboard')}
  oncontextmenu={(event) => event.preventDefault()}
>
  <p id="reorder-instructions" class="sr-only">
    {$tStore('app.reorderInstructions')}
  </p>
  {#if renderedResizeEdge === 'top'}
    <div
      class="panel-resize-dragger panel-resize-dragger--top"
      role="separator"
      aria-label={$tStore('app.resizePanelHeight')}
      aria-orientation="horizontal"
      onpointerdown={handlePanelResizePointerDown}
    ></div>
  {/if}
  {#if floatingWindow}
    <header class="floating-chrome" aria-label={$tStore('app.windowControls')}>
      <div class="floating-chrome__drag">
        <Quota01Mark size={14} />
        <span>Quota01</span>
      </div>
      <button
        class="floating-chrome__close"
        type="button"
        aria-label={settingsState?.trayAvailable
          ? $tStore('app.hideQuota01')
          : $tStore('app.closeQuota01')}
        onclick={closeMainWindow}
      >
        <Icon name="close" size={12} strokeWidth={2.1} />
      </button>
    </header>
  {/if}
  {#if settingsState}
    <div
      class="content"
      class:content--chrome={isSettingsWindow}
      class:content--dashboard={!isSettingsWindow}
    >
      {#if settingsError}<div class="notice notice--blocking" role="alert">
          {$tBackendStore(settingsError)}
        </div>{/if}
      {#if screen === 'dashboard'}
        {#if selectedProviderId}
          <ProviderRail
            {viewState}
            settings={settingsState.settings}
            {catalog}
            {selectedProviderId}
            onSelect={selectDashboardProvider}
          />
        {/if}
      {/if}
      <div class="screen-stage">
        <div class="screen-page" data-screen={screen}>
          {#if isSettingsWindow}
            {#key settingsWorkspaceRequest.revision}
              <SettingsWorkspace
                settingsView={settingsState}
                {dashboardProps}
                {platform}
                {panelHeightMode}
                onPanelHeightModeChange={(mode) => void changePanelHeightMode(mode)}
                onRequestNotifications={requestNotifications}
                onOpenNotificationSettings={openNotificationSettings}
                checkingUpdate={updates.checking}
                onCheckForUpdates={() => void checkForUpdates(true)}
                onCopyLogPath={copyLogPath}
                onOpenLogFolder={openLogFolder}
                onResetAllSettings={() => (settingsResetConfirmationOpen = true)}
                onResetAllCustomization={requestCustomizationReset}
                onResetProviderCustomization={resetProviderCustomization}
                {resettingProviderId}
                initialProviderId={settingsWorkspaceRequest.target.startsWith('provider:')
                  ? settingsWorkspaceRequest.target.slice(9)
                  : settingsWorkspaceRequest.target === 'customize'
                    ? currentProviderId
                    : null}
                initialGeneral={settingsWorkspaceRequest.target === 'settings'}
              />
            {/key}
          {:else}
            <Dashboard
              {...dashboardProps}
              focusedProviderId={null}
              showGlobalContent={true}
              showProviderContent={false}
            />
            {#if selectedProviderId}
              <ProviderDataView
                {...dashboardProps}
                providerId={selectedProviderId}
                readOnlyPreview={false}
              />
            {/if}
          {/if}
        </div>
      </div>
    </div>

    {#if screen === 'dashboard' || screen === 'settings'}
      <footer class="footer">
        <button
          class="identity"
          type="button"
          onclick={refresh}
          disabled={anyRefreshing}
          aria-label={$tStore('app.refreshAll')}
        >
          <span>Quota01 {appVersion}</span><small
            >{anyRefreshing ? $tStore('app.updating') : nextUpdateLabelText}</small
          >
        </button>
        {#if screen === 'dashboard'}
          <div class="footer-actions">
            <button
              class="footer-icon-button"
              type="button"
              aria-label={$tStore('settings.openSettings')}
              title={$tStore('settings.openSettings')}
              onclick={() => navigate('settings')}
              ><Icon name="gear" size={15} strokeWidth={1.9} /></button
            >
            <button
              class="footer-icon-button"
              type="button"
              aria-label={$tStore('app.shareScreenshot')}
              title={$tStore('app.shareScreenshot')}
              disabled={!selectedProviderId}
              onclick={() => selectedProviderId && void shareProvider(selectedProviderId)}
              ><Icon name="share" size={15} strokeWidth={1.9} /></button
            >
          </div>
        {/if}
      </footer>
    {/if}

    {#if confirmationMessage}
      <div class="transient-pill" role="status">
        <Icon name="check" size={15} strokeWidth={2.4} />{confirmationMessage}
      </div>
    {/if}

    {#if resetConfirmationOpen}
      <ConfirmationSheet
        title={t('app.confirmations.resetAllCustomization.title')}
        message={t('app.confirmations.resetAllCustomization.message')}
        confirmLabel={t('app.confirmations.resetAllCustomization.confirm')}
        pending={resettingCustomization}
        onConfirm={() => void confirmCustomizationReset()}
        onCancel={() => (resetConfirmationOpen = false)}
      />
    {/if}

    {#if settingsResetConfirmationOpen}
      <ConfirmationSheet
        title={t('app.confirmations.resetAllSettings.title')}
        message={t('app.confirmations.resetAllSettings.message')}
        confirmLabel={t('app.confirmations.resetAllSettings.confirm')}
        pending={resettingAllSettings}
        onConfirm={() => void confirmAllSettingsReset()}
        onCancel={() => (settingsResetConfirmationOpen = false)}
      />
    {/if}

    {#if leaveSettingsConfirmationOpen}
      <ConfirmationSheet
        title={t('app.confirmations.leaveSettings.title')}
        message={t('app.confirmations.leaveSettings.message')}
        confirmLabel={t('app.confirmations.leaveSettings.confirm')}
        pending={leavingSettings}
        onConfirm={() => void confirmLeaveSettings()}
        onCancel={cancelLeaveSettings}
      />
    {/if}

    {#if renameCard}
      <RenameProviderSheet
        initialValue={renameCard.initialValue}
        onRename={renameProvider}
        onCancel={() => void closeRenameProvider()}
      />
    {/if}
  {:else}
    <div class="content">
      {#if settingsError}
        <div class="notice notice--blocking" role="alert">{$tBackendStore(settingsError)}</div>
      {:else}
        <p class="empty-row">{$tStore('app.loading')}</p>
      {/if}
    </div>
  {/if}
  {#if renderedResizeEdge === 'bottom'}
    <div
      class="panel-resize-dragger panel-resize-dragger--bottom"
      role="separator"
      aria-label={$tStore('app.resizePanelHeight')}
      aria-orientation="horizontal"
      onpointerdown={handlePanelResizePointerDown}
    ></div>
  {/if}
</main>

<style>
  :global {
    .popover {
      position: relative;
      display: flex;
      width: 100%;
      height: 100%;
      flex-direction: column;
      overflow: hidden;
      color: var(--text);
      background: var(--tray);
      isolation: isolate;
      user-select: none;
    }

    .floating-chrome {
      position: relative;
      z-index: 30;
      display: grid;
      width: 100%;
      height: 32px;
      flex: 0 0 32px;
      grid-template-columns: 32px 1fr 32px;
      align-items: center;
      border-bottom: 1px solid var(--separator);
      background: color-mix(in srgb, var(--text) 3%, var(--tray));
    }

    .floating-chrome__drag {
      grid-row: 1;
      grid-column: 1 / -1;
      display: flex;
      height: 100%;
      align-items: center;
      justify-content: center;
      gap: 6px;
      color: var(--secondary);
      cursor: grab;
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.01em;
      touch-action: none;
    }

    .floating-chrome__drag:active {
      cursor: grabbing;
    }

    .floating-chrome__drag > * {
      pointer-events: none;
    }

    .floating-chrome__close {
      position: relative;
      z-index: 1;
      display: grid;
      width: 24px;
      height: 24px;
      grid-row: 1;
      grid-column: 3;
      align-items: center;
      justify-self: center;
      padding: 0;
      border: 0;
      border-radius: 7px;
      color: var(--secondary);
      background: transparent;
      cursor: default;
      place-items: center;
      transition:
        color 120ms ease,
        background-color 120ms ease,
        transform 80ms ease;
    }

    .popover--macos .floating-chrome__close {
      grid-column: 1;
    }

    .floating-chrome__close:hover {
      color: var(--text);
      background: color-mix(in srgb, var(--text) 9%, transparent);
    }

    .floating-chrome__close:active {
      transform: scale(0.92);
    }

    .floating-chrome__close:focus-visible {
      outline: 2px solid color-mix(in srgb, var(--meter-fill) 55%, transparent);
      outline-offset: -1px;
    }

    .panel-resize-dragger {
      position: relative;
      z-index: 20;
      display: grid;
      width: 100%;
      height: 10px;
      flex: 0 0 10px;
      cursor: ns-resize;
      touch-action: none;
      place-items: center;
    }

    .panel-resize-dragger::after {
      width: 36px;
      height: 4px;
      border-radius: 999px;
      background: var(--separator);
      content: '';
      transition:
        width 120ms ease,
        background-color 120ms ease;
    }

    .panel-resize-dragger:hover::after {
      width: 42px;
      background: var(--tertiary);
    }

    .panel-resize-dragger--top {
      box-shadow: 0 10px 18px -20px rgba(0, 0, 0, 0.7);
    }

    .panel-resize-dragger--bottom {
      box-shadow: 0 -10px 18px -20px rgba(0, 0, 0, 0.7);
    }

    .content {
      flex: 1;
      min-height: 0;
      padding: 14px 14px 12px;
      overflow-y: auto;
      scrollbar-width: none;
      overflow-x: hidden;
    }

    .content::-webkit-scrollbar {
      width: 0;
      height: 0;
    }

    .footer {
      display: flex;
      min-height: 58px;
      align-items: center;
      gap: 12px;
      padding: 10px 14px;
      border-top: 1px solid var(--separator);
      background: color-mix(in srgb, var(--tray) 92%, transparent);
    }

    .identity {
      display: flex;
      flex-direction: column;
      color: var(--secondary);
      font-size: 10px;
      line-height: 14px;
    }

    .identity small {
      color: var(--tertiary);
      font: inherit;
    }

    .screen-header {
      display: grid;
      min-height: 30px;
      align-items: center;
      grid-template-columns: 54px 1fr 54px;
      margin-bottom: 8px;
    }

    .screen-header__title {
      margin: 0;
      font-size: 14px;
      text-align: center;
    }

    .screen-header button {
      width: fit-content;
      padding: 3px 7px;
      border: 0;
      border-radius: 6px;
      color: var(--secondary);
      background: transparent;
      cursor: pointer;
    }

    .screen-header > button:first-child {
      font-size: 23px;
      line-height: 20px;
    }

    .screen-header .text-button {
      justify-self: end;
      color: var(--meter-fill);
      font-size: 10px;
    }

    .screen-header button:hover {
      background: var(--button-hover);
    }

    :root[data-density='compact'] .content {
      padding: 9px 11px 7px;
    }

    :root[data-density='compact'] .footer {
      min-height: 48px;
      padding-top: 6px;
      padding-bottom: 6px;
    }

    .content {
      padding: 14px 14px 12px;
      scrollbar-width: none;
    }

    .content--chrome {
      display: flex;
      flex-direction: column;
      padding: 0;
      overflow: hidden;
    }

    .content--chrome > .screen-stage {
      flex: 1 1 auto;
    }

    .content--chrome .screen-page {
      height: 100%;
      align-self: stretch;
    }

    .content--dashboard {
      display: grid;
      min-height: 0;
      grid-template-columns: 92px minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
      padding: 0;
      overflow: hidden;
    }

    :root[data-density='compact'] .content--dashboard {
      padding: 0;
    }

    .content--dashboard > .notice {
      grid-row: 1;
      grid-column: 1 / -1;
      margin: 8px 14px 0;
    }

    .content--dashboard > .provider-rail {
      grid-row: 2;
      grid-column: 1;
    }

    .content--dashboard > .screen-stage {
      min-height: 0;
      grid-row: 2;
      grid-column: 2;
      padding: 14px 14px 12px;
      overflow-y: auto;
      overscroll-behavior: contain;
    }

    .screen-stage {
      display: grid;
      width: 100%;
      min-width: 0;
      min-height: 0;
      overflow: clip;
      isolation: isolate;
      background: var(--tray);
    }

    .screen-page {
      position: relative;
      width: 100%;
      min-width: 0;
      min-height: 0;
      grid-area: 1 / 1;
      align-self: start;
      background: var(--tray);
      transform-origin: 50% 45%;
    }

    .footer {
      min-height: 52px;
      padding: 12px 14px;
      border-top: 0;
      background: color-mix(in srgb, var(--tray) 94%, transparent);
      box-shadow: 0 -10px 18px -18px rgba(0, 0, 0, 0.65);
    }

    .identity {
      padding: 0;
      border: 0;
      color: var(--secondary);
      background: none;
      font-size: 10px;
      line-height: 12px;
      text-align: left;
      cursor: pointer;
    }

    .identity:disabled {
      cursor: default;
    }

    .footer-actions {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-left: auto;
    }

    .footer-icon-button {
      display: grid;
      width: 30px;
      height: 30px;
      flex: 0 0 30px;
      padding: 0;
      border: 0;
      border-radius: 50%;
      color: var(--secondary);
      background: transparent;
      cursor: pointer;
      place-items: center;
      transition:
        color 120ms ease,
        background-color 120ms ease,
        transform 80ms ease;
    }

    .footer-icon-button:hover:not(:disabled) {
      color: var(--text);
      background: var(--button-hover);
    }

    .footer-icon-button:active:not(:disabled) {
      transform: scale(0.92);
    }

    .footer-icon-button:focus-visible {
      outline: 2px solid color-mix(in srgb, var(--meter-fill) 55%, transparent);
      outline-offset: 1px;
    }

    .footer-icon-button:disabled {
      color: var(--tertiary);
      cursor: default;
    }

    .screen-header {
      position: sticky;
      top: 0;
      z-index: 5;
      min-height: 44px;
      grid-template-columns: 44px 1fr 44px;
      margin: 0 -14px 12px;
      padding: 0 14px;
      background: color-mix(in srgb, var(--tray) 94%, transparent);
      box-shadow: 0 10px 18px -20px rgba(0, 0, 0, 0.8);
      backdrop-filter: blur(18px);
    }

    .screen-header__title {
      font-size: 13px;
      font-weight: 600;
    }

    .screen-header button:first-child {
      display: grid;
      width: 28px;
      height: 28px;
      padding: 0;
      border-radius: 50%;
      background: var(--button-hover);
      place-items: center;
    }

    .screen-header .text-button {
      width: 28px;
      height: 28px;
      overflow: hidden;
      color: var(--secondary);
      font-size: inherit;
    }

    .screen-header .text-button::after {
      content: none;
    }

    .transient-pill {
      position: absolute;
      right: 14px;
      bottom: 62px;
      z-index: 90;
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 7px 10px;
      border: 1px solid var(--separator);
      border-radius: 999px;
      color: var(--text);
      background: color-mix(in srgb, var(--tray) 96%, transparent);
      box-shadow: 0 8px 24px rgba(0, 0, 0, 0.22);
      font-size: 10px;
      animation: detail-in var(--motion-spring) both;
    }

    .transient-pill .symbol-icon {
      color: #34c759;
    }

    :root[data-density='compact'] .content {
      padding: 10px 14px 8px;
    }

    :root[data-density='compact'] .content--chrome {
      padding: 0;
    }

    .notice--blocking {
      color: var(--error);
      background: var(--error-bg);
    }
  }
</style>
