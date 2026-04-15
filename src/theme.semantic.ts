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
  primary: '#6367FF',
  primarySoft: '#8494FF',
  info: '#8494FF',
  success: '#6ec8a6',
  error: '#ef7f92',
  warning: '#faad14',
  washLilac: '#C9BEFF',
  washRose: '#FFDBFD',
  ink: '#1F2333',
  slate: '#8B92A8',
  mist: '#EEF1FA',
};

const LIGHT_MAP: SemanticColorMap = {
  brand: BRAND,
  surface: {
    glassBg: 'rgba(255, 255, 255, 0.16)',
    glassBgStrong: 'rgba(255, 255, 255, 0.22)',
    glassBorder: 'rgba(255, 255, 255, 0.34)',
    glassShadow: '0 8px 32px rgba(15, 23, 42, 0.12)',
    glassInset:
      'inset 0 1px 0 rgba(255, 255, 255, 0.56), inset 0 -1px 0 rgba(255, 255, 255, 0.14), inset 0 0 12px 6px rgba(255, 255, 255, 0.4)',
    minimalBg: 'rgba(255, 255, 255, 0.92)',
    minimalBgStrong: 'rgba(250, 251, 255, 0.98)',
    minimalBorder: 'rgba(99, 103, 255, 0.14)',
    minimalShadow: '0 8px 24px rgba(15, 23, 42, 0.06)',
    tooltipBg: 'rgba(255, 255, 255, 0.96)',
    tooltipSurface: 'linear-gradient(180deg, rgba(255,255,255,0.96) 0%, rgba(247,249,255,0.92) 100%)',
    tooltipBorder: 'rgba(99, 103, 255, 0.18)',
    tooltipShadow: '0 12px 28px rgba(15, 23, 42, 0.14)',
    softGradient: 'linear-gradient(135deg, rgba(255,255,255,0.2) 0%, rgba(255,255,255,0.14) 100%)',
    softBorder: 'rgba(255,255,255,0.3)',
    softBorderStrong: 'rgba(255,255,255,0.34)',
    mutedBg: 'rgba(0, 0, 0, 0.04)',
    mutedBgLight: 'rgba(0, 0, 0, 0.03)',
    textMuted: 'rgba(0, 0, 0, 0.52)',
    dividerSubtle: 'rgba(0, 0, 0, 0.14)',
    insetSoft: 'inset 0 1px 0 rgba(255,255,255,0.16)',
  },
  interactive: {
    accent: BRAND.primary,
    accentGlow: 'rgba(99, 103, 255, 0.22)',
    active: '#7ee6b7',
    activeGlow: 'rgba(126, 230, 183, 0.22)',
    dangerPastel: BRAND.washRose,
  },
  status: {
    success: BRAND.success,
    error: BRAND.error,
    warning: BRAND.warning,
    dangerBg: 'rgba(255,77,79,0.15)',
    dangerBorder: 'rgba(255,77,79,0.26)',
    warningBg: 'rgba(250,173,20,0.18)',
    warningBorder: 'rgba(250,173,20,0.3)',
    successBg: 'rgba(110,200,166,0.18)',
    successBorder: 'rgba(110,200,166,0.3)',
  },
  emphasis: {
    headerShadow: '0 1px 8px rgba(15,23,42,0.05), inset 0 1px 0 rgba(255,255,255,0.14)',
    footerShadow: '0 -1px 8px rgba(15,23,42,0.05), inset 0 1px 0 rgba(255,255,255,0.14)',
    chainToolbarShadow: '0 8px 20px rgba(15,23,42,0.06), inset 0 1px 0 rgba(255,255,255,0.16)',
    chainPanelShadow: '0 8px 24px rgba(15,23,42,0.08), inset 0 1px 0 rgba(255,255,255,0.16)',
    chainWellBg: 'linear-gradient(135deg, rgba(255,255,255,0.18) 0%, rgba(255,255,255,0.14) 100%)',
    chainWellBorder: 'rgba(0,0,0,0.15)',
    chainInsertBg: 'linear-gradient(135deg, rgba(99,103,255,0.26), rgba(132,148,255,0.22))',
  },
};

const DARK_MAP: SemanticColorMap = {
  ...LIGHT_MAP,
  brand: BRAND,
  surface: {
    glassBg: 'rgba(45, 50, 78, 0.18)',
    glassBgStrong: 'rgba(58, 64, 96, 0.24)',
    glassBorder: 'rgba(210, 216, 255, 0.26)',
    glassShadow: '0 8px 32px rgba(0, 0, 0, 0.28)',
    glassInset:
      'inset 0 1px 0 rgba(255, 255, 255, 0.2), inset 0 -1px 0 rgba(255, 255, 255, 0.08), inset 0 0 12px 6px rgba(167, 180, 255, 0.12)',
    minimalBg: 'rgba(18, 20, 34, 0.92)',
    minimalBgStrong: 'rgba(28, 31, 48, 0.96)',
    minimalBorder: 'rgba(132, 148, 255, 0.16)',
    minimalShadow: '0 10px 32px rgba(0, 0, 0, 0.34)',
    tooltipBg: 'rgba(18, 20, 34, 0.96)',
    tooltipSurface: 'linear-gradient(180deg, rgba(28,31,48,0.96) 0%, rgba(18,20,34,0.94) 100%)',
    tooltipBorder: 'rgba(132, 148, 255, 0.22)',
    tooltipShadow: '0 12px 28px rgba(0, 0, 0, 0.42)',
    softGradient: 'linear-gradient(135deg, rgba(58,64,96,0.24) 0%, rgba(45,50,78,0.18) 100%)',
    softBorder: 'rgba(210,216,255,0.24)',
    softBorderStrong: 'rgba(210,216,255,0.28)',
    mutedBg: 'rgba(255, 255, 255, 0.06)',
    mutedBgLight: 'rgba(255, 255, 255, 0.04)',
    textMuted: 'rgba(255, 255, 255, 0.52)',
    dividerSubtle: 'rgba(255, 255, 255, 0.16)',
    insetSoft: 'inset 0 1px 0 rgba(255,255,255,0.08)',
  },
  emphasis: {
    headerShadow: '0 1px 8px rgba(0,0,0,0.18), inset 0 1px 0 rgba(255,255,255,0.04)',
    footerShadow: '0 -1px 8px rgba(0,0,0,0.18), inset 0 1px 0 rgba(255,255,255,0.04)',
    chainToolbarShadow: '0 8px 24px rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.05)',
    chainPanelShadow: '0 8px 32px rgba(0,0,0,0.25), inset 0 1px 0 rgba(255,255,255,0.05)',
    chainWellBg: 'linear-gradient(135deg, rgba(58,64,96,0.22) 0%, rgba(45,50,78,0.16) 100%)',
    chainWellBorder: 'rgba(255,255,255,0.12)',
    chainInsertBg: 'linear-gradient(135deg, rgba(99,103,255,0.34), rgba(132,148,255,0.28))',
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
