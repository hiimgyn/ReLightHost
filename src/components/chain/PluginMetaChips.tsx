import { Tag, Tooltip, theme } from 'antd';
import {
  AppstoreOutlined,
  ApartmentOutlined,
  FieldNumberOutlined,
  TagOutlined,
} from '@ant-design/icons';
import type { CSSProperties, ReactNode } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';

interface MetaChip {
  key: string;
  label: string;
  tooltip?: string;
  icon: ReactNode;
}

function getFormatPalette(format: PluginInstanceInfo['format']) {
  switch (format) {
    case 'vst3':
      return { border: 'rgba(138, 92, 255, 0.42)', background: 'rgba(138, 92, 255, 0.12)', color: '#8a5cff' };
    case 'clap':
      return { border: 'rgba(34, 197, 94, 0.42)', background: 'rgba(34, 197, 94, 0.12)', color: '#22c55e' };
    case 'builtin':
      return { border: 'rgba(20, 184, 166, 0.42)', background: 'rgba(20, 184, 166, 0.12)', color: '#14b8a6' };
    case 'vst':
    default:
      return { border: 'rgba(245, 158, 11, 0.42)', background: 'rgba(245, 158, 11, 0.12)', color: '#f59e0b' };
  }
}

function normalizeManufacturerLabel(format: PluginInstanceInfo['format'], value?: string) {
  if (!value) return null;
  const normalized = value.trim();
  if (!normalized) return null;
  if (format === 'builtin' && normalized.toLowerCase().includes('built')) {
    return 'System';
  }
  return normalized;
}

function metaChipIconFor(key: string): ReactNode {
  switch (key) {
    case 'format':
      return <AppstoreOutlined />;
    case 'manufacture':
      return <ApartmentOutlined />;
    case 'version':
      return <FieldNumberOutlined />;
    case 'category':
      return <TagOutlined />;
    default:
      return <TagOutlined />;
  }
}

interface PluginMetaChipsProps {
  plugin: PluginInstanceInfo;
}

/** Row of small tags: format, manufacturer, version, category. */
export default function PluginMetaChips({ plugin }: PluginMetaChipsProps) {
  const { token } = theme.useToken();
  const formatPalette = getFormatPalette(plugin.format);

  const metaChipStyleFor = (chip: MetaChip): CSSProperties => {
    const isPrimary = chip.key === 'format';
    const isManufacturer = chip.key === 'manufacture';

    return {
      display: 'inline-flex',
      alignItems: 'center',
      gap: 3,
      minWidth: 0,
      padding: isPrimary ? '2px 6px' : '1px 5px',
      margin: 0,
      borderRadius: 999,
      border: `1px solid ${isPrimary ? formatPalette.border : token.colorBorderSecondary}`,
      background: isPrimary
        ? formatPalette.background
        : isManufacturer
        ? token.colorBgElevated
        : token.colorBgContainer,
      color: isPrimary ? formatPalette.color : token.colorTextSecondary,
      fontSize: isPrimary ? 8.5 : 7.5,
      fontWeight: isPrimary ? 700 : 600,
      letterSpacing: isPrimary ? 0.3 : 0.08,
      textTransform: isPrimary ? 'uppercase' : 'none',
      boxShadow: 'none',
      overflow: 'visible',
      whiteSpace: 'nowrap',
      flexShrink: 0,
      lineHeight: 1,
      minHeight: 18,
    };
  };

  const manufacturerLabel = normalizeManufacturerLabel(plugin.format, plugin.manufacture);
  const metaChips: MetaChip[] = [
    { key: 'format', label: plugin.format === 'builtin' ? 'SYSTEM' : plugin.format.toUpperCase(), icon: metaChipIconFor('format') },
    manufacturerLabel
      ? { key: 'manufacture', label: manufacturerLabel, tooltip: plugin.manufacture, icon: metaChipIconFor('manufacture') }
      : null,
    plugin.version
      ? { key: 'version', label: `v${plugin.version}`, tooltip: `v${plugin.version}`, icon: metaChipIconFor('version') }
      : null,
    plugin.category && plugin.category !== 'Unknown'
      ? { key: 'category', label: plugin.category, tooltip: plugin.category, icon: metaChipIconFor('category') }
      : null,
  ].filter((chip): chip is MetaChip => chip !== null);

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', overflow: 'visible', maxHeight: 30, width: '100%', paddingTop: 1 }}>
      {metaChips.map((chip) => (
        <Tooltip title={chip.tooltip ?? chip.label} key={chip.key}>
          <Tag color="default" style={metaChipStyleFor(chip)}>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 3, minWidth: 0, whiteSpace: 'nowrap' }}>
              <span style={{ display: 'inline-flex', alignItems: 'center', fontSize: 8, opacity: 0.88 }}>
                {chip.icon}
              </span>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{chip.label}</span>
            </span>
          </Tag>
        </Tooltip>
      ))}
    </div>
  );
}
