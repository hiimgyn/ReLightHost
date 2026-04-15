import { Modal, Button, Tag, Typography, Space, Descriptions, theme, Grid } from 'antd';
import { FolderOpenOutlined, InfoCircleOutlined, AppstoreOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../../lib/types';

const { Text } = Typography;

interface PluginInfoModalProps {
  plugin: PluginInfo;
  isOpen: boolean;
  onClose: () => void;
  onLoad?: () => void;
}

function getFormatColor(format: string) {
  if (format === 'vst3') return 'purple';
  if (format === 'vst') return 'blue';
  return 'green';
}

export default function PluginInfoModal({ plugin, isOpen, onClose, onLoad }: PluginInfoModalProps) {
  const { token } = theme.useToken();

  const modalWidth = typeof window === 'undefined' ? 448 : 'clamp(300px, 58vw, 448px)';
  const screens = Grid.useBreakpoint();
  const descColumns = screens.md ? 2 : 1;

  return (
    <Modal
      className="minimal-panel"
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 12, maxWidth: 448 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 220px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '12px 14px 14px',
        },
      }}
      title={
        <Space>
          <AppstoreOutlined style={{ color: token.colorPrimary }} />
          <Text strong style={{ fontSize: 15, letterSpacing: '-0.01em' }}>{plugin.name}</Text>
          <Tag color={getFormatColor(plugin.format)}>{plugin.format.toUpperCase()}</Tag>
        </Space>
      }
      footer={
        <Space>
          <Button onClick={onClose}>Close</Button>
          {onLoad && (
            <Button type="primary" onClick={() => { onLoad(); onClose(); }}>
              Load Plugin
            </Button>
          )}
        </Space>
      }
    >

      <Descriptions column={descColumns} size="small" bordered style={{ marginBottom: 16 }}>
        <Descriptions.Item label="Manufacture">{plugin.manufacture || '—'}</Descriptions.Item>
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

      <div className="minimal-surface" style={{
        background: token.colorInfoBg,
        border: `1px solid ${token.colorInfoBorder}`,
        borderRadius: token.borderRadius,
        padding: '10px 14px',
      }}>
        <Space>
          <InfoCircleOutlined style={{ color: token.colorInfo }} />
          <Text style={{ fontSize: 12, color: token.colorInfoText }}>
            This is a {plugin.format.toUpperCase()} plugin. Click "Load Plugin" to add it to your signal chain.
          </Text>
        </Space>
      </div>
    </Modal>
  );
}
