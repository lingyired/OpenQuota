<script lang="ts">
  import type { DashboardProps } from './Dashboard.svelte';
  import CustomizeProviderDetail from './CustomizeProviderDetail.svelte';
  import CustomizeProviderList from './CustomizeProviderList.svelte';
  import Icon from './Icon.svelte';
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
    onResetAllCustomization: () => void;
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
    onResetAllCustomization,
    onResetProviderCustomization,
    resettingProviderId,
  }: Props = $props();
  let selectedProviderId = $state<string | null>(null);
  let generalSelected = $state(false);
  let initializedSelection = false;
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
  const previewProviderId = $derived(generalSelected ? null : selectedProviderId);
  const previewProviderName = $derived(
    previewProviderId
      ? dashboardProps.catalog.displayName(previewProviderId, settingsView.settings.providerNames)
      : '',
  );
  const selectedProviderName = $derived(
    selectedProvider
      ? dashboardProps.catalog.displayName(selectedProvider.id, settingsView.settings.providerNames)
      : '',
  );
  const settingsScreenProps = $derived({
    settingsView,
    platform,
    panelHeightMode,
    onChange: dashboardProps.onSettingsChange,
    onPanelHeightModeChange,
    onRequestNotifications,
    onOpenNotificationSettings,
    updateError: dashboardProps.updateError,
    checkingUpdate,
    onCheckForUpdates,
    onCopyLogPath,
    onOpenLogFolder,
    onResetAllSettings,
    onResetAllCustomization,
  });

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
  }

  function selectGeneralSettings() {
    generalSelected = true;
    selectedProviderId = null;
  }
</script>

