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
    colorBgLayout: 'transparent',
    // Glassmorphism surfaces: low alpha with blur from CSS layer.
    colorBgContainer: semantic.surface.minimalBgStrong,
    colorBgElevated: isDark ? semantic.surface.minimalBgStrong : 'rgba(255, 255, 255, 1)',
    // Border — Fluent enhanced opacity
    colorBorder: isDark ? 'rgba(209, 212, 255, 0.22)' : 'rgba(99, 103, 255, 0.2)',
    colorBorderSecondary: isDark ? 'rgba(209, 212, 255, 0.14)' : 'rgba(99, 103, 255, 0.14)',
    // Text hierarchy — Fluent refined for better contrast with higher opacity backgrounds
    colorText: isDark ? 'rgba(255, 255, 255, 0.92)' : 'rgba(0, 0, 0, 0.92)',
    colorTextSecondary: isDark ? 'rgba(255, 255, 255, 0.72)' : 'rgba(0, 0, 0, 0.72)',
    colorTextTertiary: isDark ? 'rgba(255, 255, 255, 0.52)' : 'rgba(0, 0, 0, 0.52)',
    colorTextQuaternary: isDark ? 'rgba(255, 255, 255, 0.35)' : 'rgba(0, 0, 0, 0.35)',
    // Fluent Design accent colors
    colorSuccess: semantic.status.success,
    colorError: semantic.status.error,
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
    root.style.setProperty('--rh-bg-elevated', tokens.colorBgElevated ?? semantic.surface.glassBgStrong);
    root.style.setProperty('--rh-bg-container', tokens.colorBgContainer ?? semantic.surface.glassBg);
    
    // Text colors
    root.style.setProperty('--rh-text', tokens.colorText ?? 'rgba(0, 0, 0, 0.90)');
    root.style.setProperty('--rh-text-secondary', tokens.colorTextSecondary ?? 'rgba(0, 0, 0, 0.70)');
    root.style.setProperty('--rh-text-tertiary', tokens.colorTextTertiary ?? 'rgba(0, 0, 0, 0.50)');
    
    // Borders
    root.style.setProperty('--rh-border', tokens.colorBorder ?? 'rgba(0, 0, 0, 0.08)');
    root.style.setProperty('--rh-border-secondary', tokens.colorBorderSecondary ?? 'rgba(0, 0, 0, 0.04)');
    root.style.setProperty('--rh-border-radius', String(tokens.borderRadius ?? 10) + 'px');
    
    // Fluent Design accent glows and states
    root.style.setProperty('--rh-accent', tokens.colorPrimary ?? semantic.interactive.accent);
    
    // Status colors
    root.style.setProperty('--rh-success', tokens.colorSuccess ?? semantic.status.success);
    root.style.setProperty('--rh-error', tokens.colorError ?? semantic.status.error);
  } catch (e) {
    // ignore in environments without DOM
  }
}
