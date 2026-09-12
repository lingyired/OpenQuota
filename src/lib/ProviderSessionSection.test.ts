import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ProviderSessionSection from './ProviderSessionSection.svelte';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

describe('ProviderSessionSection', () => {
  beforeEach(() => {
    mocks.invoke.mockReset().mockImplementation((command: string) => {
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      if (command === 'open_provider_webview_login') {
        return Promise.resolve();
      }
      if (command === 'capture_provider_session') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'saved' });
      }
      if (command === 'delete_provider_session') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
  });

  afterEach(cleanup);

  it('opens the provider-owned sign-in window without accepting a URL from the frontend', async () => {
    render(ProviderSessionSection, {
      providerId: 'trae-cn',
      providerName: 'Trae CN',
    });

    expect(await screen.findByRole('region', { name: 'Trae CN Connection' })).toBeInTheDocument();
    expect(await screen.findByText('Not connected')).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: 'Open Sign-In' }));

    expect(mocks.invoke).toHaveBeenCalledWith('open_provider_webview_login', {
      providerId: 'trae-cn',
    });
  });

  it('captures the signed-in WebView session and reports connected state', async () => {
    render(ProviderSessionSection, {
      providerId: 'trae-cn',
      providerName: 'Trae CN',
    });

    await screen.findByText('Not connected');
    await fireEvent.click(screen.getByRole('button', { name: 'I Have Signed In' }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('capture_provider_session', {
        providerId: 'trae-cn',
      }),
    );
    expect(await screen.findByRole('status')).toHaveTextContent('Connected');
  });

  it('removes a saved session through the provider capability', async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'saved' });
      }
      if (command === 'delete_provider_session') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
    render(ProviderSessionSection, {
      providerId: 'trae-cn',
      providerName: 'Trae CN',
    });

    await screen.findByText('Connected');
    await fireEvent.click(screen.getByRole('button', { name: 'Disconnect' }));

    await waitFor(() => expect(screen.getByText('Not connected')).toBeInTheDocument());
    expect(mocks.invoke).toHaveBeenCalledWith('delete_provider_session', {
      providerId: 'trae-cn',
    });
  });

  it('does not offer disconnect for credentials owned by the environment', async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'fromEnvironment' });
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
    render(ProviderSessionSection, {
      providerId: 'trae-cn',
      providerName: 'Trae CN',
    });

    expect(await screen.findByText('Connected')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Disconnect' })).not.toBeInTheDocument();
  });

  it('keeps a capture failure actionable instead of showing connected', async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'get_provider_session_state') {
        return Promise.resolve({ providerId: 'trae-cn', status: 'notSet' });
      }
      if (command === 'capture_provider_session') {
        return Promise.reject(new Error('Sign in to Trae CN first.'));
      }
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
    render(ProviderSessionSection, {
      providerId: 'trae-cn',
      providerName: 'Trae CN',
    });

    await screen.findByText('Not connected');
    await fireEvent.click(screen.getByRole('button', { name: 'I Have Signed In' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Sign in to Trae CN first.');
    expect(screen.getByText('Not connected')).toBeInTheDocument();
  });
});
