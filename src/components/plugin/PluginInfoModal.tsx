import { Modal, Button, Tag, Typography, Space, Descriptions, Tooltip, theme, Grid } from 'antd';
import { FolderOpen, Info, Boxes } from 'lucide-react';
import type { PluginInfo } from '../../lib/types';
import { getFormatColor } from './pluginLibraryHelpers';
import { useTranslation } from '../../i18n';

const { Text } = Typography;

interface PluginInfoModalProps {
  plugin: PluginInfo;
  isOpen: boolean;
  onClose: () => void;
  onLoad?: () => void;
}

export default function PluginInfoModal({ plugin, isOpen, onClose, onLoad }: PluginInfoModalProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();

  const modalWidth = typeof window === 'undefined' ? 600 : 'clamp(480px, 58vw, 620px)';
  const screens = Grid.useBreakpoint();
  const descColumns = screens.md ? 2 : 1;

  return (
    <Modal
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 32, maxWidth: 620 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 180px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '16px 22px 22px',
        },
      }}
      title={
        <Space size={12} align="center">
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 10,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${token.colorPrimary}1c`,
              border: `1px solid ${token.colorPrimary}38`,
            }}
          >
            <Boxes size={18} style={{ color: token.colorPrimary }} />
          </div>
          <div>
            <Space size={8} align="center">
              <Text strong style={{ fontSize: 16, lineHeight: 1.2 }}>{plugin.name}</Text>
              <Tag color={getFormatColor(plugin.format)} style={{ margin: 0 }}>
                {plugin.format.toUpperCase()}
              </Tag>
            </Space>
            <Text type="secondary" style={{ fontSize: 12, display: 'block' }}>
              {plugin.manufacture || t('info.unknownDeveloper')}
            </Text>
          </div>
        </Space>
      }
      footer={
        <Space>
          <Button onClick={onClose} style={{ minWidth: 70, borderRadius: 8 }}>
            {t('info.close')}
          </Button>
          {onLoad && (
            <Button
              type="primary"
              onClick={() => {
                onLoad();
                onClose();
              }}
              style={{ borderRadius: 8 }}
            >
              {t('info.loadPlugin')}
            </Button>
          )}
        </Space>
      }
    >
      <Descriptions
        column={descColumns}
        size="small"
        bordered
        style={{ marginBottom: 16 }}
        labelStyle={{ fontWeight: 600, width: 120 }}
      >
        <Descriptions.Item label={t('info.manufacturer')}>{plugin.manufacture || '—'}</Descriptions.Item>
        <Descriptions.Item label={t('info.version')}>{plugin.version || '—'}</Descriptions.Item>
        <Descriptions.Item label={t('info.format')}>
          <Tag color={getFormatColor(plugin.format)} style={{ margin: 0 }}>
            {plugin.format.toUpperCase()}
          </Tag>
        </Descriptions.Item>
        <Descriptions.Item label={t('info.category')}>{plugin.category || '—'}</Descriptions.Item>
        <Descriptions.Item label={t('info.pluginId')} span={descColumns}>
          <Tooltip title={plugin.id}>
            <Text
              code
              style={{
                fontSize: 11,
                fontFamily: 'JetBrains Mono, monospace',
                maxWidth: 400,
                display: 'inline-block',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                verticalAlign: 'bottom',
              }}
            >
              {plugin.id}
            </Text>
          </Tooltip>
        </Descriptions.Item>
        <Descriptions.Item
          label={
            <Space size={4}>
              <FolderOpen size={14} />
              <span>{t('info.filePath')}</span>
            </Space>
          }
          span={descColumns}
        >
          <Tooltip title={plugin.path}>
            <Text
              code
              style={{
                fontSize: 11,
                fontFamily: 'JetBrains Mono, monospace',
                maxWidth: 400,
                display: 'inline-block',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                verticalAlign: 'bottom',
              }}
            >
              {plugin.path}
            </Text>
          </Tooltip>
        </Descriptions.Item>
      </Descriptions>

      <div
        style={{
          background: `${token.colorPrimary}12`,
          border: `1px solid ${token.colorPrimary}2e`,
          borderRadius: 10,
          padding: '12px 16px',
        }}
      >
        <Space align="start" size={12}>
          <Info size={16} style={{ color: token.colorPrimary, marginTop: 2, flexShrink: 0 }} />
          <Text style={{ fontSize: 12.5, lineHeight: 1.5, color: token.colorTextSecondary }}>
            {t('info.notice', { format: plugin.format.toUpperCase() })}
          </Text>
        </Space>
      </div>
    </Modal>
  );
}
