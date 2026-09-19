export interface SemanticColorMap {
  brand: {
    primary: string;
    primarySoft: string;
    info: string;
    success: string;
    error: string;
    warning: string;
    washLilac: string;
    washRose: string;
    ink: string;
    slate: string;
    mist: string;
  };
  surface: {
    glassBg: string;
    glassBgStrong: string;
    glassBorder: string;
    glassShadow: string;
    glassInset: string;
    minimalBg: string;
    minimalBgStrong: string;
    minimalBorder: string;
    minimalShadow: string;
    tooltipBg: string;
    tooltipSurface: string;
    tooltipBorder: string;
    tooltipShadow: string;
    softGradient: string;
    softBorder: string;
    softBorderStrong: string;
    mutedBg: string;
    mutedBgLight: string;
    textMuted: string;
    dividerSubtle: string;
    insetSoft: string;
  };
  interactive: {
    accent: string;
    accentGlow: string;
    active: string;
    activeGlow: string;
    dangerPastel: string;
  };
  status: {
    success: string;
    error: string;
    warning: string;
    dangerBg: string;
    dangerBorder: string;
    warningBg: string;
    warningBorder: string;
    successBg: string;
    successBorder: string;
  };
  emphasis: {
    headerShadow: string;
    footerShadow: string;
    chainToolbarShadow: string;
    chainPanelShadow: string;
    chainWellBg: string;
    chainWellBorder: string;
    chainInsertBg: string;
  };
}

const BRAND = {
  primary: '#6366f1',
  primarySoft: '#818cf8',
  info: '#38bdf8',
  success: '#10b981',
  error: '#f43f5e',
  warning: '#f59e0b',
  washLilac: '#c7d2fe',
  washRose: '#fecdd3',
  ink: '#0f172a',
  slate: '#64748b',
  mist: '#f1f5f9',
};

const LIGHT_MAP: SemanticColorMap = {
  brand: BRAND,
  surface: {
    glassBg: 'rgba(255, 255, 255, 0.85)',
    glassBgStrong: 'rgba(255, 255, 255, 0.95)',
    glassBorder: 'rgba(99, 102, 241, 0.16)',
    glassShadow: '0 8px 30px rgba(15, 23, 42, 0.08)',
    glassInset: 'none',
    minimalBg: '#ffffff',
    minimalBgStrong: '#f8fafc',
    minimalBorder: 'rgba(99, 102, 241, 0.14)',
    minimalShadow: '0 4px 16px rgba(15, 23, 42, 0.05)',
    tooltipBg: '#1e293b',
    tooltipSurface: '#1e293b',
    tooltipBorder: 'rgba(99, 102, 241, 0.24)',
    tooltipShadow: '0 10px 24px rgba(15, 23, 42, 0.12)',
    softGradient: 'linear-gradient(180deg, rgba(255, 255, 255, 0.95) 0%, rgba(248, 250, 252, 0.90) 100%)',
    softBorder: 'rgba(99, 102, 241, 0.14)',
    softBorderStrong: 'rgba(99, 102, 241, 0.22)',
    mutedBg: 'rgba(15, 23, 42, 0.04)',
    mutedBgLight: 'rgba(15, 23, 42, 0.02)',
    textMuted: 'rgba(15, 23, 42, 0.50)',
    dividerSubtle: 'rgba(15, 23, 42, 0.08)',
    insetSoft: 'none',
  },
  interactive: {
    accent: BRAND.primary,
    accentGlow: 'rgba(99, 102, 241, 0.25)',
    active: '#10b981',
    activeGlow: 'rgba(16, 185, 129, 0.25)',
    dangerPastel: BRAND.washRose,
  },
  status: {
    success: BRAND.success,
    error: BRAND.error,
    warning: BRAND.warning,
    dangerBg: 'rgba(244, 63, 94, 0.10)',
    dangerBorder: 'rgba(244, 63, 94, 0.25)',
    warningBg: 'rgba(245, 158, 11, 0.10)',
    warningBorder: 'rgba(245, 158, 11, 0.25)',
    successBg: 'rgba(16, 185, 129, 0.10)',
    successBorder: 'rgba(16, 185, 129, 0.25)',
  },
  emphasis: {
    headerShadow: '0 1px 0 rgba(15, 23, 42, 0.06)',
    footerShadow: '0 -1px 0 rgba(15, 23, 42, 0.06)',
    chainToolbarShadow: 'none',
    chainPanelShadow: '0 8px 24px rgba(15, 23, 42, 0.05)',
    chainWellBg: 'linear-gradient(180deg, #f8fafc 0%, #f1f5f9 100%)',
    chainWellBorder: 'rgba(99, 102, 241, 0.12)',
    chainInsertBg: 'rgba(99, 102, 241, 0.15)',
  },
};

