<script lang="ts">
  import { onMount } from 'svelte';
  import {
    captureProviderSession,
    deleteProviderSession,
    getProviderSessionState,
    onProviderSessionWindowClosed,
    openProviderWebviewLogin,
  } from './backend';
  import Icon from './Icon.svelte';
  import { t, tStore } from './i18n';
  import ProviderIcon from './ProviderIcon.svelte';
  import type { ApiKeyStatus, ProviderApiKeyState } from './types';

  interface Props {
    providerId: string;
    providerName: string;
    compact?: boolean;
  }

  let { providerId, providerName, compact = false }: Props = $props();
  let credentialState = $state<ProviderApiKeyState | null>(null);
  let loading = $state(true);
  let busy = $state<'capture' | 'disconnect' | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  const status = $derived<ApiKeyStatus>(credentialState?.status ?? 'notSet');
  const connected = $derived(status !== 'notSet');
  const canDisconnect = $derived(status === 'saved' || status === 'overrideActive');

  function errorMessage(cause: unknown, fallback: string) {
    if (typeof cause === 'string') return cause;
    if (cause instanceof Error && cause.message) return cause.message;
    return fallback;
  }

  async function openLogin() {
    error = null;
    notice = null;
    try {
      await openProviderWebviewLogin(providerId);
      notice = t('providerSession.hint');
    } catch (cause) {
      error = errorMessage(cause, t('providerSession.captureError'));
    }
  }

  async function capture() {
    if (busy) return;
    busy = 'capture';
    error = null;
    notice = null;
    try {
      credentialState = await captureProviderSession(providerId);
      notice = t('providerSession.connected');
    } catch (cause) {
      error = errorMessage(cause, t('providerSession.captureError'));
    } finally {
      busy = null;
    }
  }

  async function disconnect() {
    if (busy || !canDisconnect) return;
    busy = 'disconnect';
    error = null;
    notice = null;
    try {
      credentialState = await deleteProviderSession(providerId);
    } catch (cause) {
      error = errorMessage(cause, t('providerSession.disconnectError'));
    } finally {
      busy = null;
    }
  }

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void onProviderSessionWindowClosed((event) => {
      if (!disposed && event.providerId === providerId && busy === null) {
        void capture();
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    void getProviderSessionState(providerId)
      .then((next) => {
        credentialState = next;
      })
      .catch((cause) => {
        error = errorMessage(cause, t('providerSession.loadError'));
      })
      .finally(() => {
        loading = false;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<div class:compact class="session-actions-root">
  {#if !compact}
    <div class="session-summary">
      <ProviderIcon {providerId} size={18} />
      <span class="session-provider">{providerName}</span>
      <span class:connected class="session-status"
        >{connected
          ? $tStore('providerSession.connected')
          : loading
            ? $tStore('providerSession.capturing')
            : $tStore('providerSession.notConnected')}</span
      >
      <i class:connected aria-hidden="true"></i>
    </div>
    <p class="session-hint">{$tStore('providerSession.hint')}</p>
  {/if}
  {#if error}
    <p class="session-message session-error" role="alert">
      <Icon name="about" size={14} strokeWidth={2.2} />{error}
    </p>
  {/if}
  {#if notice && !error}
    <p class="session-message session-notice" role="status">
      <Icon name="check" size={14} strokeWidth={2.2} />{notice}
    </p>
  {/if}
  <div class:compact class="session-actions" aria-label={`${providerName} sign-in actions`}>
    <button type="button" disabled={busy !== null} onclick={openLogin}
      >{$tStore('providerSession.openSignIn')}</button
    >
    <button class="primary" type="button" disabled={busy !== null} onclick={capture}
      >{busy === 'capture'
        ? $tStore('providerSession.capturing')
        : $tStore('providerSession.capture')}</button
    >
    {#if !compact && canDisconnect}
      <button class="danger" type="button" disabled={busy !== null} onclick={disconnect}
        >{busy === 'disconnect'
          ? $tStore('providerSession.disconnecting')
          : $tStore('providerSession.disconnect')}</button
      >
    {/if}
  </div>
</div>

<style>
  .session-summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--separator);
  }

  .session-provider {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-status {
    color: var(--secondary);
    font-size: 11px;
  }

  .session-status.connected {
    color: var(--success, #34c759);
  }

  .session-summary i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--tertiary);
  }

  .session-summary i.connected {
    background: var(--success, #34c759);
  }

  .session-hint,
  .session-message {
    margin: 0;
    padding: 10px 12px;
    color: var(--secondary);
    font-size: 11px;
    line-height: 1.45;
  }

  .session-message {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }

  .session-error {
    color: var(--danger);
  }

  .session-notice {
    color: var(--success, #34c759);
  }

  .session-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 0 12px 12px;
  }

  .session-actions.compact {
    padding: 0;
  }

  .session-actions button {
    min-height: 30px;
    padding: 0 10px;
    border: 1px solid var(--separator);
    border-radius: 8px;
    color: var(--text);
    background: var(--surface-2, var(--surface));
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  .session-actions.compact button {
    min-height: 24px;
    padding-inline: 8px;
    border-radius: 6px;
    font-size: 10px;
  }

  .session-actions button.primary {
    border-color: var(--meter-fill);
    color: var(--surface);
    background: var(--meter-fill);
  }

  .session-actions button.danger {
    color: var(--danger);
  }

  .session-actions button:disabled {
    cursor: default;
    opacity: 0.55;
  }
</style>
