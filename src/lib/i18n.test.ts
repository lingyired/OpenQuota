import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { locale } from 'svelte-i18n';
import { resolveLocale, setLanguage, t, tBackend, tBackendStore, tStore } from './i18n';
import { en } from './i18n/messages/en';
import { zhCn } from './i18n/messages/zh-CN';
import fixture from '../test/i18nFixture.svelte';

// Helpers to make locale switches deterministic in tests.
async function switchLocale(lang: string) {
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
  it('resolves Chinese system languages to the matching script', () => {
    stubNavigatorLanguage('zh-CN');
    expect(resolveLocale('system')).toBe('zh-CN');
    stubNavigatorLanguage('zh-TW');
    expect(resolveLocale('system')).toBe('zh-TW');
    stubNavigatorLanguage('zh-HK');
    expect(resolveLocale('system')).toBe('zh-TW');
    stubNavigatorLanguage('zh');
    expect(resolveLocale('system')).toBe('zh-CN');
  });

  it('resolves supported system languages and falls back to en', () => {
    stubNavigatorLanguage('en-US');
    expect(resolveLocale('system')).toBe('en');
    stubNavigatorLanguage('ja-JP');
    expect(resolveLocale('system')).toBe('ja');
    stubNavigatorLanguage('pt-PT');
    expect(resolveLocale('system')).toBe('pt-BR');
    stubNavigatorLanguage('xx-YY');
    expect(resolveLocale('system')).toBe('en');
  });

  it('honors an explicit language preference over the system language', () => {
    stubNavigatorLanguage('zh-CN');
    expect(resolveLocale('en')).toBe('en');
    stubNavigatorLanguage('en-US');
    expect(resolveLocale('zh-CN')).toBe('zh-CN');
    expect(resolveLocale('ar')).toBe('ar');
  });
});