const DARK_MAP: SemanticColorMap = {
  brand: {
    primary: '#6366f1',
    primarySoft: '#818cf8',
    info: '#38bdf8',
    success: '#10b981',
    error: '#f43f5e',
    warning: '#f59e0b',
    washLilac: '#818cf8',
    washRose: '#fb7185',
    ink: '#f8fafc',
    slate: '#94a3b8',
    mist: '#0f172a',
  },
  surface: {
    glassBg: 'rgba(20, 24, 36, 0.85)',
    glassBgStrong: 'rgba(25, 30, 45, 0.95)',
    glassBorder: 'rgba(255, 255, 255, 0.09)',
    glassShadow: '0 12px 36px rgba(0, 0, 0, 0.65)',
    glassInset: 'none',
    minimalBg: '#111420',
    minimalBgStrong: '#161a28',
    minimalBorder: 'rgba(255, 255, 255, 0.08)',
    minimalShadow: '0 10px 30px rgba(0, 0, 0, 0.55)',
    tooltipBg: '#1e2436',
    tooltipSurface: '#1e2436',
    tooltipBorder: 'rgba(99, 102, 241, 0.35)',
    tooltipShadow: '0 12px 32px rgba(0, 0, 0, 0.75)',
    softGradient: 'linear-gradient(180deg, rgba(22, 26, 38, 0.9) 0%, rgba(17, 20, 31, 0.9) 100%)',
    softBorder: 'rgba(255, 255, 255, 0.08)',
    softBorderStrong: 'rgba(99, 102, 241, 0.32)',
    mutedBg: 'rgba(255, 255, 255, 0.05)',
    mutedBgLight: 'rgba(255, 255, 255, 0.03)',
    textMuted: 'rgba(255, 255, 255, 0.55)',
    dividerSubtle: 'rgba(255, 255, 255, 0.08)',
    insetSoft: 'none',
  },
  interactive: {
    accent: '#6366f1',
    accentGlow: 'rgba(99, 102, 241, 0.35)',
    active: '#10b981',
    activeGlow: 'rgba(16, 185, 129, 0.35)',
    dangerPastel: '#f43f5e',
  },
  status: {
    success: '#10b981',
    error: '#f43f5e',
    warning: '#f59e0b',
    dangerBg: 'rgba(244, 63, 94, 0.14)',
    dangerBorder: 'rgba(244, 63, 94, 0.32)',
    warningBg: 'rgba(245, 158, 11, 0.14)',
    warningBorder: 'rgba(245, 158, 11, 0.32)',
    successBg: 'rgba(16, 185, 129, 0.14)',
    successBorder: 'rgba(16, 185, 129, 0.32)',
  },
  emphasis: {
    headerShadow: '0 1px 0 rgba(255, 255, 255, 0.07)',
    footerShadow: '0 -1px 0 rgba(255, 255, 255, 0.07)',
    chainToolbarShadow: 'none',
    chainPanelShadow: '0 14px 40px rgba(0, 0, 0, 0.6)',
    chainWellBg: 'linear-gradient(180deg, #0b0e17 0%, #090b12 100%)',
    chainWellBorder: 'rgba(255, 255, 255, 0.07)',
    chainInsertBg: 'rgba(99, 102, 241, 0.25)',
  },
};

export function getSemanticColorMap(isDark: boolean): SemanticColorMap {
  return isDark ? DARK_MAP : LIGHT_MAP;
}

