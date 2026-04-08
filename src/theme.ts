export type ThemeTokens = Record<string, any>;

export function getThemeTokens(isDark: boolean): ThemeTokens {
  return {
    colorPrimary: '#6367FF',
    colorPrimarySoft: '#8494FF',
    colorInfo: '#8494FF',
    colorLink: '#6367FF',
    borderRadius: 10,
    fontFamily:
      '"Inter", "Space Grotesk", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif',
    colorBgLayout: 'transparent',
    // Glassmorphism surfaces: low alpha with blur from CSS layer.
    colorBgContainer: isDark ? 'rgba(45, 50, 78, 0.18)' : 'rgba(255, 255, 255, 0.16)',
    colorBgElevated: isDark ? 'rgba(58, 64, 96, 0.24)' : 'rgba(255, 255, 255, 0.22)',
    // Border — Fluent enhanced opacity
    colorBorder: isDark ? 'rgba(209, 212, 255, 0.22)' : 'rgba(99, 103, 255, 0.2)',
    colorBorderSecondary: isDark ? 'rgba(209, 212, 255, 0.14)' : 'rgba(99, 103, 255, 0.14)',
    // Text hierarchy — Fluent refined for better contrast with higher opacity backgrounds
    colorText: isDark ? 'rgba(255, 255, 255, 0.92)' : 'rgba(0, 0, 0, 0.92)',
    colorTextSecondary: isDark ? 'rgba(255, 255, 255, 0.72)' : 'rgba(0, 0, 0, 0.72)',
    colorTextTertiary: isDark ? 'rgba(255, 255, 255, 0.52)' : 'rgba(0, 0, 0, 0.52)',
    colorTextQuaternary: isDark ? 'rgba(255, 255, 255, 0.35)' : 'rgba(0, 0, 0, 0.35)',
    // Fluent Design accent colors
    colorSuccess: '#6ec8a6',
    colorError: '#ef7f92',
    // Rhythm unit for consistent spacing
    rhythm: 8,
  };
}

