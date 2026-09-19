import { Tag, Tooltip, theme } from 'antd';
import {
  Boxes,
  Building2,
  Hash,
  Tag as TagIcon,
} from 'lucide-react';
import type { CSSProperties, ReactNode } from 'react';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';

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

function normalizeManufacturerLabel(format: PluginInstanceInfo['format'], value?: string, systemLabel = 'System') {
  if (!value) return null;
  const normalized = value.trim();
  if (!normalized) return null;
  if (format === 'builtin' && normalized.toLowerCase().includes('built')) {
    return systemLabel;
  }
  return normalized;
}

function metaChipIconFor(key: string): ReactNode {
  switch (key) {
    case 'format':
      return <Boxes size={10} />;
    case 'manufacture':
      return <Building2 size={10} />;
    case 'version':
      return <Hash size={10} />;
    case 'category':
      return <TagIcon size={10} />;
    default:
      return <TagIcon size={10} />;
  }
}

interface PluginMetaChipsProps {
  plugin: PluginInstanceInfo;
}

/** Row of small tags: format, manufacturer, version, category. */
export default function PluginMetaChips({ plugin }: PluginMetaChipsProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const formatPalette = getFormatPalette(plugin.format);

  const metaChipStyleFor = (chip: MetaChip): CSSProperties => {
    const isPrimary = chip.key === 'format';
    const isManufacturer = chip.key === 'manufacture';

    return {
      display: 'inline-flex',
      alignItems: 'center',
      gap: 4,
      minWidth: 0,
      maxWidth: isPrimary ? 80 : 110,
      padding: isPrimary ? '2px 8px' : '1px 6px',
      margin: 0,
      borderRadius: 6,
      border: `1px solid ${isPrimary ? formatPalette.border : token.colorBorderSecondary}`,
      background: isPrimary
        ? formatPalette.background
        : isManufacturer
        ? token.colorBgContainer
        : token.colorFillQuaternary,
      color: isPrimary ? formatPalette.color : token.colorTextSecondary,
      fontSize: isPrimary ? 9.5 : 9,
      fontWeight: isPrimary ? 700 : 500,
      letterSpacing: isPrimary ? 0.4 : 0.1,
      textTransform: isPrimary ? 'uppercase' : 'none',
      boxShadow: 'none',
      overflow: 'hidden',
      whiteSpace: 'nowrap',
      textOverflow: 'ellipsis',
      // Never shrink below maxWidth — the row scrolls instead, so a chip's
      // text is either shown in full or ellipsized on its own terms, never
      // squeezed to illegibility by neighboring chips.
      flexShrink: 0,
      lineHeight: 1.2,
      height: 20,
    };
  };

  const manufacturerLabel = normalizeManufacturerLabel(plugin.format, plugin.manufacture, t('common.system'));

  const metaChips: MetaChip[] = [
    { key: 'format', label: plugin.format === 'builtin' ? t('common.system') : plugin.format.toUpperCase(), icon: metaChipIconFor('format') },
    manufacturerLabel
      ? { key: 'manufacture', label: manufacturerLabel, tooltip: plugin.manufacture, icon: metaChipIconFor('manufacture') }
      : null,
    plugin.category && plugin.category !== 'Unknown'
      ? { key: 'category', label: plugin.category, tooltip: plugin.category, icon: metaChipIconFor('category') }
      : null,
    plugin.version
      ? { key: 'version', label: `v${plugin.version}`, tooltip: `v${plugin.version}`, icon: metaChipIconFor('version') }
      : null,
  ].filter((chip): chip is MetaChip => chip !== null);

  return (
    <div
      className="rh-meta-chip-scroll"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        flexWrap: 'nowrap',
        overflowX: 'auto',
        overflowY: 'hidden',
        width: '100%',
        minHeight: 22,
        // Fades the row's trailing edge instead of hard-clipping chips, so an
        // overflowing chip reads as "scroll for more" rather than vanishing.
        WebkitMaskImage: 'linear-gradient(90deg, #000 calc(100% - 18px), transparent 100%)',
        maskImage: 'linear-gradient(90deg, #000 calc(100% - 18px), transparent 100%)',
      }}
    >
      {metaChips.map((chip) => (
        <Tooltip title={chip.tooltip ?? chip.label} key={chip.key}>
          <Tag color="default" style={metaChipStyleFor(chip)}>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 3, minWidth: 0, overflow: 'hidden' }}>
              <span style={{ display: 'inline-flex', alignItems: 'center', fontSize: 9, opacity: 0.85, flexShrink: 0 }}>
                {chip.icon}
              </span>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {chip.label}
              </span>
            </span>
          </Tag>
        </Tooltip>
      ))}
    </div>
  );
}