export function applySemanticCssVars(root: HTMLElement, map: SemanticColorMap) {
  root.style.setProperty('--rh-primary', map.brand.primary);
  root.style.setProperty('--rh-info', map.brand.info);
  root.style.setProperty('--rh-primary-soft', map.brand.primarySoft);
  root.style.setProperty('--rh-wash-lilac', map.brand.washLilac);
  root.style.setProperty('--rh-wash-rose', map.brand.washRose);
  root.style.setProperty('--rh-minimal-ink', map.brand.ink);
  root.style.setProperty('--rh-minimal-slate', map.brand.slate);
  root.style.setProperty('--rh-minimal-mist', map.brand.mist);

  root.style.setProperty('--rh-glass-bg', map.surface.glassBg);
  root.style.setProperty('--rh-glass-bg-strong', map.surface.glassBgStrong);
  root.style.setProperty('--rh-glass-border', map.surface.glassBorder);
  root.style.setProperty('--rh-glass-shadow', map.surface.glassShadow);
  root.style.setProperty('--rh-glass-inset', map.surface.glassInset);

  root.style.setProperty('--rh-minimal-bg', map.surface.minimalBg);
  root.style.setProperty('--rh-minimal-bg-strong', map.surface.minimalBgStrong);
  root.style.setProperty('--rh-minimal-border', map.surface.minimalBorder);
  root.style.setProperty('--rh-minimal-shadow', map.surface.minimalShadow);

  root.style.setProperty('--rh-tooltip-bg', map.surface.tooltipBg);
  root.style.setProperty('--rh-tooltip-surface', map.surface.tooltipSurface);
  root.style.setProperty('--rh-tooltip-border', map.surface.tooltipBorder);
  root.style.setProperty('--rh-tooltip-shadow', map.surface.tooltipShadow);

  root.style.setProperty('--rh-surface-soft-gradient', map.surface.softGradient);
  root.style.setProperty('--rh-surface-soft-border', map.surface.softBorder);
  root.style.setProperty('--rh-surface-soft-border-strong', map.surface.softBorderStrong);
  root.style.setProperty('--rh-divider-subtle', map.surface.dividerSubtle);
  root.style.setProperty('--rh-inset-soft', map.surface.insetSoft);
  root.style.setProperty('--rh-text-muted', map.surface.textMuted);
  root.style.setProperty('--rh-muted-bg', map.surface.mutedBg);
  root.style.setProperty('--rh-muted-bg-light', map.surface.mutedBgLight);

  root.style.setProperty('--rh-accent', map.interactive.accent);
  root.style.setProperty('--rh-accent-glow', map.interactive.accentGlow);
  root.style.setProperty('--rh-active', map.interactive.active);
  root.style.setProperty('--rh-active-glow', map.interactive.activeGlow);
  root.style.setProperty('--rh-danger-pastel', map.interactive.dangerPastel);

  root.style.setProperty('--rh-success', map.status.success);
  root.style.setProperty('--rh-error', map.status.error);
  root.style.setProperty('--rh-warning', map.status.warning);
  root.style.setProperty('--rh-status-danger-bg', map.status.dangerBg);
  root.style.setProperty('--rh-status-danger-border', map.status.dangerBorder);
  root.style.setProperty('--rh-status-warning-bg', map.status.warningBg);
  root.style.setProperty('--rh-status-warning-border', map.status.warningBorder);
  root.style.setProperty('--rh-status-success-bg', map.status.successBg);
  root.style.setProperty('--rh-status-success-border', map.status.successBorder);

  root.style.setProperty('--rh-header-shadow', map.emphasis.headerShadow);
  root.style.setProperty('--rh-footer-shadow', map.emphasis.footerShadow);
  root.style.setProperty('--rh-chain-toolbar-shadow', map.emphasis.chainToolbarShadow);
  root.style.setProperty('--rh-chain-panel-shadow', map.emphasis.chainPanelShadow);
  root.style.setProperty('--rh-chain-well-bg', map.emphasis.chainWellBg);
  root.style.setProperty('--rh-chain-well-border', map.emphasis.chainWellBorder);
  root.style.setProperty('--rh-chain-insert-bg', map.emphasis.chainInsertBg);
}
