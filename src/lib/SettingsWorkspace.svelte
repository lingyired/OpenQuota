<script lang="ts">
  import type { DashboardProps } from './Dashboard.svelte';
  import CustomizeProviderDetail from './CustomizeProviderDetail.svelte';
  import CustomizeProviderList from './CustomizeProviderList.svelte';
  import ProviderDataView from './ProviderDataView.svelte';
  import SettingsScreen from './SettingsScreen.svelte';
  import { tStore } from './i18n';
  import type { DesktopPlatform } from './platform';
  import type { PanelHeightMode } from './backend';
  import type { SettingsViewState } from './types';

  interface Props {
    settingsView: SettingsViewState;
    dashboardProps: DashboardProps;
    platform: DesktopPlatform;
    panelHeightMode: PanelHeightMode;
    initialProviderId?: string | null;
    initialGeneral?: boolean;
    onPanelHeightModeChange: (mode: PanelHeightMode) => void;
    onRequestNotifications: () => void;
    onOpenNotificationSettings: () => void;
    onCheckForUpdates: () => void;
    checkingUpdate: boolean;
    onCopyLogPath: () => Promise<void>;
    onOpenLogFolder: () => Promise<void>;
    onResetAllSettings: () => void;
    onResetProviderCustomization: (providerId: string) => void;
    resettingProviderId: string | null;
  }

  let {
    settingsView,
    dashboardProps,
    platform,
    panelHeightMode,
    initialProviderId = null,
    initialGeneral = false,
    onPanelHeightModeChange,
    onRequestNotifications,
    onOpenNotificationSettings,
    onCheckForUpdates,
    checkingUpdate,
    onCopyLogPath,
    onOpenLogFolder,
    onResetAllSettings,
    onResetProviderCustomization,
    resettingProviderId,
  }: Props = $props();
  let selectedProviderId = $state<string | null>(null);
  let generalSelected = $state(false);
  let initializedSelection = false;
  let narrowPanel = $state<'settings' | 'preview'>('settings');
  const availableProviders = $derived(
    settingsView.settings.providers.filter((provider) =>
      dashboardProps.catalog.provider(provider.id),
    ),
  );
  const enabledProviderIds = $derived(
    availableProviders.filter((provider) => provider.enabled).map((provider) => provider.id),
  );
  const selectedProvider = $derived(
    availableProviders.find((provider) => provider.id === selectedProviderId) ?? null,
  );
  const selectedProviderName = $derived(
    selectedProvider
      ? dashboardProps.catalog.displayName(selectedProvider.id, settingsView.settings.providerNames)
      : '',
  );

  $effect(() => {
    if (!initializedSelection) {
      selectedProviderId = initialProviderId;
      generalSelected = initialGeneral;
      initializedSelection = true;
    }
    if (generalSelected) return;
    if (
      selectedProviderId &&
      availableProviders.some((provider) => provider.id === selectedProviderId)
    )
      return;
    selectedProviderId = enabledProviderIds[0] ?? availableProviders[0]?.id ?? null;
    if (selectedProviderId === null) generalSelected = true;
  });

  function selectProvider(providerId: string) {
    generalSelected = false;
    selectedProviderId = providerId;
    narrowPanel = 'settings';
  }

  function selectGeneralSettings() {
    generalSelected = true;
    selectedProviderId = null;
    narrowPanel = 'settings';
  }

  function openFirstProvider() {
    const firstProviderId = enabledProviderIds[0] ?? availableProviders[0]?.id;
    if (firstProviderId) selectProvider(firstProviderId);
  }
</script>

