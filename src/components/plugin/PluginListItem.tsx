import { memo } from 'react';
import { Button, Space, Tag, Tooltip, Typography, theme } from 'antd';
import { InfoCircleOutlined, PlusCircleOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../../lib/types';

const { Text } = Typography;

interface PluginListItemProps {
  plugin: PluginInfo;
  isMutating: boolean;
  addLocked: boolean;
  token: ReturnType<typeof theme.useToken>['token'];
  getFormatColor: (format: string) => string;
  onSelect: (plugin: PluginInfo) => void;
  onAdd: (plugin: PluginInfo) => void;
}

const PluginListItem = memo(function PluginListItem({
  plugin,
  isMutating,
  addLocked,
  token,
  getFormatColor,
  onSelect,
  onAdd,
}: PluginListItemProps) {
  return (
    <div
      className="minimal-surface plugin-list-item"
      style={{
        padding: '12px 16px',
        marginBottom: 8,
        background: token.colorBgElevated,
        border: `1px solid ${token.colorBorderSecondary}`,
        borderRadius: 10,
        cursor: 'pointer',
        transition: 'border-color 160ms ease, opacity 160ms ease',
      }}
      onClick={() => onSelect(plugin)}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <Text strong style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{plugin.name}</Text>
            <Text type="secondary" style={{ fontSize: 12, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{plugin.manufacture}</Text>
            <div style={{ marginTop: 6 }}>
              <Space size={4} wrap>
                <Tag color={getFormatColor(plugin.format)}>{plugin.format.toUpperCase()}</Tag>
                <Tag>{plugin.category}</Tag>
                {plugin.version && <Tag color="default">v{plugin.version}</Tag>}
              </Space>
            </div>
          </div>
        </div>
        <div style={{ marginLeft: 12, display: 'flex', gap: 8 }}>
          <Tooltip title="Plugin Info" key="info">
            <Button
              type="text"
              size="small"
              icon={<InfoCircleOutlined />}
              onClick={(e) => {
                e.stopPropagation();
                onSelect(plugin);
              }}
            />
          </Tooltip>
          <Tooltip title="Add to Chain" key="add">
            <Button
              type="primary"
              size="small"
              icon={<PlusCircleOutlined />}
              loading={isMutating}
              disabled={addLocked}
              onClick={(e) => {
                e.stopPropagation();
                onAdd(plugin);
              }}
            />
          </Tooltip>
        </div>
      </div>
    </div>
  );
});

export default PluginListItem;
