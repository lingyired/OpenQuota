import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { locale } from 'svelte-i18n';
import { resolveLocale, setLanguage, t, tBackend, tBackendStore, tStore } from './i18n';
import { en } from './i18n/messages/en';
import { zhCn } from './i18n/messages/zh-CN';
import fixture from '../test/i18nFixture.svelte';

// Helpers to make locale switches deterministic in tests.
async function switchLocale(lang: 'en' | 'zh-CN') {
  await locale.set(lang);
}

function stubNavigatorLanguage(lang: string) {
  Object.defineProperty(navigator, 'language', { configurable: true, value: lang });
}

const originalLanguage = navigator.language;

afterEach(() => {
  cleanup();
  stubNavigatorLanguage(originalLanguage);
  vi.restoreAllMocks();
});

beforeEach(() => {
  // Reset to English so each test starts from a known locale.
  return locale.set('en');
});

describe('resolveLocale', () => {
  it('resolves a Chinese system language to zh-CN', () => {
    stubNavigatorLanguage('zh-CN');
    expect(resolveLocale('system')).toBe('zh-CN');
    stubNavigatorLanguage('zh-TW');
    expect(resolveLocale('system')).toBe('zh-CN');
    stubNavigatorLanguage('zh');
    expect(resolveLocale('system')).toBe('zh-CN');
  });

  it('resolves a non-Chinese system language to en', () => {
    stubNavigatorLanguage('en-US');
    expect(resolveLocale('system')).toBe('en');
    stubNavigatorLanguage('ja-JP');
    expect(resolveLocale('system')).toBe('en');
  });

  it('honors an explicit language preference over the system language', () => {
    stubNavigatorLanguage('zh-CN');
    expect(resolveLocale('en')).toBe('en');
    stubNavigatorLanguage('en-US');
    expect(resolveLocale('zh-CN')).toBe('zh-CN');
  });
});

describe('translation helpers', () => {
  it('translates static keys with parameters', () => {
    expect(t('dashboard.refreshProvider', { provider: 'Claude' })).toBe('Refresh Claude');
  });

  it('tBackend resolves glossary entries', () => {
    expect(tBackend('Usage Today')).toBe('Usage Today');
  });

  it('tBackend falls back to the raw string when unknown', () => {
    expect(tBackend('Some brand-new backend string')).toBe('Some brand-new backend string');
  });

  it('tBackend resolves parameterized patterns', () => {
    expect(tBackend('Retrying in about 5 minutes')).toBe('Retrying in about 5 minutes');
  });
});

describe('zh-CN dictionary', () => {
  it('covers every key of the en dictionary (structure parity)', () => {
    const enKeys = Object.keys(en);
    const zhKeys = Object.keys(zhCn);
    expect(zhKeys.sort()).toEqual(enKeys.sort());
  });
});

describe('reactive language switching', () => {
  it('updates rendered text immediately when the language changes', async () => {
    render(fixture);
    expect(screen.getByTestId('static')).toHaveTextContent('Language');
    expect(screen.getByTestId('backend')).toHaveTextContent('Usage Today');

    await switchLocale('zh-CN');
    expect(screen.getByTestId('static')).toHaveTextContent('语言');
    expect(screen.getByTestId('backend')).toHaveTextContent('今日用量');

    await switchLocale('en');
    expect(screen.getByTestId('static')).toHaveTextContent('Language');
  });

  it('propagates through the tStore and tBackendStore derived stores', async () => {
    let staticValue = '';
    let backendValue = '';
    const unsubStatic = tStore.subscribe((format) => {
      staticValue = format('settings.language');
    });
    const unsubBackend = tBackendStore.subscribe((format) => {
      backendValue = format('Usage Today');
    });

    await switchLocale('zh-CN');
    expect(staticValue).toBe('语言');
    expect(backendValue).toBe('今日用量');

    await switchLocale('en');
    expect(staticValue).toBe('Language');
    expect(backendValue).toBe('Usage Today');

    unsubStatic();
    unsubBackend();
  });

  it('setLanguage applies a resolved preference', async () => {
    stubNavigatorLanguage('zh-CN');
    setLanguage('system');
    expect(get(locale)).toBe('zh-CN');
    setLanguage('en');
    expect(get(locale)).toBe('en');
  });
});
