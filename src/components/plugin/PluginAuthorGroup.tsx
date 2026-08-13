import { Space, Tag, Typography, theme } from 'antd';
import { AppstoreOutlined, DownOutlined, RightOutlined } from '@ant-design/icons';
import type { PluginInfo } from '../../lib/types';
import PluginListItem from './PluginListItem';
import { getAuthorAccent, getAuthorLabel, getFormatColor } from './pluginLibraryHelpers';

const { Text } = Typography;

interface PluginAuthorGroupProps {
  author: string;
  group: PluginInfo[];
  isCollapsed: boolean;
  onToggleCollapse: () => void;
  isMutating: boolean;
  addLocked: boolean;
  onSelect: (plugin: PluginInfo) => void;
  onAdd: (plugin: PluginInfo) => void;
}

/** One collapsible manufacturer group in the plugin library list. */
export default function PluginAuthorGroup({
  author,
  group,
  isCollapsed,
  onToggleCollapse,
  isMutating,
  addLocked,
  onSelect,
  onAdd,
}: PluginAuthorGroupProps) {
  const { token } = theme.useToken();
  const authorColor = getAuthorAccent(token, author);

  return (
    <div style={{ marginBottom: 12 }}>
      <div
        className="minimal-surface"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
          padding: '10px 12px',
          borderRadius: 10,
          background: token.colorBgElevated,
          border: `1px solid ${token.colorBorderSecondary}`,
          boxShadow: 'none',
          cursor: 'pointer',
          transition: 'border-color 160ms ease',
        }}
        onClick={onToggleCollapse}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0, flex: 1 }}>
          <div
            style={{
              width: 28,
              height: 28,
              borderRadius: 8,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${authorColor}18`,
              border: `1px solid ${authorColor}33`,
              flexShrink: 0,
            }}
          >
            <AppstoreOutlined style={{ color: authorColor, fontSize: 14 }} />
          </div>
          <div style={{ minWidth: 0, flex: 1, display: 'flex', flexDirection: 'column', gap: 2 }}>
            <Space size={6} align="center" style={{ minWidth: 0 }}>
              <Text strong style={{ maxWidth: 240, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{getAuthorLabel(author)}</Text>
              <Tag style={{ margin: 0 }}>{group.length}</Tag>
            </Space>
            <Text type="secondary" style={{ fontSize: 11, lineHeight: 1.2 }}>
              {author === 'Unknown' ? 'Plugins without manufacturer metadata' : 'Grouped by manufacturer'}
            </Text>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
          <Text style={{ color: token.colorTextTertiary, fontSize: 12 }}>
            {isCollapsed ? 'Collapsed' : 'Expanded'}
          </Text>
          {isCollapsed ? (
            <RightOutlined style={{ color: token.colorTextQuaternary, fontSize: 11 }} />
          ) : (
            <DownOutlined style={{ color: token.colorTextQuaternary, fontSize: 11 }} />
          )}
        </div>
      </div>

      {!isCollapsed && (
        <div style={{ marginTop: 8, paddingLeft: 10, borderLeft: `1px solid ${token.colorBorderSecondary}` }}>
          {group.map((plugin) => (
            <PluginListItem
              key={plugin.id}
              plugin={plugin}
              isMutating={isMutating}
              addLocked={addLocked}
              token={token}
              getFormatColor={getFormatColor}
              onSelect={onSelect}
              onAdd={onAdd}
            />
          ))}
        </div>
      )}
    </div>
  );
}
