import { memo } from 'react';
import { Button, Space, Tag, Tooltip, Typography, theme } from 'antd';
import { InfoCircleOutlined, PlusCircleOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../../lib/types';

const { Text } = Typography;

interface PluginListItemProps {
  plugin: PluginInfo;
  isAdding?: boolean;
  addLocked: boolean;
  token: ReturnType<typeof theme.useToken>['token'];
  getFormatColor: (format: string) => string;
  onSelect: (plugin: PluginInfo) => void;
  onAdd: (plugin: PluginInfo) => void;
}

const PluginListItem = memo(function PluginListItem({
  plugin,
  isAdding = false,
  addLocked,
  token,
  getFormatColor,
  onSelect,
  onAdd,
}: PluginListItemProps) {
  return (
    <div
      className="plugin-list-item"
      style={{
        padding: '12px 14px',
        marginBottom: 8,
        background: token.colorBgElevated,
        border: `1px solid ${token.colorBorderSecondary}`,
        borderRadius: 10,
        cursor: 'pointer',
        transition: 'all 160ms cubic-bezier(0.4, 0, 0.2, 1)',
      }}
      onClick={() => onSelect(plugin)}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            <Text strong style={{ fontSize: 13.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {plugin.name}
            </Text>
            <Text type="secondary" style={{ fontSize: 11.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {plugin.manufacture || 'Unknown'}
            </Text>
            <div style={{ marginTop: 6 }}>
              <Space size={4} wrap>
                <Tag color={getFormatColor(plugin.format)} style={{ margin: 0 }}>
                  {plugin.format.toUpperCase()}
                </Tag>
                {plugin.category && (
                  <Tag style={{ margin: 0 }}>{plugin.category}</Tag>
                )}
                {plugin.version && (
                  <Tag color="default" style={{ margin: 0 }}>v{plugin.version}</Tag>
                )}
              </Space>
            </div>
          </div>
        </div>
        <div style={{ marginLeft: 8, display: 'flex', alignItems: 'center', gap: 6, flexShrink: 0 }}>
          <Tooltip title="Plugin Info" key="info">
            <Button
              type="text"
              size="small"
              icon={<InfoCircleOutlined style={{ color: token.colorTextSecondary, fontSize: 14 }} />}
              onClick={(e) => {
                e.stopPropagation();
                onSelect(plugin);
              }}
              style={{ borderRadius: 8, width: 28, height: 28 }}
            />
          </Tooltip>
          <Tooltip title="Add to Chain" key="add">
            <Button
              type="primary"
              size="small"
              icon={<PlusCircleOutlined style={{ fontSize: 13 }} />}
              loading={isAdding}
              disabled={addLocked}
              onClick={(e) => {
                e.stopPropagation();
                onAdd(plugin);
              }}
              style={{ borderRadius: 8, height: 28, paddingInline: 10 }}
            />
          </Tooltip>
        </div>
      </div>
    </div>
  );
});

export default PluginListItem;
