import { Modal, Button, Tag, Typography, Space, Descriptions, theme } from 'antd';
import { FolderOpenOutlined, InfoCircleOutlined, AppstoreOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../lib/types';

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

  return (
    <Modal
      open={isOpen}
      onCancel={onClose}
      width={600}
      title={
        <Space>
          <AppstoreOutlined style={{ color: token.colorPrimary }} />
          <Text strong>{plugin.name}</Text>
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

      <Descriptions column={2} size="small" bordered style={{ marginBottom: 16 }}>
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

      <div style={{
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
