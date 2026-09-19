import { Button, Popconfirm, Space, Tooltip, Typography, theme } from 'antd';
import { Plus, Trash2, AudioWaveform, ArrowLeftRight } from 'lucide-react';
import { useTranslation } from '../../i18n';

const { Text } = Typography;

interface ChainToolbarProps {
  isChainInitializing: boolean;
  addLocked: boolean;
  isDeleteAllBusy: boolean;
  pluginChainLength: number;
  onAddPlugin: () => void;
  onDeleteAll: () => void;
}

export default function ChainToolbar({
  isChainInitializing,
  addLocked,
  isDeleteAllBusy,
  pluginChainLength,
  onAddPlugin,
  onDeleteAll,
}: ChainToolbarProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();

  return (
    <div
      style={{
        padding: '12px 20px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 16,
        flexWrap: 'wrap',
        borderBottom: `1px solid ${token.colorBorderSecondary}`,
        background: 'rgba(0, 0, 0, 0.02)',
        flexShrink: 0,
      }}
    >
      {/* Title & Count Badge */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: 8,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            background: `${token.colorPrimary}18`,
            border: `1px solid ${token.colorPrimary}32`,
            color: token.colorPrimary,
            flexShrink: 0,
          }}
        >
          <AudioWaveform size={17} strokeWidth={2.2} />
        </div>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Text
              strong
              style={{
                fontSize: 16,
                fontWeight: 700,
                letterSpacing: '-0.02em',
                color: token.colorText,
                lineHeight: 1.2,
              }}
            >
              {t('chain.title')}
            </Text>
            <span
              style={{
                fontSize: 11,
                fontWeight: 700,
                padding: '1px 8px',
                borderRadius: 999,
                background: `${token.colorPrimary}18`,
                color: token.colorPrimary,
                border: `1px solid ${token.colorPrimary}30`,
              }}
            >
              {t('footer.pluginsCount', { count: pluginChainLength, plural: pluginChainLength !== 1 ? 's' : '' })}
            </span>
          </div>
          <Text style={{ fontSize: 11, color: token.colorTextTertiary, display: 'block', lineHeight: 1.2, marginTop: 2 }}>
            {t('chain.subtitle')}
          </Text>
        </div>
      </div>

      {/* Middle: Reorder Hint (only shown when 2+ plugins exist) */}
      {pluginChainLength > 1 && (
        <div
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: 6,
            padding: '4px 10px',
            borderRadius: 8,
            background: token.colorFillQuaternary,
            color: token.colorTextTertiary,
            fontSize: 11,
            fontWeight: 500,
          }}
        >
          <ArrowLeftRight size={13} style={{ color: token.colorPrimary }} />
          <span>{t('chain.reorderHint')}</span>
        </div>
      )}

      {/* Action Buttons */}
      <Space size={8} wrap>
        <Tooltip title={isChainInitializing ? t('chain.preparing') : t('chain.addPlugin')}>
          <Button
            type="primary"
            icon={<Plus size={15} strokeWidth={2.5} />}
            size="middle"
            className="btn-pill"
            loading={addLocked}
            disabled={addLocked}
            onClick={onAddPlugin}
            aria-label={isChainInitializing ? t('chain.preparing') : t('chain.addPlugin')}
          >
            {t('chain.addPlugin')}
          </Button>
        </Tooltip>

        <Popconfirm
          title={t('chain.removeAllConfirm')}
          onConfirm={onDeleteAll}
          okText={t('chain.remove')}
          cancelText={t('chain.cancel')}
        >
          <Tooltip title={t('chain.removeAllConfirm')}>
            <Button
              size="middle"
              type="text"
              danger
              icon={<Trash2 size={16} />}
              loading={isDeleteAllBusy}
              disabled={pluginChainLength === 0 || isDeleteAllBusy || isChainInitializing}
              style={{
                borderRadius: 8,
              }}
            />
          </Tooltip>
        </Popconfirm>
      </Space>
    </div>
  );
}
