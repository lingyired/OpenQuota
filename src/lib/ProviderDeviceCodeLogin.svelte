<script lang="ts">
  import { onMount } from 'svelte';
  import {
    cancelProviderLogin,
    deleteProviderSession,
    getProviderSessionState,
    pollProviderLogin,
    startProviderLogin,
  } from './backend';
  import Icon from './Icon.svelte';
  import { t, tStore } from './i18n';
  import ProviderIcon from './ProviderIcon.svelte';
  import type {
    ApiKeyStatus,
    DeviceCodeChallenge,
    DeviceCodePoll,
    ProviderApiKeyState,
  } from './types';

  interface Props {
    providerId: string;
    providerName: string;
    compact?: boolean;
  }

  let { providerId, providerName, compact = false }: Props = $props();

  // Each poll waits for the previous one to settle, and a poll that follows a
  // successful sign-in would be answered with a failure by the provider, so a
  // completed attempt never schedules another poll.
  const pollIntervalMs = 2000;

  let credentialState = $state<ProviderApiKeyState | null>(null);
  let challenge = $state<DeviceCodeChallenge | null>(null);
  let loading = $state(true);
  let busy = $state<'start' | 'disconnect' | null>(null);
  let expired = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let disposed = false;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let expiryTimer: ReturnType<typeof setTimeout> | undefined;

  const status = $derived<ApiKeyStatus>(credentialState?.status ?? 'notSet');
  const connected = $derived(status !== 'notSet');
  const canDisconnect = $derived(status === 'saved' || status === 'overrideActive');
  const authorizing = $derived(challenge !== null);

  function errorMessage(cause: unknown, fallback: string) {
    if (typeof cause === 'string') return cause;
    if (cause instanceof Error && cause.message) return cause.message;
    return fallback;
  }

  function clearPollTimer() {
    if (pollTimer !== undefined) {
      clearTimeout(pollTimer);
      pollTimer = undefined;
    }
  }

  function clearExpiryTimer() {
    if (expiryTimer !== undefined) {
      clearTimeout(expiryTimer);
      expiryTimer = undefined;
    }
  }

  function clearTimers() {
    clearPollTimer();
    clearExpiryTimer();
  }

  function endAttempt() {
    clearTimers();
    challenge = null;
    busy = null;
    expired = false;
  }

  function expire() {
    endAttempt();
    error = null;
    notice = null;
    expired = true;
  }

  function schedulePoll(loginId: string) {
    clearPollTimer();
    pollTimer = setTimeout(() => void pollOnce(loginId), pollIntervalMs);
  }

  async function pollOnce(loginId: string) {
    pollTimer = undefined;
    let outcome: DeviceCodePoll | null = null;
    let failure: string | null = null;
    try {
      outcome = await pollProviderLogin(providerId, loginId);
    } catch (cause) {
      failure = errorMessage(cause, t('providerSession.signInError'));
    }
    // A cancel or unmount while this poll was in flight owns the outcome, so
    // a late answer must not revive the attempt.
    if (disposed || challenge?.loginId !== loginId) return;
    if (failure !== null) {
      endAttempt();
      error = failure;
      return;
    }
    // A provider error is terminal whether or not `done` is set: the Rust side
    // reports an expired/cancelled attempt and a failed session save alike as
    // `{ done: true, error: Some(message) }`, so `done` must never be read as a
    // successful sign-in while an error is present.
    if (outcome?.error) {
      const message = outcome.error;
      if (isExpiry(message)) {
        expire();
      } else {
        // The provider's own wording is more useful than a generic line: it
        // names an untrusted domain, a vault write failure, etc.
        endAttempt();
        notice = null;
        error = message;
      }
      return;
    }
    if (outcome?.done) {
      await complete();
      return;
    }
    schedulePoll(loginId);
  }

  // Only the provider's expiry wording gets the localized "expired" copy; every
  // other failure keeps the provider's message. The backend's failure messages
  // are English (workbuddy/login.rs), and only the expiry one contains this.
  function isExpiry(message: string) {
    return /expired/i.test(message);
  }

  async function complete() {
    endAttempt();
    try {
      credentialState = await getProviderSessionState(providerId);
      if (disposed) return;
      error = null;
      notice = t('providerSession.connected');
    } catch (cause) {
      if (disposed) return;
      error = errorMessage(cause, t('providerSession.loadError'));
    }
  }

  async function start() {
    if (busy !== null || authorizing) return;
    busy = 'start';
    error = null;
    notice = null;
    expired = false;
    try {
      const next = await startProviderLogin(providerId);
      if (disposed) return;
      challenge = next;
      expiryTimer = setTimeout(() => {
        if (challenge?.loginId === next.loginId) expire();
      }, next.expiresIn * 1000);
      schedulePoll(next.loginId);
    } catch (cause) {
      if (disposed) return;
      error = errorMessage(cause, t('providerSession.signInError'));
    } finally {
      busy = null;
    }
  }

  async function cancel() {
    const loginId = challenge?.loginId;
    endAttempt();
    error = null;
    notice = null;
    if (loginId === undefined) return;
    try {
      await cancelProviderLogin(providerId, loginId);
    } catch {
      // Cancelling is best effort: the attempt expires on its own.
    }
  }

  async function disconnect() {
    if (busy !== null || !canDisconnect) return;
    busy = 'disconnect';
    error = null;
    notice = null;
    expired = false;
    try {
      credentialState = await deleteProviderSession(providerId);
    } catch (cause) {
      error = errorMessage(cause, t('providerSession.disconnectError'));
    } finally {
      busy = null;
    }
  }

  onMount(() => {
    void getProviderSessionState(providerId)
      .then((next) => {
        if (!disposed) credentialState = next;
      })
      .catch((cause) => {
        if (!disposed) error = errorMessage(cause, t('providerSession.loadError'));
      })
      .finally(() => {
        if (!disposed) loading = false;
      });
    return () => {
      disposed = true;
      clearTimers();
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
          : authorizing
            ? $tStore('providerSession.waitingForSignIn')
            : loading
              ? $tStore('providerSession.capturing')
              : $tStore('providerSession.notConnected')}</span
      >
      <i class:connected aria-hidden="true"></i>
    </div>
  {/if}
  {#if authorizing && challenge}
    <p class="session-hint">{$tStore('providerSession.deviceCodeHint')}</p>
    <p class="session-uri">
      <span class="session-uri-label">{$tStore('providerSession.verificationUri')}</span>
      <code>{challenge.verificationUri}</code>
    </p>
  {/if}
  {#if error}
    <p class="session-message session-error" role="alert">
      <Icon name="about" size={14} strokeWidth={2.2} />{error}
    </p>
  {:else if expired}
    <p class="session-message session-expired" role="status">
      <Icon name="about" size={14} strokeWidth={2.2} />{$tStore('providerSession.expired')}
    </p>
  {:else if notice}
    <p class="session-message session-notice" role="status">
      <Icon name="check" size={14} strokeWidth={2.2} />{notice}
    </p>
  {/if}
  <div class:compact class="session-actions" aria-label={`${providerName} sign-in actions`}>
    {#if authorizing}
      <button type="button" disabled={busy !== null} onclick={cancel}
        >{$tStore('providerSession.cancel')}</button
      >
    {:else if !connected}
      <button class="primary" type="button" disabled={busy !== null} onclick={start}
        >{$tStore('providerSession.startSignIn')}</button
      >
    {:else if canDisconnect}
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

  .session-expired {
    color: var(--warning, #ff9f0a);
  }

  .session-notice {
    color: var(--success, #34c759);
  }

  .session-uri {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin: 0;
    padding: 0 12px 10px;
  }

  .session-uri-label {
    flex: 0 0 auto;
    color: var(--secondary);
    font-size: 11px;
  }

  .session-uri code {
    min-width: 0;
    color: var(--text);
    font-size: 11px;
    overflow-wrap: anywhere;
    user-select: text;
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