describe('translation helpers', () => {
  it('translates static keys with parameters', () => {
    expect(t('dashboard.refreshProvider', { provider: 'Claude' })).toBe('Refresh Claude');
  });

  it('tBackend resolves glossary entries', () => {
    expect(tBackend('Usage Today')).toBe('Usage Today');
  });

  it('translates provider catalog labels and plan names in Chinese', async () => {
    await switchLocale('zh-CN');
    expect(tBackend('Spark')).toBe('Spark 配额');
    expect(tBackend('Spark Weekly')).toBe('Spark 周配额');
    expect(tBackend('Extra Usage')).toBe('额外用量');
    expect(tBackend('Auto usage')).toBe('自动用量');
    expect(tBackend('API usage')).toBe('API 用量');
    expect(tBackend('On-demand')).toBe('按需用量');
    expect(tBackend('requests')).toBe('次请求');
    expect(tBackend('Usage Trend')).toBe('用量趋势');
    expect(tBackend('searches')).toBe('次搜索');
    expect(tBackend('Rate Limit Resets')).toBe('速率限制重置');
    expect(tBackend('Today')).toBe('今日');
    expect(tBackend('Free')).toBe('免费');
    expect(tBackend('available')).toBe('可用');
    expect(tBackend('Available')).toBe('可用');
    expect(tBackend('Your TraeWork CN session expired. Sign in again.')).toBe(
      '你的 TraeWork CN 会话已过期。请重新登录。',
    );
    expect(tBackend('Sign in to DeepSeek to view usage.')).toBe('请先登录 DeepSeek，以查看用量。');
    expect(tBackend('Your DeepSeek session expired. Sign in again.')).toBe(
      '你的 DeepSeek 会话已过期。请重新登录。',
    );
    expect(tBackend('The DeepSeek session could not be read or updated.')).toBe(
      'DeepSeek 会话无法读取或更新。',
    );
    expect(tBackend('Could not reach Kimi. Check your internet connection.')).toBe(
      '无法访问 Kimi。请检查你的网络连接。',
    );
    expect(tBackend('Cursor usage request failed (HTTP 429).')).toBe(
      'Cursor 用量请求失败（HTTP 429）。',
    );
    expect(tBackend('Not logged in. Run `codex` to authenticate.')).toBe(
      '尚未登录。请运行 `codex` 进行身份验证。',
    );
    expect(
      tBackend(
        'Add a Z.ai API key in Customize, set ZAI_API_KEY, or configure ~/.config/quota01/zai.json.',
      ),
    ).toBe('在自定义中添加 Z.ai API 密钥、设置 ZAI_API_KEY，或配置 ~/.config/quota01/zai.json。');
    expect(tBackend('MiniMax request failed (HTTP 500).')).toBe('MiniMax 请求失败（HTTP 500）。');
    expect(tBackend('Granted Balance')).toBe('赠送余额');
    expect(tBackend('Recharged Balance')).toBe('充值余额');
    expect(tBackend('Coding Plan')).toBe('编程套餐');
    expect(
      tBackend('Add a SiliconFlow CN API key in Customize or set SILICONFLOW_CN_API_KEY.'),
    ).toBe('在自定义中添加 SiliconFlow CN API 密钥，或设置 SILICONFLOW_CN_API_KEY。');
    expect(tBackend('Add an Infini API key in Customize or set INFINI_API_KEY.')).toBe(
      '在自定义中添加 Infini API 密钥，或设置 INFINI_API_KEY。',
    );
    expect(
      tBackend(
        'The SiliconFlow CN API key is invalid. Check it at cloud.siliconflow.cn/account/ak.',
      ),
    ).toBe('SiliconFlow CN API 密钥无效。请在 cloud.siliconflow.cn/account/ak 查看。');
    expect(tBackend('Infini usage data is temporarily unavailable.')).toBe(
      'Infini 用量数据暂时不可用。',
    );
    expect(tBackend('Could not reach Infini. Check your internet connection.')).toBe(
      '无法访问 Infini。请检查你的网络连接。',
    );
    expect(tBackend('Add an OpenCode Go API key in Customize or set OPENCODE_GO_API_KEY.')).toBe(
      '在自定义中添加 OpenCode Go API 密钥，或设置 OPENCODE_GO_API_KEY。',
    );
    expect(tBackend('Add a CommandCode API key in Customize or set COMMANDCODE_API_KEY.')).toBe(
      '在自定义中添加 CommandCode API 密钥，或设置 COMMANDCODE_API_KEY。',
    );
    expect(tBackend('The CommandCode API key is invalid. Check it at commandcode.ai.')).toBe(
      'CommandCode API 密钥无效。请在 commandcode.ai 查看。',
    );
    expect(tBackend('Could not reach CommandCode. Check your internet connection.')).toBe(
      '无法访问 CommandCode。请检查你的网络连接。',
    );
  });

  it('translates the WorkBuddy sign-in and device-code backend messages', async () => {
    await switchLocale('zh-CN');
    expect(
      tBackend(
        'WorkBuddy returned credentials for an untrusted domain (evil.example). Sign in again.',
      ),
    ).toBe('WorkBuddy 返回了不受信任域名（evil.example）的凭据。请重新登录。');
    expect(tBackend('WorkBuddy returned credentials without a domain. Sign in again.')).toBe(
      'WorkBuddy 返回的凭据缺少域名。请重新登录。',
    );
    expect(tBackend('This WorkBuddy sign-in attempt is unknown. Start again.')).toBe(
      '此 WorkBuddy 登录尝试未知。请重新开始。',
    );
    expect(tBackend('This WorkBuddy sign-in attempt is no longer active. Start again.')).toBe(
      '此 WorkBuddy 登录尝试已不再有效。请重新开始。',
    );
    expect(tBackend('This WorkBuddy sign-in attempt expired. Start again.')).toBe(
      '此 WorkBuddy 登录尝试已过期。请重新开始。',
    );
    expect(tBackend('That provider does not use a device-code sign-in.')).toBe(
      '该提供方不使用设备码登录。',
    );
    expect(tBackend('The sign-in could not be started.')).toBe('无法启动登录。');
    expect(tBackend('The sign-in status could not be read.')).toBe('无法读取登录状态。');
    expect(tBackend('Could not reach WorkBuddy.')).toBe('无法访问 WorkBuddy。请检查你的网络连接。');
    expect(tBackend('That provider does not have a saved connection.')).toBe(
      '该提供方没有已保存的连接。',
    );
    expect(
      tBackend(
        'WorkBuddy 5.6 encrypts the login data it keeps on this computer, so it cannot be read directly. Sign in to WorkBuddy from Quota01 to connect; the legacy plaintext login file is still used when present.',
      ),
    ).toBe(
      'WorkBuddy 会加密保存在本机的登录数据，因此无法直接读取。请在 Quota01 中登录 WorkBuddy 以建立连接；本机若仍有旧版明文登录文件，仍会继续使用。',
    );
  });

  it('tBackend falls back to the raw string when unknown', () => {
    expect(tBackend('Some brand-new backend string')).toBe('Some brand-new backend string');
  });

  it('tBackend resolves parameterized patterns', () => {
    expect(tBackend('Retrying in about 5 minutes')).toBe('Retrying in about 5 minutes');
  });

  it('tBackend translates refresh-state, settings, and update failures', async () => {
    await switchLocale('zh-CN');
    expect(tBackend('Limit reached')).toBe('已达上限');
    expect(tBackend('Reset unavailable')).toBe('重置时间不可用');
    expect(tBackend('Provider refresh timed out.')).toBe('提供方刷新超时。');
    expect(tBackend('The saved global shortcut is currently unavailable.')).toBe(
      '已保存的全局快捷键当前不可用。',
    );
    expect(tBackend('GitHub refused the update download.')).toBe('GitHub 拒绝了更新下载。');
    expect(tBackend('Wait for it to finish, then try again.')).toBe('请等待其完成，然后重试。');
    expect(
      tBackend('Quota01 could not check for updates because the network request failed.'),
    ).toBe('Quota01 无法完成该操作，因为网络请求失败。');

    await switchLocale('en');
    expect(tBackend('Limit reached')).toBe('Limit reached');
    expect(tBackend('Provider refresh timed out.')).toBe('Provider refresh timed out.');
    expect(tBackend('GitHub refused the update download.')).toBe(
      'GitHub refused the update download.',
    );
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
