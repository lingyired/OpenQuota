// i18n infrastructure: message registration, locale resolution, and
// translation helpers. Initialized at module load so both tests and the app
// see the same dictionaries (en + zh-CN, fallback en).
//
// Usage:
//   - Templates: `{$_('key', params)}` for static copy (reactive), and
//     `{$tBackendStore(raw)}` for backend-produced strings (reactive).
//   - Script / callbacks: `t('key', params)` and `tBackend(raw)` return the
//     current-locale text (evaluated once; fine for one-shot handlers).
import { get } from 'svelte/store';
import { derived } from 'svelte/store';
import { addMessages, init, locale, _ } from 'svelte-i18n';
import type { LanguagePreference } from '../types';
import { backendGlossary, backendPatterns } from './backendGlossary';
import { en, type Messages } from './messages/en';
import { zhCn } from './messages/zh-CN';

addMessages('en', en);
addMessages('zh-CN', zhCn);

init({ initialLocale: 'en', fallbackLocale: 'en' });

type DotKeys<T, Prefix extends string = ''> = {
  [K in keyof T & string]: T[K] extends string
    ? `${Prefix}${K}`
    : T[K] extends Record<string, unknown>
      ? DotKeys<T[K], `${Prefix}${K}.`>
      : never;
}[keyof T & string];

export type MessageKey = DotKeys<Messages>;

/** The message key type used by the backend glossary entries. */
export type { Messages };

/** Resolve a language preference to an actual locale. */
export function resolveLocale(pref: LanguagePreference): 'en' | 'zh-CN' {
  if (pref === 'en') return 'en';
  if (pref === 'zh-CN') return 'zh-CN';
  const lang = typeof navigator !== 'undefined' ? navigator.language : 'en';
  return lang.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en';
}

/** Apply a language preference; all active components update reactively. */
export function setLanguage(pref: LanguagePreference) {
  void locale.set(resolveLocale(pref));
}

/** Typed translation helper; evaluated once for the current locale. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  return get(_)(key as string, params ? { values: params } : undefined);
}

/**
 * Reactive translation helper for templates: `{$tStore('key', { name })}`
 * re-evaluates whenever the locale changes. Params are passed directly (no
 * `values` wrapper needed).
 */
export const tStore = derived([_], ([formatMessage]) => {
  return (key: string, params?: Record<string, string | number>) =>
    formatMessage(key, params ? { values: params } : undefined);
});

function translateRaw(formatMessage: (key: string, options?: object) => string, raw: string) {
  const exact = backendGlossary[raw];
  if (exact !== undefined) {
    if (Array.isArray(exact)) {
      const [key, params] = exact;
      return formatMessage(key as string, { values: params });
    }
    return formatMessage(exact as string);
  }
  for (const { pattern, key, params } of backendPatterns) {
    const match = raw.match(pattern);
    if (match) {
      const values = Object.fromEntries(params.map((name, index) => [name, match[index + 1]]));
      return formatMessage(key as string, { values });
    }
  }
  return raw;
}

/**
 * Translate a backend-produced English string (evaluated once for the current
 * locale). Exact matches go through the glossary; parameterized strings match
 * patterns; anything unknown falls back to the raw string.
 */
export function tBackend(raw: string): string {
  return translateRaw(get(_), raw);
}

/**
 * Reactive variant of `tBackend` for templates: `{$tBackendStore(raw)}`
 * updates automatically when the locale changes.
 */
export const tBackendStore = derived([_], ([formatMessage]) => {
  return (raw: string) =>
    translateRaw(formatMessage as (key: string, params?: object) => string, raw);
});

export { _ };
