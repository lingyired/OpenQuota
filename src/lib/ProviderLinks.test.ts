import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { locale } from 'svelte-i18n';
import ProviderLinks from './ProviderLinks.svelte';

afterEach(() => {
  cleanup();
  return locale.set('en');
});

describe('ProviderLinks', () => {
  it('opens the selected catalog link and caps the grid at three columns', async () => {
    const onOpen = vi.fn();
    const { container } = render(ProviderLinks, {
      links: [
        { label: 'Status', url: 'https://status.example.com/' },
        { label: 'Dashboard', url: 'https://example.com/dashboard' },
        { label: 'Docs', url: 'https://example.com/docs' },
        { label: 'Support', url: 'https://example.com/support' },
      ],
      onOpen,
    });

    expect(container.querySelector('.provider-links')).toHaveStyle('--provider-link-columns: 3');
    await fireEvent.click(screen.getByRole('button', { name: 'Docs, opens in browser' }));
    expect(onOpen).toHaveBeenCalledWith(2);
  });

  it('localizes known provider link labels', async () => {
    await locale.set('zh-CN');
    render(ProviderLinks, {
      links: [{ label: 'Dashboard', url: 'https://example.com/dashboard' }],
      onOpen: vi.fn(),
    });

    expect(screen.getByText('控制台')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '控制台，在浏览器中打开' })).toBeInTheDocument();
  });
});