export function applyThemeCssVars(tokens: ThemeTokens, isDark: boolean) {
  try {
    const root = document.documentElement;
    if (!root) return;
    
    // Primary and accent colors
    root.style.setProperty('--rh-primary', tokens.colorPrimary ?? '#6367FF');
    root.style.setProperty('--rh-info', tokens.colorInfo ?? '#8494FF');
    root.style.setProperty('--rh-primary-soft', tokens.colorPrimarySoft ?? '#8494FF');
    root.style.setProperty('--rh-wash-lilac', '#C9BEFF');
    root.style.setProperty('--rh-wash-rose', '#FFDBFD');
    root.style.setProperty('--rh-minimal-ink', '#1F2333');
    root.style.setProperty('--rh-minimal-slate', '#8B92A8');
    root.style.setProperty('--rh-minimal-mist', '#EEF1FA');
    
    // Glass and minimal surface palettes
    root.style.setProperty('--rh-glass-bg', isDark ? 'rgba(45, 50, 78, 0.18)' : 'rgba(255, 255, 255, 0.16)');
    root.style.setProperty('--rh-glass-bg-strong', isDark ? 'rgba(58, 64, 96, 0.24)' : 'rgba(255, 255, 255, 0.22)');
    root.style.setProperty('--rh-glass-border', isDark ? 'rgba(210, 216, 255, 0.26)' : 'rgba(255, 255, 255, 0.34)');
    root.style.setProperty('--rh-glass-shadow', isDark ? '0 8px 32px rgba(0, 0, 0, 0.28)' : '0 8px 32px rgba(15, 23, 42, 0.12)');
    root.style.setProperty('--rh-glass-inset', isDark
      ? 'inset 0 1px 0 rgba(255, 255, 255, 0.2), inset 0 -1px 0 rgba(255, 255, 255, 0.08), inset 0 0 12px 6px rgba(167, 180, 255, 0.12)'
      : 'inset 0 1px 0 rgba(255, 255, 255, 0.56), inset 0 -1px 0 rgba(255, 255, 255, 0.14), inset 0 0 12px 6px rgba(255, 255, 255, 0.4)');

    root.style.setProperty('--rh-minimal-bg', isDark ? 'rgba(18, 20, 34, 0.92)' : 'rgba(255, 255, 255, 0.92)');
    root.style.setProperty('--rh-minimal-bg-strong', isDark ? 'rgba(28, 31, 48, 0.96)' : 'rgba(250, 251, 255, 0.98)');
    root.style.setProperty('--rh-minimal-border', isDark ? 'rgba(132, 148, 255, 0.16)' : 'rgba(99, 103, 255, 0.14)');
    root.style.setProperty('--rh-minimal-shadow', isDark ? '0 10px 32px rgba(0, 0, 0, 0.34)' : '0 8px 24px rgba(15, 23, 42, 0.06)');

    root.style.setProperty('--rh-tooltip-bg', isDark ? 'rgba(18, 20, 34, 0.96)' : 'rgba(255, 255, 255, 0.96)');
    root.style.setProperty('--rh-tooltip-border', isDark ? 'rgba(132, 148, 255, 0.22)' : 'rgba(99, 103, 255, 0.18)');
    root.style.setProperty('--rh-tooltip-shadow', isDark ? '0 12px 28px rgba(0, 0, 0, 0.42)' : '0 12px 28px rgba(15, 23, 42, 0.14)');

    // Background colors
    root.style.setProperty('--rh-bg-elevated', tokens.colorBgElevated ?? (isDark ? 'rgba(58, 64, 96, 0.24)' : 'rgba(255, 255, 255, 0.22)'));
    root.style.setProperty('--rh-bg-container', tokens.colorBgContainer ?? (isDark ? 'rgba(45, 50, 78, 0.18)' : 'rgba(255, 255, 255, 0.16)'));
    
    // Text colors
    root.style.setProperty('--rh-text', tokens.colorText ?? 'rgba(0, 0, 0, 0.90)');
    root.style.setProperty('--rh-text-secondary', tokens.colorTextSecondary ?? 'rgba(0, 0, 0, 0.70)');
    root.style.setProperty('--rh-text-tertiary', tokens.colorTextTertiary ?? 'rgba(0, 0, 0, 0.50)');
    
    // Borders
    root.style.setProperty('--rh-border', tokens.colorBorder ?? 'rgba(0, 0, 0, 0.08)');
    root.style.setProperty('--rh-border-secondary', tokens.colorBorderSecondary ?? 'rgba(0, 0, 0, 0.04)');
    root.style.setProperty('--rh-border-radius', String(tokens.borderRadius ?? 10) + 'px');
    
    // Fluent Design accent glows and states
    root.style.setProperty('--rh-accent', tokens.colorPrimary ?? '#6367FF');
    root.style.setProperty('--rh-accent-glow', 'rgba(99, 103, 255, 0.22)');
    root.style.setProperty('--rh-active', '#7ee6b7');
    root.style.setProperty('--rh-active-glow', 'rgba(126, 230, 183, 0.22)');
    root.style.setProperty('--rh-danger-pastel', '#FFDBFD');
    
    // Muted backgrounds for subtle elements
    root.style.setProperty('--rh-muted-bg', isDark ? 'rgba(255, 255, 255, 0.06)' : 'rgba(0, 0, 0, 0.04)');
    root.style.setProperty('--rh-muted-bg-light', isDark ? 'rgba(255, 255, 255, 0.04)' : 'rgba(0, 0, 0, 0.03)');
    
    // Status colors
    root.style.setProperty('--rh-success', tokens.colorSuccess ?? '#52c41a');
    root.style.setProperty('--rh-error', tokens.colorError ?? '#ff4d4f');
  } catch (e) {
    // ignore in environments without DOM
  }
}
