import antigravityColor from '@lobehub/icons-static-svg/icons/antigravity-color.svg';
import claudeColor from '@lobehub/icons-static-svg/icons/claude-color.svg';
import codebuddyColor from '@lobehub/icons-static-svg/icons/codebuddy-color.svg';
import codexColor from '@lobehub/icons-static-svg/icons/codex-color.svg';
import copilotColor from '@lobehub/icons-static-svg/icons/copilot-color.svg';
import devinColor from '@lobehub/icons-static-svg/icons/devin-color.svg';
import kimiColor from '@lobehub/icons-static-svg/icons/kimi-color.svg';
import minimaxColor from '@lobehub/icons-static-svg/icons/minimax-color.svg';
import openrouterColor from '@lobehub/icons-static-svg/icons/openrouter-color.svg';
import antigravity from '../assets/provider-icons/antigravity.svg?raw';
import claude from '../assets/provider-icons/claude.svg?raw';
import codex from '../assets/provider-icons/codex.svg?raw';
import copilot from '../assets/provider-icons/copilot.svg?raw';
import cursor from '../assets/provider-icons/cursor.svg?raw';
import deepseek from '../assets/provider-icons/deepseek.svg?raw';
import devin from '../assets/provider-icons/devin.svg?raw';
import grok from '../assets/provider-icons/grok.svg?raw';
import kimi from '../assets/provider-icons/kimi.svg?raw';
import minimax from '../assets/provider-icons/minimax.svg?raw';
import opencode from '../assets/provider-icons/opencode.svg?raw';
import openrouter from '../assets/provider-icons/openrouter.svg?raw';
import zai from '../assets/provider-icons/zai.svg?raw';
import trae from '../assets/provider-icons/trae.svg?raw';
import workbuddy from '../assets/provider-icons/workbuddy.svg?raw';

const visuals: Record<string, { source: string; color: string | null }> = {
  antigravity: { source: antigravity, color: '#4285F4' },
  claude: { source: claude, color: '#DE7356' },
  codex: { source: codex, color: null },
  copilot: { source: copilot, color: null },
  cursor: { source: cursor, color: null },
  deepseek: { source: deepseek, color: null },
  devin: { source: devin, color: null },
  grok: { source: grok, color: null },
  kimi: { source: kimi, color: '#1783FF' },
  minimax: { source: minimax, color: '#E2167E' },
  opencode: { source: opencode, color: null },
  openrouter: { source: openrouter, color: '#C8FF00' },
  zai: { source: zai, color: null },
  trae: { source: trae, color: null },
  workbuddy: { source: workbuddy, color: '#6C4DFF' },
};

// Full-color brand marks from Lobe Icons. WorkBuddy intentionally uses CodeBuddy's mark.
const colorAssetSlugs: Record<string, string> = {
  antigravity: 'antigravity',
  claude: 'claude',
  codex: 'codex',
  copilot: 'copilot',
  devin: 'devin',
  kimi: 'kimi',
  minimax: 'minimax',
  openrouter: 'openrouter',
  workbuddy: 'codebuddy',
};

const colorAssets: Record<string, string> = {
  antigravity: antigravityColor,
  claude: claudeColor,
  codex: codexColor,
  codebuddy: codebuddyColor,
  copilot: copilotColor,
  devin: devinColor,
  kimi: kimiColor,
  minimax: minimaxColor,
  openrouter: openrouterColor,
};

const fallbackColors: Record<string, string> = {
  cursor: 'var(--provider-cursor)',
  deepseek: '#5786FE',
  grok: 'var(--provider-grok)',
  opencode: 'var(--provider-opencode)',
  trae: '#32F08C',
  zai: 'var(--provider-zai)',
};

export function providerFamily(providerId: string) {
  const family = providerId.split('@', 1)[0];
  return family === 'trae-cn' ? 'trae' : family;
}

export function providerIconPath(providerId: string) {
  const source = visuals[providerFamily(providerId)]?.source;
  if (!source) return '';
  return [...source.matchAll(/<path\b[^>]*\bd="([^"]+)"/g)].map((match) => match[1]).join(' ');
}

export function providerIconUrl(providerId: string) {
  const slug = providerIconSlug(providerId);
  return slug ? (colorAssets[slug] ?? null) : null;
}

export function providerIconSlug(providerId: string) {
  return colorAssetSlugs[providerFamily(providerId)] ?? null;
}

export function providerIconColor(providerId: string) {
  return visuals[providerFamily(providerId)]?.color ?? null;
}

export function providerIconFallbackColor(providerId: string) {
  return fallbackColors[providerFamily(providerId)] ?? 'currentColor';
}

export function providerIconViewBox(providerId: string) {
  return (
    visuals[providerFamily(providerId)]?.source.match(/viewBox="([^"]+)"/)?.[1] ?? '0 0 100 100'
  );
}
