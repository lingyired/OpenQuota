import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ProviderDeviceCodeLogin from './ProviderDeviceCodeLogin.svelte';

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
  beforeEach(() => {
    vi.useFakeTimers();
    mockCommands();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
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
        return Promise.resolve({ done: false, error: 'expired_token' });
      },
    });
    renderPanel();
    await flush();
    await startSignIn();

    await vi.advanceTimersByTimeAsync(2000);

    expect(screen.getByRole('status')).toHaveTextContent('expired');
    expect(screen.getByRole('button', { name: 'Start Sign-In' })).toBeInTheDocument();
    expect(screen.queryByText('https://example.test/device')).not.toBeInTheDocument();

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
