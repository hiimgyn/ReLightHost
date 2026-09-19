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
      bg: 'rgba(244, 63, 94, 0.14)',
      border: 'rgba(244, 63, 94, 0.32)',
    },
    bypassed: {
      color: token.colorTextTertiary,
      bg: token.colorFillQuaternary,
      border: token.colorBorderSecondary,
    },
    live: {
      color: token.colorWarning,
      bg: 'rgba(245, 158, 11, 0.14)',
      border: 'rgba(245, 158, 11, 0.32)',
    },
    active: {
      color: token.colorSuccess,
      bg: 'rgba(16, 185, 129, 0.14)',
      border: 'rgba(16, 185, 129, 0.32)',
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
