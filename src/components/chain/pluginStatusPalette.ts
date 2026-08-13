import type { GlobalToken } from 'antd';
import type { PluginInstanceInfo, PluginStatus } from '../../lib/types';

export type PluginStatusKind = 'crashed' | 'bypassed' | 'live' | 'active';

export interface PluginStatusPalette {
  isCrashed: boolean;
  isActive: boolean;
  statusKind: PluginStatusKind;
  statusText: string;
  color: string;
  bg: string;
  border: string;
  bypassButtonColor: string;
  bypassButtonBg: string;
  bypassButtonBorder: string;
}

/** Derive the crashed/bypassed/live/active visual state for a plugin card. */
export function getPluginStatusPalette(
  plugin: PluginInstanceInfo,
  crashStatus: PluginStatus | undefined,
  token: GlobalToken,
): PluginStatusPalette {
  const effectiveCrashStatus = crashStatus ?? { type: 'Ok' as const };
  const isCrashed = effectiveCrashStatus.type !== 'Ok';
  const isActive = !isCrashed && !plugin.bypassed;
  const statusKind: PluginStatusKind = isCrashed
    ? 'crashed'
    : plugin.bypassed
    ? 'bypassed'
    : plugin.gui_open
    ? 'live'
    : 'active';

  const paletteByKind: Record<PluginStatusKind, { color: string; bg: string; border: string }> = {
    crashed: {
      color: token.colorError,
      bg: 'rgba(255,77,79,0.12)',
      border: 'rgba(255,77,79,0.26)',
    },
    bypassed: {
      color: token.colorTextTertiary,
      bg: token.colorFillQuaternary,
      border: token.colorBorderSecondary,
    },
    live: {
      color: token.colorWarning,
      bg: 'rgba(250,173,20,0.14)',
      border: 'rgba(250,173,20,0.3)',
    },
    active: {
      color: token.colorSuccess,
      bg: 'rgba(110,200,166,0.12)',
      border: 'rgba(110,200,166,0.3)',
    },
  };
  const palette = paletteByKind[statusKind];

  const statusText = statusKind === 'crashed'
    ? 'Crashed'
    : statusKind === 'bypassed'
    ? 'Bypassed'
    : statusKind === 'live'
    ? 'Live'
    : 'Active';

  return {
    isCrashed,
    isActive,
    statusKind,
    statusText,
    color: palette.color,
    bg: palette.bg,
    border: palette.border,
    bypassButtonColor: palette.color,
    bypassButtonBg: statusKind === 'bypassed' ? token.colorBgContainer : palette.bg,
    bypassButtonBorder: palette.border,
  };
}
