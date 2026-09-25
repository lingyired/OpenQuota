import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { locale } from 'svelte-i18n';
import ProviderDeviceCodeLogin from './ProviderDeviceCodeLogin.svelte';
import { tBackend } from './i18n';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

type InvokeArgs = { providerId?: string; loginId?: string };
type Command =
  | 'get_provider_session_state'
  | 'start_provider_login'
  | 'poll_provider_login'
  | 'cancel_provider_login'
  | 'delete_provider_session';
type CommandOverrides = Partial<Record<Command, (args?: InvokeArgs) => unknown>>;

const challenge = {
  loginId: 'login-1',
  verificationUri: 'https://example.test/device',
  expiresIn: 600,
};

function mockCommands(overrides: CommandOverrides = {}) {
  mocks.invoke.mockImplementation((command: string, args?: InvokeArgs) => {
    const override = overrides[command as Command];
    if (override) return override(args);
    if (command === 'get_provider_session_state') {
      return Promise.resolve({ providerId: args?.providerId, status: 'notSet' });
    }
    if (command === 'start_provider_login') {
      return Promise.resolve(challenge);
    }
    if (command === 'poll_provider_login') {
      return Promise.resolve({ done: false });
    }
    if (command === 'cancel_provider_login') {
      return Promise.resolve(true);
    }
    if (command === 'delete_provider_session') {
      return Promise.resolve({ providerId: args?.providerId, status: 'notSet' });
    }
    return Promise.reject(new Error(`unexpected command ${command}`));
  });
}

// The polling loop runs on timers, so every step is driven explicitly.
const flush = () => vi.advanceTimersByTimeAsync(0);

function renderPanel(props: { compact?: boolean } = {}) {
  return render(ProviderDeviceCodeLogin, {
    providerId: 'workbuddy-cn',
    providerName: 'Workbuddy CN',
    ...props,
  });
}

async function startSignIn() {
  await fireEvent.click(screen.getByRole('button', { name: 'Start Sign-In' }));
  await flush();
}

