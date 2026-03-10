import { Card, Button, Space, Tag, Tooltip, theme } from 'antd';
import { 
  CloseOutlined, 
  PoweroffOutlined, 
  PlayCircleOutlined,
  ThunderboltOutlined 
} from '@ant-design/icons';
import type { PluginInstanceInfo } from '../lib/types';

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  onRemove: () => void;
  onToggleBypass: () => void;
  onLaunch?: () => void;
}

export default function PluginCard({ plugin, onRemove, onToggleBypass, onLaunch }: PluginCardProps) {
  const { token } = theme.useToken();
  return (
    <Card
      size="small"
      className={`
        transition-all shadow-lg
        ${plugin.bypassed ? 'opacity-60' : ''}
      `}
      style={{
        borderWidth: 2,
        borderColor: plugin.bypassed ? token.colorBorderSecondary : token.colorPrimary,
        background: token.colorBgContainer,
      }}
      bodyStyle={{ padding: '16px' }}
    >
      {/* Header */}
      <Space direction="vertical" size="small" style={{ width: '100%' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <h3 style={{ 
              fontWeight: 600, 
              color: token.colorText, 
              marginBottom: 4,
              fontSize: 15,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis'
            }}>
              {plugin.name}
            </h3>
            <code style={{ 
              fontSize: 10, 
              color: token.colorTextSecondary,
              display: 'block'
            }}>
              {plugin.instance_id.slice(0, 12)}...
            </code>
          </div>
        </div>

        {/* Parameters Info */}
        {plugin.parameters.length > 0 && (
          <Tag icon={<ThunderboltOutlined />} color="blue" style={{ fontSize: 11 }}>
            {plugin.parameters.length} parameter{plugin.parameters.length !== 1 ? 's' : ''}
          </Tag>
        )}

        {/* Action Buttons */}
        <Space style={{ width: '100%' }} size="small">
          <Tooltip title={plugin.bypassed ? 'Enable Plugin' : 'Bypass Plugin'}>
            <Button
              type={plugin.bypassed ? 'default' : 'primary'}
              danger={!plugin.bypassed}
              size="small"
              icon={<PoweroffOutlined />}
              onClick={onToggleBypass}
              style={{ flex: 1 }}
            >
              {plugin.bypassed ? 'Bypassed' : 'Active'}
            </Button>
          </Tooltip>

          <Tooltip title="Launch Plugin">
            <Button
              type="default"
              size="small"
              icon={<PlayCircleOutlined />}
              onClick={onLaunch}
            >
              Launch
            </Button>
          </Tooltip>

          <Tooltip title="Remove from Chain">
            <Button
              type="primary"
              danger
              size="small"
              icon={<CloseOutlined />}
              onClick={onRemove}
            />
          </Tooltip>
        </Space>
      </Space>
    </Card>
  );
}
