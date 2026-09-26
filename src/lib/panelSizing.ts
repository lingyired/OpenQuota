export const PANEL_MIN_HEIGHT = 360;
export const PANEL_MAX_HEIGHT = 800;
export const PANEL_SCREEN_FRACTION = 0.85;
export const PANEL_POPUP_WIDTH = 440;
export const PANEL_SETTINGS_WIDTH = 1000;
export const PANEL_COMPACT_BREAKPOINT = 900;

export function panelLayoutForScreen(screen: 'dashboard' | 'settings', workAreaWidth: number) {
  const availableWidth = Number.isFinite(workAreaWidth) && workAreaWidth > 0 ? workAreaWidth : 0;
  const insetWorkAreaWidth =
    availableWidth > PANEL_POPUP_WIDTH ? availableWidth - 32 : availableWidth;
  const width =
    screen === 'dashboard'
      ? PANEL_POPUP_WIDTH
      : Math.min(PANEL_SETTINGS_WIDTH, insetWorkAreaWidth || PANEL_SETTINGS_WIDTH);
  return { width, compact: screen === 'settings' && width < PANEL_COMPACT_BREAKPOINT };
}

export function shouldDeferPanelFit(screen: string, refreshing: boolean) {
  return screen === 'dashboard' && refreshing;
}

export function panelMaximumHeight(workAreaHeight: number) {
  if (!Number.isFinite(workAreaHeight) || workAreaHeight <= 0) return 680;
  return Math.max(
    PANEL_MIN_HEIGHT,
    Math.min(PANEL_MAX_HEIGHT, Math.floor(workAreaHeight * PANEL_SCREEN_FRACTION)),
  );
}

export function panelTargetHeight(idealHeight: number, workAreaHeight: number) {
  const maximum = panelMaximumHeight(workAreaHeight);
  return Math.min(maximum, Math.max(PANEL_MIN_HEIGHT, Math.ceil(idealHeight)));
}

export function screenPanelHeight(
  screen: 'dashboard' | 'customize' | 'settings' | `provider:${string}`,
  contentTarget: number,
  dashboardHeight: number,
) {
  return screen === 'dashboard' ? contentTarget : dashboardHeight;
}
