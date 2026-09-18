import { Modal, Button, Tag, Typography, Space, Descriptions, theme, Grid } from 'antd';
import { FolderOpenOutlined, InfoCircleOutlined, AppstoreOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../../lib/types';
import { getFormatColor } from './pluginLibraryHelpers';

const { Text } = Typography;

interface PluginInfoModalProps {
  plugin: PluginInfo;
  isOpen: boolean;
  onClose: () => void;
  onLoad?: () => void;
}

export default function PluginInfoModal({ plugin, isOpen, onClose, onLoad }: PluginInfoModalProps) {
  const { token } = theme.useToken();

  const modalWidth = typeof window === 'undefined' ? 448 : 'clamp(300px, 58vw, 448px)';
  const screens = Grid.useBreakpoint();
  const descColumns = screens.md ? 2 : 1;

  return (
    <Modal
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 20, maxWidth: 448 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 200px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '14px 18px 18px',
        },
      }}
      title={
        <Space size={10} align="center">
          <div
            style={{
              width: 32,
              height: 32,
              borderRadius: 8,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${token.colorPrimary}1c`,
              border: `1px solid ${token.colorPrimary}38`,
            }}
          >
            <AppstoreOutlined style={{ color: token.colorPrimary, fontSize: 16 }} />
          </div>
          <div>
            <Space size={6} align="center">
              <Text strong style={{ fontSize: 15, lineHeight: 1.2 }}>{plugin.name}</Text>
              <Tag color={getFormatColor(plugin.format)} style={{ margin: 0 }}>
                {plugin.format.toUpperCase()}
              </Tag>
            </Space>
            <Text type="secondary" style={{ fontSize: 11, display: 'block' }}>
              {plugin.manufacture || 'Unknown developer'}
            </Text>
          </div>
        </Space>
      }
      footer={
        <Space>
          <Button onClick={onClose} style={{ minWidth: 70, borderRadius: 8 }}>
            Close
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
              Load Plugin
            </Button>
          )}
        </Space>
      }
    >
      <Descriptions column={descColumns} size="small" bordered style={{ marginBottom: 16 }}>
        <Descriptions.Item label="Manufacturer">{plugin.manufacture || '—'}</Descriptions.Item>
        <Descriptions.Item label="Version">{plugin.version || '—'}</Descriptions.Item>
        <Descriptions.Item label="Format">
          <Tag color={getFormatColor(plugin.format)}>{plugin.format.toUpperCase()}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="Category">{plugin.category || '—'}</Descriptions.Item>
        <Descriptions.Item label="Plugin ID" span={2}>
          <Text code style={{ fontSize: 11, wordBreak: 'break-all' }}>{plugin.id}</Text>
        </Descriptions.Item>
        <Descriptions.Item label={<Space><FolderOpenOutlined /> File Path</Space>} span={2}>
          <Text code style={{ fontSize: 11, wordBreak: 'break-all' }}>{plugin.path}</Text>
        </Descriptions.Item>
      </Descriptions>

      <div
        style={{
          background: `${token.colorPrimary}12`,
          border: `1px solid ${token.colorPrimary}2e`,
          borderRadius: 8,
          padding: '10px 14px',
        }}
      >
        <Space align="start" size={10}>
          <InfoCircleOutlined style={{ color: token.colorPrimary, marginTop: 2 }} />
          <Text style={{ fontSize: 12, color: token.colorTextSecondary }}>
            This is a {plugin.format.toUpperCase()} plugin. Click "Load Plugin" to add it to your signal chain.
          </Text>
        </Space>
      </div>
    </Modal>
  );
}
