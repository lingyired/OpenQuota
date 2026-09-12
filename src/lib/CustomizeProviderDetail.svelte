<script lang="ts">
  import { onDestroy } from 'svelte';
  import { flip } from 'svelte/animate';
  import { locale } from 'svelte-i18n';
  import type { ProviderCatalogIndex } from './metrics';
  import { desktopPlatform } from './platform';
  import SelectMenu from './SelectMenu.svelte';
  import { t, tBackend, tStore } from './i18n';
  import type {
    AppSettings,
    MetricLayout,
    MetricSection,
    ProviderLayout,
    TaskbandColorStyle,
    TaskbandLayout,
    TaskbandSide,
  } from './types';
  import Icon from './Icon.svelte';
  import ProviderApiKeySection from './ProviderApiKeySection.svelte';
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
  function updateTaskbandColor(line: 'topColor' | 'bottomColor', value: string) {
    const color: TaskbandColorStyle | null = value ? { type: 'solid', value } : { type: 'default' };
    updateTaskband({ [line]: color });
  }
  function restoreTaskbandDefaults() {
    const rest = { ...settings.taskbandProviders };
    delete rest[providerId];
    onChange({ ...settings, taskbandProviders: rest });
  }
  function taskbandColor(line: 'top' | 'bottom'): { mode: 'auto' | 'custom'; value: string } {
    const color = line === 'top' ? taskband?.topColor : taskband?.bottomColor;
    if (color?.type === 'solid') return { mode: 'custom', value: color.value };
    return { mode: 'auto', value: '#3b82f6' };
  }
  function clampInt(value: string, min: number, max: number) {
    const parsed = Number.parseInt(value, 10);
    if (Number.isNaN(parsed)) return min;
    return Math.min(max, Math.max(min, parsed));
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
    {#if platform === 'windows'}
      <div class="taskband-section" role="group" aria-label={$tStore('customize.taskbar')}>
        <h2>{$tStore('customize.taskbar')}</h2>
        <div class="taskband-row">
          <span class="taskband-row-label"
            ><b>{$tStore('customize.showOnTaskbar')}</b><small
              >{$tStore('customize.showOnTaskbarDesc')}</small
            ></span
          >
          <label class="switch"
            ><input
              type="checkbox"
              aria-label={$tStore('customize.showOnTaskbar')}
              checked={taskband?.enabled ?? true}
              onchange={(event) => updateTaskband({ enabled: event.currentTarget.checked })}
            /><span></span></label
          >
        </div>
        <div class="taskband-row">
          <span class="taskband-row-label"><b>{$tStore('customize.position')}</b></span><SelectMenu
            label={$tStore('customize.taskbarPosition')}
            value={taskband?.side ?? ''}
            options={[
              { value: '', label: $tStore('customize.followDefault') },
              { value: 'left', label: $tStore('customize.leftStart') },
              { value: 'right', label: $tStore('customize.rightTray') },
            ]}
            onChange={(value) => updateTaskband({ side: value ? (value as TaskbandSide) : null })}
          />
        </div>
        {#snippet taskbandLineStyle(label: string, line: 'top' | 'bottom')}
          <div class="taskband-row">
            <span class="taskband-row-label"
              ><b>{$tStore('customize.lineColor', { label })}</b></span
            >
            <div class="taskband-color-control">
              <SelectMenu
                label={$tStore('customize.lineColor', { label })}
                value={taskbandColor(line).mode}
                options={[
                  { value: 'auto', label: $tStore('customize.auto') },
                  { value: 'custom', label: $tStore('customize.custom') },
                ]}
                onChange={(value) =>
                  updateTaskbandColor(
                    line === 'top' ? 'topColor' : 'bottomColor',
                    value === 'auto' ? '' : taskbandColor(line).value,
                  )}
              />
              {#if taskbandColor(line).mode === 'custom'}<input
                  class="taskband-color-input"
                  type="color"
                  value={taskbandColor(line).value}
                  aria-label={$tStore('customize.lineColor', { label })}
                  onchange={(event) =>
                    updateTaskbandColor(
                      line === 'top' ? 'topColor' : 'bottomColor',
                      event.currentTarget.value,
                    )}
                />{/if}
            </div>
          </div>
          <div class="taskband-row">
            <span class="taskband-row-label"><b>{$tStore('customize.lineBold', { label })}</b></span
            >
            <label class="switch"
              ><input
                type="checkbox"
                aria-label={$tStore('customize.lineBold', { label })}
                checked={line === 'top'
                  ? (taskband?.topBold ?? false)
                  : (taskband?.bottomBold ?? false)}
                onchange={(event) =>
                  updateTaskband({
                    [line === 'top' ? 'topBold' : 'bottomBold']: event.currentTarget.checked,
                  })}
              /><span></span></label
            >
          </div>
          <div class="taskband-row">
            <span class="taskband-row-label"><b>{$tStore('customize.lineSize', { label })}</b></span
            >
            <input
              class="number-field"
              type="number"
              min="7"
              max="16"
              step="0.5"
              value={line === 'top' ? (taskband?.topSize ?? 9) : (taskband?.bottomSize ?? 9)}
              aria-label={$tStore('customize.lineSize', { label })}
              onchange={(event) =>
                updateTaskband({
                  [line === 'top' ? 'topSize' : 'bottomSize']: Number(event.currentTarget.value),
                })}
            />
          </div>
          <div class="taskband-row">
            <span class="taskband-row-label"
              ><b>{$tStore('customize.lineAlignment', { label })}</b></span
            ><SelectMenu
              label={$tStore('customize.lineAlignment', { label })}
              value={String(
                line === 'top' ? (taskband?.topAlign ?? 0) : (taskband?.bottomAlign ?? 0),
              )}
              options={[
                { value: '0', label: $tStore('customize.left') },
                { value: '1', label: $tStore('customize.center') },
                { value: '2', label: $tStore('customize.right') },
              ]}
              onChange={(value) =>
                updateTaskband({
                  [line === 'top' ? 'topAlign' : 'bottomAlign']: Number(value),
                })}
            />
          </div>
        {/snippet}
        {@render taskbandLineStyle($tStore('customize.firstLine'), 'top')}
        {@render taskbandLineStyle($tStore('customize.secondLine'), 'bottom')}
        <div class="taskband-row">
          <span class="taskband-row-label"
            ><b>{$tStore('customize.padding')}</b><small>{$tStore('customize.paddingDesc')}</small
            ></span
          >
          <div class="taskband-pad-controls">
            <input
              class="number-field"
              type="number"
              min="0"
              max="40"
              value={taskband?.paddingLeft ?? 4}
              aria-label={$tStore('customize.paddingLeft')}
              onchange={(event) =>
                updateTaskband({ paddingLeft: clampInt(event.currentTarget.value, 0, 40) })}
            />
            <input
              class="number-field"
              type="number"
              min="0"
              max="40"
              value={taskband?.paddingRight ?? 4}
              aria-label={$tStore('customize.paddingRight')}
              onchange={(event) =>
                updateTaskband({ paddingRight: clampInt(event.currentTarget.value, 0, 40) })}
            />
          </div>
        </div>
        <div class="taskband-row taskband-row--button">
          <button class="taskband-restore" type="button" onclick={restoreTaskbandDefaults}
            >{$tStore('customize.restoreDefaults')}</button
          >
        </div>
      </div>
    {/if}
    {#if catalog.supportsWebviewAuth(provider.id)}
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
  }
</style>