describe('ProviderDeviceCodeLogin', () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    mocks.invoke.mockReset();
    mockCommands();
    // Every test starts from a known locale; one of them switches to zh-CN.
    await locale.set('en');
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  // The state a user with only the legacy local WorkBuddy/CodeBuddy login sees:
  // `session_status()` reports Quota01's own vault session as `notSet`, so the
  // panel says "Not connected" even though the app still reads their data. The
  // panel has to explain that, or the copy lives only in the active-attempt
  // state where the confusion cannot be seen.
  const idleFallbackHint =
    'This status shows the sign-in Quota01 owns. An existing local WorkBuddy or CodeBuddy login keeps working as a fallback.';

  it('explains the Quota01-owned status and the local fallback while not connected', async () => {
    renderPanel();
    await flush();

    expect(screen.getByText('Not connected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    // Removing this line from the idle branch of the template fails here.
    expect(screen.getByText(idleFallbackHint)).toBeInTheDocument();

    await locale.set('zh-CN');
    await flush();
    expect(
      screen.getByText(
        '此处显示的是 Quota01 自己的登录；本机已有的 WorkBuddy 或 CodeBuddy 登录会继续作为后备可用。',
      ),
    ).toBeInTheDocument();
  });

  it('swaps the idle clarification for the attempt hint while authorizing', async () => {
    renderPanel();
    await flush();
    expect(screen.getByText(idleFallbackHint)).toBeInTheDocument();

    await startSignIn();

    // The authorizing state keeps its own hint and does not stack the idle copy.
    expect(screen.queryByText(idleFallbackHint)).not.toBeInTheDocument();
    expect(
      screen.getByText(/Open the authorization link and confirm the sign-in there\./),
    ).toBeInTheDocument();
  });

  it('hides the fallback clarification once a session is connected', async () => {
    mockCommands({
      get_provider_session_state: () =>
        Promise.resolve({ providerId: 'workbuddy-cn', status: 'saved' }),
    });
    renderPanel();
    await flush();

    expect(screen.getByText('Connected')).toBeInTheDocument();
    expect(screen.queryByText(idleFallbackHint)).not.toBeInTheDocument();
  });

  it('shows the authorization link after starting a sign-in without opening it', async () => {
    renderPanel();
    await flush();
    expect(screen.getByText('Not connected')).toBeInTheDocument();

    await startSignIn();

    expect(mocks.invoke).toHaveBeenCalledWith('start_provider_login', {
      providerId: 'workbuddy-cn',
    });
    expect(screen.getByText('https://example.test/device')).toBeInTheDocument();
    expect(screen.getByText('Waiting for confirmation…')).toBeInTheDocument();
    // The Rust side owns the browser; the panel only displays the link.
    expect(screen.queryByRole('button', { name: /open/i })).not.toBeInTheDocument();
  });

  it('reports the connected state once a poll completes', async () => {
    let polls = 0;
    let connected = false;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        connected = polls >= 2;
        return Promise.resolve({ done: connected });
      },
      get_provider_session_state: () =>
        Promise.resolve({ providerId: 'workbuddy-cn', status: connected ? 'saved' : 'notSet' }),
    });
    renderPanel();
    await flush();

    await startSignIn();
    expect(polls).toBe(0);

    await vi.advanceTimersByTimeAsync(2000);
    expect(polls).toBe(1);
    expect(screen.getByText('https://example.test/device')).toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(2000);
    expect(polls).toBe(2);
    expect(screen.getByRole('status')).toHaveTextContent('Connected');

    // A poll that follows the completed attempt would report a failure from
    // the provider, so no further poll may be scheduled.
    await vi.advanceTimersByTimeAsync(10000);
    expect(polls).toBe(2);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('never overlaps two polls', async () => {
    let polls = 0;
    let resolvePoll: ((poll: { done: boolean }) => void) | undefined;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return new Promise((resolve) => {
          resolvePoll = resolve;
        });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();

    await vi.advanceTimersByTimeAsync(2000);
    expect(polls).toBe(1);

    // The first poll is still in flight, so no second one may start.
    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(1);

    resolvePoll?.({ done: false });
    await flush();
    await vi.advanceTimersByTimeAsync(2000);
    expect(polls).toBe(2);
  });

  it('returns to idle without an error when the attempt is cancelled', async () => {
    let polls = 0;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return Promise.resolve({ done: false });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();
    expect(screen.getByText('https://example.test/device')).toBeInTheDocument();

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await flush();

    expect(mocks.invoke).toHaveBeenCalledWith('cancel_provider_login', {
      providerId: 'workbuddy-cn',
      loginId: 'login-1',
    });
    expect(screen.queryByText('https://example.test/device')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    // Cancelling is not an expiry: the user gets no failure message at all.
    expect(
      screen.queryByText('This sign-in attempt expired. Start again.'),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(0);
  });

  it('ignores a poll that settles after the attempt was cancelled', async () => {
    let polls = 0;
    let resolvePoll: ((poll: { done: boolean }) => void) | undefined;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return new Promise((resolve) => {
          resolvePoll = resolve;
        });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();
    await vi.advanceTimersByTimeAsync(2000);
    expect(polls).toBe(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await flush();
    resolvePoll?.({ done: true });
    await flush();

    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getByText('Not connected')).toBeInTheDocument();
    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(1);
  });

  it('shows the expired message when the provider reports the attempt expired', async () => {
    let polls = 0;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        // The real wire shape: expiry is a terminal *failure* carrying the
        // provider's own wording — `done: true` with an error
        // (`workbuddy/login.rs` `active_state`).
        return Promise.resolve({
          done: true,
          error: 'This WorkBuddy sign-in attempt expired. Start again.',
        });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();

    await vi.advanceTimersByTimeAsync(2000);

    expect(screen.getByRole('status')).toHaveTextContent('expired');
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByText('Connected')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    expect(screen.queryByText('https://example.test/device')).not.toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(1);
  });

  it('shows the provider message instead of a connection when a poll fails for another reason', async () => {
    let polls = 0;
    // A vault/session-save failure and an untrusted-domain rejection reach the
    // frontend the same way as expiry: `done: true` with an error. Neither may
    // ever be rendered as a completed sign-in.
    const failure =
      'WorkBuddy returned credentials for an untrusted domain (evil.example). Sign in again.';
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return Promise.resolve({ done: true, error: failure });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();

    await vi.advanceTimersByTimeAsync(2000);

    expect(screen.getByRole('alert')).toHaveTextContent(failure);
    expect(screen.queryByText('Connected')).not.toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    expect(screen.queryByText('https://example.test/device')).not.toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(1);
  });

  it('translates a provider failure through the backend glossary', async () => {
    let polls = 0;
    const failure =
      'WorkBuddy returned credentials for an untrusted domain (evil.example). Sign in again.';
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return Promise.resolve({ done: true, error: failure });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();

    await vi.advanceTimersByTimeAsync(2000);
    // English-to-English: the glossary maps this string to the same wording, so
    // raw and translated renders are indistinguishable while the locale is en.
    expect(screen.getByRole('alert')).toHaveTextContent(failure);

    // Switching the locale *after* the message was stored only shows localized
    // copy if the panel translates at render time. Rendering the raw state, or
    // capturing `t(...)`/`tBackend(...)` when the poll settles, both stay
    // English here and fail the assertions below.
    await locale.set('zh-CN');
    await flush();

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent(
      'WorkBuddy 返回了不受信任域名（evil.example）的凭据。请重新登录。',
    );
    expect(alert).toHaveTextContent(tBackend(failure));
    expect(alert).not.toHaveTextContent('untrusted domain');

    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(1);
  });

  it('shows the expired message when the challenge lifetime elapses', async () => {
    let polls = 0;
    mockCommands({
      start_provider_login: () => Promise.resolve({ ...challenge, expiresIn: 1 }),
      poll_provider_login: () => {
        polls += 1;
        return Promise.resolve({ done: false });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();
    expect(screen.getByText('https://example.test/device')).toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(1000);

    expect(screen.getByRole('status')).toHaveTextContent('expired');
    await vi.advanceTimersByTimeAsync(20000);
    expect(polls).toBe(0);
  });

  it('keeps a failed start actionable instead of showing a link', async () => {
    mockCommands({
      start_provider_login: () => Promise.reject(new Error('The sign-in service is unreachable.')),
    });
    renderPanel();
    await flush();

    await startSignIn();

    expect(screen.getByRole('alert')).toHaveTextContent('The sign-in service is unreachable.');
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    expect(screen.queryByText('https://example.test/device')).not.toBeInTheDocument();
  });

  it('stops polling when the panel unmounts', async () => {
    let polls = 0;
    mockCommands({
      poll_provider_login: () => {
        polls += 1;
        return Promise.resolve({ done: false });
      },
    });
    const view = renderPanel();
    await flush();
    await startSignIn();

    view.unmount();
    await vi.advanceTimersByTimeAsync(20000);

    expect(polls).toBe(0);
  });

  it('removes a connected session through the existing disconnect action', async () => {
    let status = 'saved';
    mockCommands({
      get_provider_session_state: () => Promise.resolve({ providerId: 'workbuddy-cn', status }),
      delete_provider_session: () => {
        status = 'notSet';
        return Promise.resolve({ providerId: 'workbuddy-cn', status });
      },
    });
    renderPanel();
    await flush();
    expect(screen.getByText('Connected')).toBeInTheDocument();

    await fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));
    await flush();

    expect(mocks.invoke).toHaveBeenCalledWith('delete_provider_session', {
      providerId: 'workbuddy-cn',
    });
    expect(screen.getByText('Not connected')).toBeInTheDocument();
  });

  it('does not offer disconnect for credentials owned by the environment', async () => {
    mockCommands({
      get_provider_session_state: () =>
        Promise.resolve({ providerId: 'workbuddy-cn', status: 'fromEnvironment' }),
    });
    renderPanel();
    await flush();

    expect(screen.getByText('Connected')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Disconnect' })).not.toBeInTheDocument();
  });

  it('renders the compact variant without the provider summary', async () => {
    renderPanel({ compact: true });
    await flush();

    expect(screen.queryByText('Not connected')).not.toBeInTheDocument();
    expect(screen.queryByText('Workbuddy CN')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
  });
});