<section class="settings-workspace" aria-label={$tStore('settings.title')} data-settings-workspace>
  <div class="settings-workspace__narrow-tabs" role="group" aria-label={$tStore('settings.title')}>
    <button
      type="button"
      aria-pressed={narrowPanel === 'settings'}
      onclick={() => (narrowPanel = 'settings')}
    >
      {$tStore('settings.title')}
    </button>
    <button
      type="button"
      aria-pressed={narrowPanel === 'preview'}
      onclick={() => (narrowPanel = 'preview')}
    >
      {$tStore('app.usageDashboard')}
    </button>
  </div>

  <nav
    class="settings-workspace__column settings-workspace__providers"
    data-workspace-column="providers"
    aria-label={$tStore('customize.title')}
  >
    <h2 class="settings-workspace__column-title">
      {$tStore('customize.title')}
    </h2>
    <CustomizeProviderList
      settings={settingsView.settings}
      catalog={dashboardProps.catalog}
      workspace={true}
      {selectedProviderId}
      {generalSelected}
      onSelect={selectProvider}
      onGeneralSettings={selectGeneralSettings}
      onOpen={() => {}}
      onChange={dashboardProps.onCustomizationChange}
      onReorderStart={dashboardProps.onReorderStart}
      onReorderEnd={dashboardProps.onReorderEnd}
      onSettings={selectGeneralSettings}
      reducedMotion={dashboardProps.reducedMotion}
    />
  </nav>

  <section
    class="settings-workspace__column settings-workspace__settings"
    class:settings-workspace__column--narrow-active={narrowPanel === 'settings'}
    data-workspace-column="settings"
    aria-label={generalSelected ? $tStore('settings.title') : $tStore('customize.title')}
  >
    <h2 class="settings-workspace__column-title">
      {generalSelected
        ? $tStore('settings.title')
        : $tStore('customize.customizeProvider', { id: selectedProviderName })}
    </h2>
    <p class="settings-workspace__instance-status" aria-live="polite">
      {$tStore('customize.nativeInstanceCount', { count: settingsView.providerInstanceCount })}
      {#each settingsView.providerInstanceFailures as failure (failure)}
        <span class="settings-workspace__instance-failure">{failure}</span>
      {/each}
    </p>
    {#if !generalSelected && selectedProvider}
      <button
        class="settings-workspace__reset-provider"
        type="button"
        disabled={resettingProviderId !== null}
        aria-label={$tStore('app.resetProvider', { name: selectedProviderName })}
        onclick={() => onResetProviderCustomization(selectedProvider.id)}
      >
        {$tStore('app.resetProvider', { name: selectedProviderName })}
      </button>
    {/if}
    {#if generalSelected}
      <SettingsScreen
        {settingsView}
        {platform}
        {panelHeightMode}
        onChange={dashboardProps.onSettingsChange}
        {onPanelHeightModeChange}
        {onRequestNotifications}
        {onOpenNotificationSettings}
        updateError={dashboardProps.updateError}
        {checkingUpdate}
        {onCheckForUpdates}
        onCustomize={openFirstProvider}
        {onCopyLogPath}
        {onOpenLogFolder}
        {onResetAllSettings}
      />
    {:else if selectedProvider}
      <div class="settings-workspace__settings-detail">
        {#key selectedProvider.id}
          <CustomizeProviderDetail
            settings={settingsView.settings}
            providerId={selectedProvider.id}
            catalog={dashboardProps.catalog}
            renamableProviderIds={settingsView.renamableProviderIds}
            onChange={dashboardProps.onCustomizationChange}
            onNameChange={dashboardProps.onSettingsChange}
            onReorderStart={dashboardProps.onReorderStart}
            onReorderEnd={dashboardProps.onReorderEnd}
            reducedMotion={dashboardProps.reducedMotion}
          />
        {/key}
      </div>
    {/if}
  </section>

  <section
    class="settings-workspace__column settings-workspace__preview"
    class:settings-workspace__column--narrow-active={narrowPanel === 'preview'}
    data-workspace-column="preview"
    aria-label={$tStore('app.usageDashboard')}
  >
    <h2 class="settings-workspace__column-title">
      {$tStore('app.usageDashboard')}
    </h2>
    {#if generalSelected}
      <div class="settings-workspace__general-preview">
        <p>{$tStore('settings.general')}</p>
      </div>
    {:else if selectedProviderId}
      <div class="settings-workspace__preview-body" data-provider-preview>
        <ProviderDataView
          {...dashboardProps}
          settings={settingsView.settings}
          providerId={selectedProviderId}
          readOnlyPreview={true}
        />
      </div>
    {/if}
  </section>
</section>

<style>
  .settings-workspace {
    display: grid;
    width: 100%;
    min-width: 0;
    min-height: 0;
    height: 100%;
    grid-template-columns: minmax(168px, 0.72fr) minmax(270px, 1.05fr) minmax(360px, 1.5fr);
    grid-template-rows: minmax(0, 1fr);
    gap: 1px;
    overflow: hidden;
    background: var(--separator);
    color: var(--text);
  }

  .settings-workspace__column {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    overflow: hidden;
    background: var(--tray);
  }

  .settings-workspace__column-title {
    flex: 0 0 auto;
    margin: 0;
    padding: 12px 14px 9px;
    border-bottom: 1px solid var(--separator);
    color: var(--secondary);
    font-size: 11px;
    font-weight: 650;
    line-height: 1.3;
    text-wrap: balance;
  }

  .settings-workspace__reset-provider {
    flex: 0 0 auto;
    align-self: flex-end;
    margin: 6px 10px 0;
    padding: 4px 7px;
    border: 1px solid var(--separator);
    border-radius: 6px;
    color: var(--secondary);
    background: transparent;
    font: inherit;
    font-size: 10px;
    cursor: pointer;
  }

  .settings-workspace__instance-status {
    margin: 0;
    padding: 0 10px 8px;
    color: var(--secondary);
    font-size: 10px;
    line-height: 1.4;
  }

  .settings-workspace__instance-failure {
    display: block;
    color: var(--danger, var(--secondary));
  }

  .settings-workspace__providers :global(.customize-screen),
  .settings-workspace__settings-detail,
  .settings-workspace__settings :global(.settings-screen),
  .settings-workspace__preview-body {
    min-width: 0;
    min-height: 0;
    flex: 1 1 auto;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-color: var(--separator) transparent;
    scrollbar-width: thin;
  }

  .settings-workspace__providers :global(.customize-screen),
  .settings-workspace__settings :global(.settings-screen),
  .settings-workspace__settings-detail,
  .settings-workspace__general-preview {
    padding: 10px;
  }

  .settings-workspace__narrow-tabs {
    display: none;
  }

  .settings-workspace__general-preview {
    flex: 1 1 auto;
    color: var(--secondary);
    font-size: 12px;
    line-height: 1.5;
  }

  @media (max-width: 900px) {
    .settings-workspace {
      grid-template-columns: minmax(118px, 30%) minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }

    .settings-workspace__narrow-tabs {
      display: flex;
      grid-column: 2;
      grid-row: 1;
      gap: 5px;
      padding: 6px 8px;
      background: var(--tray);
    }

    .settings-workspace__narrow-tabs button {
      min-height: 32px;
      padding: 5px 9px;
      border: 1px solid var(--separator);
      border-radius: 7px;
      color: var(--secondary);
      background: transparent;
      font: inherit;
      font-size: 10px;
      cursor: pointer;
    }

    .settings-workspace__narrow-tabs button[aria-pressed='true'] {
      color: var(--text);
      background: var(--button-hover);
    }

    .settings-workspace__providers {
      grid-column: 1;
      grid-row: 1 / 3;
    }

    .settings-workspace__settings,
    .settings-workspace__preview {
      display: none;
      grid-column: 2;
      grid-row: 2;
    }

    .settings-workspace__settings.settings-workspace__column--narrow-active,
    .settings-workspace__preview.settings-workspace__column--narrow-active {
      display: flex;
    }
  }
</style>