<section class="settings-workspace" aria-label={$tStore('settings.title')} data-settings-workspace>
  <nav
    class="settings-workspace__column settings-workspace__providers"
    data-workspace-panel="providers"
    aria-label={$tStore('settings.title')}
  >
    <header class="settings-workspace__column-header">
      <h1 class="settings-workspace__column-title">{$tStore('settings.title')}</h1>
    </header>
    <CustomizeProviderList
      settings={settingsView.settings}
      catalog={dashboardProps.catalog}
      workspace={true}
      {selectedProviderId}
      {generalSelected}
      providerInstanceCount={settingsView.providerInstanceCount}
      providerInstanceFailures={settingsView.providerInstanceFailures}
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
    data-workspace-panel="settings"
    aria-label={generalSelected ? $tStore('settings.preferences') : selectedProviderName}
  >
    <header class="settings-workspace__column-header">
      <h2 class="settings-workspace__column-title">
        {generalSelected ? $tStore('settings.preferences') : selectedProviderName}
      </h2>
      {#if !generalSelected && selectedProvider}
        <button
          class="settings-workspace__header-action"
          type="button"
          disabled={resettingProviderId !== null}
          aria-label={$tStore('app.resetProvider', { name: selectedProviderName })}
          title={$tStore('app.resetProvider', { name: selectedProviderName })}
          onclick={() => onResetProviderCustomization(selectedProvider.id)}
          ><Icon name="reset" size={16} strokeWidth={2} /></button
        >
      {/if}
    </header>
    {#if generalSelected}
      <SettingsScreen {...settingsScreenProps} region="primary" />
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
    data-workspace-panel="preview"
    aria-label={generalSelected
      ? `${$tStore('settings.advanced')} · ${$tStore('settings.updates')}`
      : $tStore('settings.usagePreview')}
  >
    <header class="settings-workspace__column-header">
      <h2 class="settings-workspace__column-title">
        {generalSelected
          ? `${$tStore('settings.advanced')} · ${$tStore('settings.updates')}`
          : $tStore('settings.usagePreview')}
      </h2>
      {#if !generalSelected && previewProviderId}
        <button
          class="settings-workspace__header-action"
          type="button"
          aria-label={$tStore('dashboard.refreshProvider', { provider: previewProviderName })}
          title={$tStore('dashboard.refreshProvider', { provider: previewProviderName })}
          onclick={() => dashboardProps.onRefresh(previewProviderId)}
          ><Icon name="refresh" size={16} strokeWidth={2} /></button
        >
      {/if}
    </header>
    {#if generalSelected}
      <SettingsScreen {...settingsScreenProps} region="advanced" />
    {:else if previewProviderId}
      <div class="settings-workspace__preview-body" data-provider-preview>
        <ProviderDataView
          {...dashboardProps}
          settings={settingsView.settings}
          providerId={previewProviderId}
          readOnlyPreview={true}
        />
      </div>
    {:else}
      <div class="settings-workspace__general-preview">
        <p>{$tStore('settings.general')}</p>
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
    grid-template-columns:
      minmax(210px, min(280px, 24vw))
      minmax(320px, 1.12fr)
      minmax(360px, 1.2fr);
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

  .settings-workspace__providers {
    grid-column: 1;
    grid-row: 1;
  }

  .settings-workspace__settings {
    grid-column: 2;
    grid-row: 1;
  }

  .settings-workspace__preview {
    grid-column: 3;
    grid-row: 1;
  }

  .settings-workspace__column-header {
    display: flex;
    min-height: 54px;
    flex: 0 0 54px;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-inline: 16px;
    border-bottom: 1px solid var(--separator);
  }

  .settings-workspace__column-title {
    flex: 1 1 auto;
    margin: 0;
    min-width: 0;
    color: var(--text);
    font-size: 14px;
    font-weight: 650;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .settings-workspace__header-action {
    display: grid;
    width: 32px;
    height: 32px;
    flex: 0 0 auto;
    padding: 0;
    border: 0;
    border-radius: 8px;
    color: var(--secondary);
    background: transparent;
    cursor: pointer;
    place-items: center;
  }

  .settings-workspace__header-action:hover:not(:disabled) {
    color: var(--text);
    background: var(--button-hover);
  }

  .settings-workspace__header-action:focus-visible {
    outline: 2px solid var(--meter-fill);
    outline-offset: 2px;
  }

  .settings-workspace__header-action:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .settings-workspace__providers :global(.customize-screen),
  .settings-workspace__settings-detail,
  .settings-workspace__settings :global(.settings-screen),
  .settings-workspace__preview :global(.settings-screen),
  .settings-workspace__preview-body {
    min-width: 0;
    min-height: 0;
    flex: 1 1 auto;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-color: var(--separator) transparent;
    scrollbar-width: thin;
  }

  .settings-workspace__preview-body {
    overflow-x: hidden;
    overflow-y: auto;
  }

  .settings-workspace__providers :global(.customize-screen),
  .settings-workspace__settings :global(.settings-screen),
  .settings-workspace__preview :global(.settings-screen),
  .settings-workspace__preview-body,
  .settings-workspace__settings-detail,
  .settings-workspace__general-preview {
    padding: 10px;
  }

  .settings-workspace__general-preview {
    flex: 1 1 auto;
    color: var(--secondary);
    font-size: 12px;
    line-height: 1.5;
  }

  @media (max-width: 900px) {
    .settings-workspace {
      grid-template-columns: minmax(168px, 24vw) minmax(250px, 1fr) minmax(260px, 1.05fr);
    }

    .settings-workspace__column-header {
      padding-inline: 12px;
    }
  }

  @media (max-width: 720px) {
    .settings-workspace {
      grid-template-columns: minmax(152px, 23vw) minmax(220px, 1fr) minmax(230px, 1.05fr);
    }

    .settings-workspace__providers :global(.customize-screen),
    .settings-workspace__settings :global(.settings-screen),
    .settings-workspace__preview :global(.settings-screen),
    .settings-workspace__preview-body,
    .settings-workspace__settings-detail,
    .settings-workspace__general-preview {
      padding: 8px;
    }
  }
</style>
