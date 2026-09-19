import { applySemanticCssVars, getSemanticColorMap } from './theme.semantic';
export type ThemeTokens = Record<string, any>;

export function getThemeTokens(isDark: boolean): ThemeTokens {
  const semantic = getSemanticColorMap(isDark);
  return {
    colorPrimary: semantic.brand.primary,
    colorPrimarySoft: semantic.brand.primarySoft,
    colorInfo: semantic.brand.info,
    colorLink: semantic.brand.primary,
    borderRadius: 10,
    fontFamily:
      '"Inter", "Space Grotesk", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif',
    colorBgLayout: isDark ? '#090b10' : '#f4f6fb',
    colorBgContainer: isDark ? '#141824' : '#ffffff',
    colorBgElevated: isDark ? '#1b2030' : '#ffffff',
    colorBgSpotlight: isDark ? '#242a3e' : '#1e293b',
    // Border — crisp and clean with high precision
    colorBorder: isDark ? 'rgba(255, 255, 255, 0.09)' : 'rgba(99, 102, 241, 0.16)',
    colorBorderSecondary: isDark ? 'rgba(255, 255, 255, 0.05)' : 'rgba(99, 102, 241, 0.08)',
    // Text hierarchy — clear distinction between primary, secondary, and tertiary
    colorText: isDark ? 'rgba(255, 255, 255, 0.95)' : 'rgba(15, 23, 42, 0.92)',
    colorTextSecondary: isDark ? 'rgba(255, 255, 255, 0.68)' : 'rgba(15, 23, 42, 0.68)',
    colorTextTertiary: isDark ? 'rgba(255, 255, 255, 0.46)' : 'rgba(15, 23, 42, 0.46)',
    colorTextQuaternary: isDark ? 'rgba(255, 255, 255, 0.26)' : 'rgba(15, 23, 42, 0.26)',
    // Design accent colors
    colorSuccess: semantic.status.success,
    colorError: semantic.status.error,
    colorWarning: semantic.status.warning,
    // Rhythm unit for consistent spacing
    rhythm: 8,
  };
}

export function applyThemeCssVars(tokens: ThemeTokens, isDark: boolean) {
  try {
    const root = document.documentElement;
    if (!root) return;
    const semantic = getSemanticColorMap(isDark);
    applySemanticCssVars(root, semantic);
    
    // Primary and accent colors
    root.style.setProperty('--rh-primary', tokens.colorPrimary ?? semantic.brand.primary);
    root.style.setProperty('--rh-info', tokens.colorInfo ?? semantic.brand.info);
    root.style.setProperty('--rh-primary-soft', tokens.colorPrimarySoft ?? semantic.brand.primarySoft);
    
    // Background colors
    root.style.setProperty('--rh-bg-layout', tokens.colorBgLayout);
    root.style.setProperty('--rh-bg-elevated', tokens.colorBgElevated ?? (isDark ? '#1b2030' : '#ffffff'));
    root.style.setProperty('--rh-bg-container', tokens.colorBgContainer ?? (isDark ? '#141824' : '#ffffff'));
    
    // Text colors
    root.style.setProperty('--rh-text', tokens.colorText ?? (isDark ? 'rgba(255, 255, 255, 0.95)' : 'rgba(15, 23, 42, 0.92)'));
    root.style.setProperty('--rh-text-secondary', tokens.colorTextSecondary ?? (isDark ? 'rgba(255, 255, 255, 0.68)' : 'rgba(15, 23, 42, 0.68)'));
    root.style.setProperty('--rh-text-tertiary', tokens.colorTextTertiary ?? (isDark ? 'rgba(255, 255, 255, 0.46)' : 'rgba(15, 23, 42, 0.46)'));
    
    // Borders
    root.style.setProperty('--rh-border', tokens.colorBorder ?? (isDark ? 'rgba(255, 255, 255, 0.09)' : 'rgba(99, 102, 241, 0.16)'));
    root.style.setProperty('--rh-border-secondary', tokens.colorBorderSecondary ?? (isDark ? 'rgba(255, 255, 255, 0.05)' : 'rgba(99, 102, 241, 0.08)'));
    root.style.setProperty('--rh-border-radius', String(tokens.borderRadius ?? 10) + 'px');
    
    // Studio accent glows and states
    root.style.setProperty('--rh-accent', tokens.colorPrimary ?? semantic.interactive.accent);
    
    // Status colors
    root.style.setProperty('--rh-success', tokens.colorSuccess ?? semantic.status.success);
    root.style.setProperty('--rh-error', tokens.colorError ?? semantic.status.error);
  } catch (e) {
    // ignore in environments without DOM
  }
}
