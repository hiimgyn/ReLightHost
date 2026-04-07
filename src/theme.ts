export type ThemeTokens = Record<string, any>;

export function getThemeTokens(isDark: boolean): ThemeTokens {
  return {
    colorPrimary: '#9b72cf',
    colorPrimarySoft: '#b08ee0',
    colorInfo: '#1890ff',
    colorLink: '#9b72cf',
    borderRadius: 10,
    fontFamily:
      '"Inter", "Space Grotesk", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif',
    colorBgLayout: isDark ? 'transparent' : 'transparent',
    // Make light-mode containers slightly bluish to pop against the page gradient
    colorBgContainer: isDark ? '#16161d' : '#f3f5fb',
    colorBgElevated: isDark ? '#1e1e28' : '#ffffff',
    // Border
    colorBorder: isDark ? '#303030' : '#d8dce8',
    colorBorderSecondary: isDark ? '#1f1f1f' : '#e8ecf4',
    // Text hierarchy
    colorText: isDark ? 'rgba(255,255,255,0.88)' : 'rgba(0,0,0,0.88)',
    colorTextSecondary: isDark ? 'rgba(255,255,255,0.65)' : 'rgba(0,0,0,0.65)',
    colorTextTertiary: isDark ? 'rgba(255,255,255,0.45)' : 'rgba(0,0,0,0.48)',
    colorTextQuaternary: isDark ? 'rgba(255,255,255,0.28)' : 'rgba(0,0,0,0.35)',
    // Additional accents
    colorSuccess: '#52c41a',
    colorError: '#ff4d4f',
    // Additional tokens used in CSS mapping
    rhythm: 8,
  };
}

export function applyThemeCssVars(tokens: ThemeTokens) {
  try {
    const root = document.documentElement;
    if (!root) return;
    root.style.setProperty('--rh-primary', tokens.colorPrimary ?? '#9b72cf');
    root.style.setProperty('--rh-info', tokens.colorInfo ?? '#1890ff');
    root.style.setProperty('--rh-bg-elevated', tokens.colorBgElevated ?? '#ffffff');
    root.style.setProperty('--rh-bg-container', tokens.colorBgContainer ?? '#fcfcff');
    root.style.setProperty('--rh-text', tokens.colorText ?? 'rgba(0,0,0,0.88)');
    root.style.setProperty('--rh-text-secondary', tokens.colorTextSecondary ?? 'rgba(0,0,0,0.6)');
    root.style.setProperty('--rh-border', tokens.colorBorder ?? '#d9d9d9');
    root.style.setProperty('--rh-border-secondary', tokens.colorBorderSecondary ?? '#f0f0f0');
    root.style.setProperty('--rh-border-radius', String(tokens.borderRadius ?? 10) + 'px');
    // Pastel accents and utility vars
    root.style.setProperty('--rh-accent', tokens.colorPrimary ?? '#9b72cf');
    root.style.setProperty('--rh-primary-soft', tokens.colorPrimarySoft ?? '#b08ee0');
    root.style.setProperty('--rh-accent-glow', 'rgba(155,114,207,0.12)');
    root.style.setProperty('--rh-active', '#7ee6b7');
    root.style.setProperty('--rh-active-glow', 'rgba(126,230,183,0.18)');
    root.style.setProperty('--rh-danger-pastel', '#ff9ab3');
    root.style.setProperty('--rh-muted-bg', 'rgba(255,255,255,0.04)');
    root.style.setProperty('--rh-muted-bg-light', 'rgba(0,0,0,0.04)');
    root.style.setProperty('--rh-success', tokens.colorSuccess ?? '#52c41a');
    root.style.setProperty('--rh-error', tokens.colorError ?? '#ff4d4f');
  } catch (e) {
    // ignore in environments without DOM
  }
}
