<script lang="ts">
  import { onDestroy } from 'svelte';
  import { flip } from 'svelte/animate';
  import { locale } from 'svelte-i18n';
  import type { ProviderCatalogIndex } from './metrics';
  import { desktopPlatform } from './platform';
  import { t, tBackend, tStore } from './i18n';
  import type {
    AppSettings,
    MetricLayout,
    MetricSection,
    ProviderLayout,
    TaskbandLayout,
  } from './types';
  import Icon from './Icon.svelte';
  import ProviderApiKeySection from './ProviderApiKeySection.svelte';
  import ProviderDeviceCodeLogin from './ProviderDeviceCodeLogin.svelte';
  import ProviderSessionSection from './ProviderSessionSection.svelte';
  import ProviderNameSection from './ProviderNameSection.svelte';
  import { reorderFlip } from './motion';
  import { pointerReorder } from './pointerReorder';
  import { canRenameProvider } from './providerNames';

  interface Props {
    settings: AppSettings;
    providerId: string;
    catalog: ProviderCatalogIndex;
    renamableProviderIds: string[];
    onChange: (settings: AppSettings) => void;
    onNameChange: (settings: AppSettings) => void;
    onReorderStart: () => void;
    onReorderEnd: (moved: boolean, cancelled?: boolean) => void;
    reducedMotion: boolean;
  }
  let {
    settings,
    providerId,
    catalog,
    renamableProviderIds,
    onChange,
    onNameChange,
    onReorderStart,
    onReorderEnd,
    reducedMotion,
  }: Props = $props();
  const metricDefinition = (id: string) => catalog.metric(id);
  const providerDisplayName = (id: string) => catalog.displayName(id, settings.providerNames);
  const currentLocale = $derived(locale);
  const metricLabel = $derived((id: string) => {
    void currentLocale;
    return tBackend(metricDefinition(id)?.label ?? id);
  });
  let message = $state('');
  let messageKind = $state<'success' | 'denied'>('success');
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => {
    if (messageTimer) clearTimeout(messageTimer);
  });
  const provider = $derived(settings.providers.find((item) => item.id === providerId));

  function updateProvider(next: ProviderLayout) {
    onChange({
      ...settings,
      providers: settings.providers.map((item) => (item.id === next.id ? next : item)),
    });
  }
  function updateMetric(metric: MetricLayout) {
    if (!provider) return;
    updateProvider({
      ...provider,
      metrics: provider.metrics.map((item) => (item.id === metric.id ? metric : item)),
    });
  }
  function updateProviderProxy(useProxy: boolean) {
    if (!provider) return;
    updateProvider({ ...provider, useProxy });
  }
  function togglePin(metric: MetricLayout, button: HTMLButtonElement) {
    if (!provider || !metricDefinition(metric.id)?.pinnable) return;
    if (!metric.pinned && provider.metrics.filter((item) => item.pinned).length >= 2) {
      showMessage(t('customize.upTo2Stars'), 'denied');
      if (!reducedMotion) {
        button.animate?.(
          [
            { transform: 'translateX(0)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(-5px)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(-5px)' },
            { transform: 'translateX(5px)' },
            { transform: 'translateX(0)' },
          ],
          { duration: 400, delay: 100 },
        );
      }
      return;
    }
    showMessage(
      metric.pinned ? t('customize.removedFromMenuBar') : t('customize.starredForMenuBar'),
      'success',
    );
    updateMetric({ ...metric, pinned: !metric.pinned });
  }
  function showMessage(text: string, kind: 'success' | 'denied') {
    message = text;
    messageKind = kind;
    if (messageTimer) clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = ''), 2500);
  }
  function reorder(
    draggedId: string,
    target: MetricLayout,
    section: MetricSection = target.section,
  ) {
    if (!provider || draggedId === target.id) return;
    const metrics = [...provider.metrics];
    const from = metrics.findIndex((metric) => metric.id === draggedId);
    const to = metrics.findIndex((metric) => metric.id === target.id);
    if (from < 0 || to < 0) return;
    const [source] = metrics.splice(from, 1);
    const moved = { ...source, section };
    metrics.splice(to, 0, moved);
    updateProvider({ ...provider, metrics });
  }
  function moveIntoSection(draggedId: string, section: MetricSection) {
    if (!provider) return;
    const metrics = [...provider.metrics];
    const from = metrics.findIndex((metric) => metric.id === draggedId);
    if (from < 0) return;
    const [source] = metrics.splice(from, 1);
    const moved = { ...source, section };
    const lastInSection = metrics.reduce(
      (last, metric, index) => (metric.section === section ? index : last),
      -1,
    );
    const insertAt =
      lastInSection >= 0 ? lastInSection + 1 : section === 'alwaysVisible' ? 0 : metrics.length;
    metrics.splice(insertAt, 0, moved);
    updateProvider({ ...provider, metrics });
  }
  const platform = desktopPlatform();
  const taskband = $derived(settings.taskbandProviders[providerId] ?? null);
  function updateTaskband(value: Partial<TaskbandLayout>) {
    if (!provider) return;
    const existing = taskband ?? {
      enabled: true,
      side: null,
      topColor: null,
      bottomColor: null,
      topBold: false,
      bottomBold: false,
      topSize: 9,
      bottomSize: 9,
      topAlign: 0,
      bottomAlign: 0,
      paddingLeft: 4,
      paddingRight: 4,
    };
    onChange({
      ...settings,
      taskbandProviders: {
        ...settings.taskbandProviders,
        [providerId]: { ...existing, ...value },
      },
    });
  }
