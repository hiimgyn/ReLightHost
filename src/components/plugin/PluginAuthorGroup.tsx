import { Space, Tag, Typography, theme } from 'antd';
import { Layers, ChevronDown, ChevronRight } from 'lucide-react';
import type { PluginInfo } from '../../lib/types';
import PluginListItem from './PluginListItem';
import { getAuthorAccent, getAuthorLabel, getFormatColor } from './pluginLibraryHelpers';
import { useTranslation } from '../../i18n';

const { Text } = Typography;

interface PluginAuthorGroupProps {
  author: string;
  group: PluginInfo[];
  isCollapsed: boolean;
  onToggleCollapse: () => void;
  addingPluginId?: string | null;
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
  addingPluginId,
  addLocked,
  onSelect,
  onAdd,
}: PluginAuthorGroupProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const authorColor = getAuthorAccent(token, author);

  return (
    <div style={{ marginBottom: 12 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
          padding: '10px 14px',
          borderRadius: 10,
          background: token.colorBgContainer,
          border: `1px solid ${token.colorBorderSecondary}`,
          cursor: 'pointer',
          transition: 'all 160ms ease',
        }}
        onClick={onToggleCollapse}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, minWidth: 0, flex: 1 }}>
          <div
            style={{
              width: 32,
              height: 32,
              borderRadius: 8,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${authorColor}22`,
              border: `1px solid ${authorColor}44`,
              flexShrink: 0,
            }}
          >
            <Layers size={16} style={{ color: authorColor }} />
          </div>
          <div style={{ minWidth: 0, flex: 1, display: 'flex', flexDirection: 'column', gap: 2 }}>
            <Space size={6} align="center" style={{ minWidth: 0 }}>
              <Text strong style={{ maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{getAuthorLabel(author)}</Text>
              <Tag style={{ margin: 0, fontSize: 11 }}>{group.length}</Tag>
            </Space>
            <Text type="secondary" style={{ fontSize: 11, lineHeight: 1.2 }}>
              {author === 'Unknown' ? t('library.unknownMfg') : t('library.groupedByMfg')}
            </Text>
          </div>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexShrink: 0 }}>
          <Text style={{ color: token.colorTextTertiary, fontSize: 12 }}>
            {isCollapsed ? t('library.collapsed') : t('library.expanded')}
          </Text>
          {isCollapsed ? (
            <ChevronRight size={14} style={{ color: token.colorTextQuaternary }} />
          ) : (
            <ChevronDown size={14} style={{ color: token.colorTextQuaternary }} />
          )}
        </div>
      </div>

      {!isCollapsed && (
        <div style={{ marginTop: 8, paddingLeft: 10, borderLeft: `1px solid ${token.colorBorderSecondary}` }}>
          {group.map((plugin) => (
            <PluginListItem
              key={plugin.id}
              plugin={plugin}
              isAdding={addingPluginId === plugin.id}
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