</script>

{#if provider}
  <section
    class="screen customize-detail"
    aria-label={$tStore('customize.customizeProvider', { id: providerDisplayName(provider.id) })}
  >
    {#if canRenameProvider(provider.id, renamableProviderIds)}
      <ProviderNameSection {settings} {provider} {catalog} onChange={onNameChange} />
    {/if}
    <div class="taskband-section proxy-setting-section">
      <div class="taskband-row proxy-setting-row">
        <span class="taskband-row-label">
          <b>{$tStore('customize.useProxy')}</b>
          <small>{$tStore('customize.useProxyHelp')}</small>
        </span>
        <label class="switch">
          <input
            type="checkbox"
            aria-label={$tStore('customize.useProxy')}
            checked={provider.useProxy}
            onchange={(event) => updateProviderProxy(event.currentTarget.checked)}
          /><span></span>
        </label>
      </div>
    </div>
    {#each ['alwaysVisible', 'onDemand'] as section (section)}
      {@const sectionMetrics = provider.metrics.filter((metric) => metric.section === section)}
      <div
        class="metric-section"
        role="group"
        aria-label={$tStore(
          section === 'alwaysVisible'
            ? 'customize.alwaysVisibleMetrics'
            : 'customize.onDemandMetrics',
        )}
      >
        <h2>
          {$tStore(section === 'alwaysVisible' ? 'customize.alwaysVisible' : 'customize.onDemand')}
        </h2>
        <div class="metric-list" role="list">
          {#if sectionMetrics.length === 0}
            <div
              class="empty-drop-zone"
              role="listitem"
              data-reorder-group={`customize-metrics:${provider.id}`}
              data-reorder-id={`section:${section}`}
            >
              {$tStore('customize.dragMetricsHere')}
            </div>
          {/if}
          {#each sectionMetrics as metric (metric.id)}
            <div
              role="listitem"
              class:disabled={!metric.enabled}
              class="customize-metric-row"
              data-reorder-group={`customize-metrics:${provider.id}`}
              data-reorder-id={metric.id}
              use:pointerReorder={{
                id: metric.id,
                group: `customize-metrics:${provider.id}`,
                label: metricLabel(metric.id),
                gripOnly: true,
                touchGripOnly: true,
                onReorder: (targetId) => {
                  if (targetId.startsWith('section:')) {
                    moveIntoSection(metric.id, targetId.slice(8) as MetricSection);
                    return;
                  }
                  const target = provider.metrics.find((item) => item.id === targetId);
                  if (target) reorder(metric.id, target, target.section);
                },
                onStart: onReorderStart,
                onEnd: onReorderEnd,
              }}
              animate:flip={reorderFlip(reducedMotion)}
            >
              <span
                class="reorder-grip"
                data-reorder-handle
                data-reorder-touch-handle
                role="button"
                tabindex="0"
                aria-label={$tStore('customize.moveMetric', { label: metricLabel(metric.id) })}
                aria-describedby="reorder-instructions"
                aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown"
                ><Icon name="grip-lines" size={16} strokeWidth={2} /></span
              >
              <span class="customize-metric-name">{metricLabel(metric.id)}</span>
              <span class="customize-metric-pin-slot">
                {#if metricDefinition(metric.id)?.pinnable}<button
                    class:pinned={metric.pinned}
                    class="pin-button"
                    type="button"
                    aria-label={$tStore(
                      metric.pinned ? 'customize.unpinMetric' : 'customize.pinMetric',
                      { label: metricLabel(metric.id) },
                    )}
                    onclick={(event) => togglePin(metric, event.currentTarget)}
                    ><Icon
                      name={metric.pinned ? 'star-filled' : 'star'}
                      size={15}
                      strokeWidth={1.7}
                    /></button
                  >{/if}
              </span>
              <label class="switch"
                ><input
                  aria-label={$tStore('customize.showMetric', { label: metricLabel(metric.id) })}
                  type="checkbox"
                  checked={metric.enabled}
                  onchange={(event) =>
                    updateMetric({
                      ...metric,
                      enabled: event.currentTarget.checked,
                    })}
                /><span></span></label
              >
            </div>
          {/each}
        </div>
      </div>
    {/each}
    {#if platform === 'windows' || platform === 'macos'}
      <div
        class="taskband-section"
        role="group"
        aria-label={$tStore(
          platform === 'macos' ? 'customize.menuBarInstance' : 'customize.taskbar',
        )}
      >
        <h2>{$tStore(platform === 'macos' ? 'customize.menuBarInstance' : 'customize.taskbar')}</h2>
        <div class="taskband-row">
          <span class="taskband-row-label"
            ><b
              >{$tStore(
                platform === 'macos' ? 'customize.showOnMenuBar' : 'customize.showOnTaskbar',
              )}</b
            ><small
              >{$tStore(
                platform === 'macos'
                  ? 'customize.showOnMenuBarDesc'
                  : 'customize.showOnTaskbarDesc',
              )}</small
            ></span
          >
          <label class="switch"
            ><input
              type="checkbox"
              aria-label={$tStore(
                platform === 'macos' ? 'customize.showOnMenuBar' : 'customize.showOnTaskbar',
              )}
              checked={taskband?.enabled ?? true}
              onchange={(event) => updateTaskband({ enabled: event.currentTarget.checked })}
            /><span></span></label
          >
        </div>
        <!--
          Style/layout controls for the native instance (position, line colour, bold,
          size, alignment, padding, restore-defaults) are intentionally NOT rendered:
          both surfaces are toggle-only now. The persisted `taskbandProviders` values
          are untouched, so existing per-provider styling survives in storage and
          keeps driving the native renderer.
        -->
      </div>
    {/if}
    {#if catalog.supportsDeviceCodeSignIn(provider.id)}
      <section
        class="session-section"
        aria-label={`${providerDisplayName(provider.id)} ${$tStore('providerSession.title')}`}
      >
        <h2>{$tStore('providerSession.title')}</h2>
        <div class="session-card">
          <ProviderDeviceCodeLogin
            providerId={provider.id}
            providerName={providerDisplayName(provider.id)}
          />
        </div>
      </section>
    {:else if catalog.supportsWebviewAuth(provider.id)}
      <ProviderSessionSection
        providerId={provider.id}
        providerName={providerDisplayName(provider.id)}
      />
    {:else}
      <ProviderApiKeySection
        providerId={provider.id}
        providerName={providerDisplayName(provider.id)}
      />
    {/if}
    {#if message}
      <div class:denied={messageKind === 'denied'} class="customization-pill" role="status">
        <Icon
          name={messageKind === 'denied' ? 'about' : 'check'}
          size={15}
          strokeWidth={2.2}
        />{message}
      </div>
    {/if}
  </section>
{/if}

<style>
  :global {
    .customize-metric-row {
      display: flex;
      min-height: 42px;
      align-items: center;
      gap: 10px;
      padding: 9px 12px;
      border-top: 1px solid var(--separator);
    }

    .customize-metric-row:first-child {
      border-top: 0;
    }

    .customize-metric-row.disabled {
      opacity: 0.55;
    }

    .customize-metric-row > label {
      display: flex;
      min-width: 0;
      flex: 1;
      align-items: center;
      gap: 5px;
      font-size: 12px;
    }

    .pin-button.pinned {
      color: var(--meter-fill);
    }

    .metric-section {
      margin-top: 0;
      margin-bottom: 14px;
    }

    .customize-metric-name {
      min-width: 0;
      flex: 1;
      overflow: hidden;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .customize-metric-pin-slot {
      display: grid;
      width: 25px;
      height: 25px;
      flex: 0 0 25px;
      place-items: center;
    }

    .customize-metric-row > .switch {
      display: block;
      flex: 0 0 28px;
    }

    .empty-drop-zone {
      display: grid;
      height: 30px;
      margin: 8px;
      border: 1px dashed var(--separator);
      border-radius: 8px;
      color: var(--tertiary);
      font-size: 10px;
      place-items: center;
    }

    .customization-pill {
      position: sticky;
      bottom: 8px;
      z-index: 20;
      display: flex;
      width: max-content;
      max-width: calc(100% - 16px);
      align-items: center;
      gap: 6px;
      margin: 8px auto 0;
      padding: 7px 10px;
      border: 1px solid var(--separator);
      border-radius: 999px;
      color: var(--text);
      background: color-mix(in srgb, var(--tray) 96%, transparent);
      box-shadow: 0 8px 22px rgba(0, 0, 0, 0.22);
      font-size: 10px;
      animation: detail-in var(--motion-spring) both;
    }

    .customization-pill .symbol-icon {
      color: #34c759;
    }

    .customization-pill.denied {
      color: var(--warning);
      animation: detail-in var(--motion-spring) both;
    }

    .customization-pill.denied .symbol-icon {
      color: var(--warning);
    }

    .taskband-section {
      margin-top: 0;
      margin-bottom: 14px;
    }

    .proxy-setting-section {
      overflow: hidden;
      border-radius: 12px;
    }

    .proxy-setting-row {
      border-top: 0;
      border-radius: 12px;
    }

    .taskband-row {
      display: flex;
      min-height: 42px;
      align-items: center;
      justify-content: space-between;
      gap: 10px;
      padding: 8px 12px;
      border-top: 1px solid var(--separator);
      background: var(--card);
      font-size: 12px;
    }

    .taskband-row:first-of-type {
      border-top: 0;
      border-radius: 12px 12px 0 0;
    }

    .taskband-row--button:last-child {
      border-radius: 0 0 12px 12px;
    }

    .taskband-row-label {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: 1px;
    }

    .taskband-row-label b {
      font-weight: 550;
    }

    .taskband-row-label small {
      color: var(--secondary);
      font-size: 9px;
      line-height: 12px;
    }

    .taskband-color-control {
      display: flex;
      flex: 0 0 auto;
      align-items: center;
      gap: 6px;
    }

    .taskband-color-input {
      width: 30px;
      height: 26px;
      padding: 1px;
      border: 1px solid var(--separator);
      border-radius: 6px;
      background: var(--card);
      cursor: pointer;
    }

    .taskband-pad-controls {
      display: flex;
      flex: 0 0 auto;
      align-items: center;
      gap: 5px;
    }

    .taskband-restore {
      width: 100%;
      min-height: 28px;
      border: 1px solid var(--separator);
      border-radius: 6px;
      color: var(--text);
      background: var(--tray);
      font-size: 12px;
    }

    .taskband-restore:hover {
      background: var(--button-hover);
    }

    .session-section {
      margin-bottom: 14px;
    }

    .session-section h2 {
      margin: 0 0 8px;
      color: var(--secondary);
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }

    .session-card {
      overflow: hidden;
      border: 1px solid var(--separator);
      border-radius: 12px;
      background: var(--surface);
    }
  }
</style>
