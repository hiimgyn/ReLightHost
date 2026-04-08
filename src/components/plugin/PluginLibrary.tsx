import { lazy, Suspense, useEffect, useState } from 'react';
import { Drawer, Input, Button, Tabs, Tag, Space, Typography, Tooltip, Empty, Spin } from 'antd';
import { 
  SearchOutlined, 
  ReloadOutlined, 
  SettingOutlined, 
  InfoCircleOutlined,
  PlusCircleOutlined,
  AppstoreOutlined,
  DownOutlined,
  RightOutlined,
} from '@ant-design/icons';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginInfo } from '../../lib/types';
import { theme } from 'antd';

const PluginSettings = lazy(() => import('./PluginSettings'));
const PluginInfoModal = lazy(() => import('./PluginInfoModal'));

const { Text } = Typography;

interface PluginLibraryProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginLibrary({ isOpen, onClose }: PluginLibraryProps) {
  const { token } = theme.useToken();
  const {
    availablePlugins,
    isScanning,
    isMutating,
    isChainInitializing,
    scanPlugins,
    addToChain,
  } = usePluginStore();
  const addLocked = isMutating || isChainInitializing;
  const [searchQuery, setSearchQuery] = useState('');
  const [filterFormat, setFilterFormat] = useState<'all' | 'vst3' | 'vst' | 'clap' | 'builtin'>('all');
  const [showSettings, setShowSettings] = useState(false);
  const [selectedPlugin, setSelectedPlugin] = useState<PluginInfo | null>(null);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (isOpen && availablePlugins.length === 0) {
      scanPlugins();
    }
  }, [isOpen, availablePlugins.length, scanPlugins]);

  const filteredPlugins = availablePlugins.filter(plugin => {
    const matchesSearch = plugin.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
                         plugin.manufacture.toLowerCase().includes(searchQuery.toLowerCase());
    const matchesFormat = filterFormat === 'all' || plugin.format === filterFormat;
    return matchesSearch && matchesFormat;
  });

  // Group plugins by manufacture/author
  const groupedByAuthor = filteredPlugins.reduce((acc: Record<string, PluginInfo[]>, plugin) => {
    const author = plugin.manufacture?.trim() || 'Unknown';
    if (!acc[author]) acc[author] = [];
    acc[author].push(plugin);
    return acc;
  }, {} as Record<string, PluginInfo[]>);
  const authorKeys = Object.keys(groupedByAuthor).sort((a, b) => a.localeCompare(b));

  const handleAddPlugin = async (plugin: PluginInfo) => {
    if (addLocked) return;
    try {
      await addToChain(plugin);
      // Optional: close drawer after adding plugin
      // onClose();
    } catch (error) {
      console.error('Failed to add plugin:', error);
    }
  };

  const getFormatColor = (format: string) => {
    switch (format) {
      case 'vst3': return 'purple';
      case 'vst': return 'blue';
      case 'clap': return 'green';
      case 'builtin': return 'cyan';
      default: return 'default';
    }
  };

  const getAuthorLabel = (author: string) => {
    return author === 'Unknown' ? 'Unknown folder' : author;
  };

  const getAuthorAccent = (author: string) => {
    if (author === 'Unknown') return token.colorTextQuaternary;
    const first = author.trim().charAt(0).toUpperCase();
    const code = first ? first.charCodeAt(0) : 0;
    const palette = [token.colorPrimary, token.colorInfo, token.colorSuccess, token.colorWarning];
    return palette[code % palette.length];
  };

  const tabItems = [
    {
      key: 'all',
      label: `All (${availablePlugins.length})`,
      children: null,
    },
    {
      key: 'builtin',
      label: `Built-in (${availablePlugins.filter(p => p.format === 'builtin').length})`,
      children: null,
    },
    {
      key: 'vst3',
      label: `VST3 (${availablePlugins.filter(p => p.format === 'vst3').length})`,
      children: null,
    },
    {
      key: 'vst',
      label: `VST2 (${availablePlugins.filter(p => p.format === 'vst').length})`,
      children: null,
    },
    {
      key: 'clap',
      label: `CLAP (${availablePlugins.filter(p => p.format === 'clap').length})`,
      children: null,
    },
  ];

  return (
    <>
      <Drawer
        className="minimal-panel"
        title={
          <Space>
            <AppstoreOutlined style={{ fontSize: 20, color: token.colorPrimary }} />
            <Text strong style={{ fontSize: 15, letterSpacing: '-0.01em', color: token.colorText }}>Plugin Library</Text>
          </Space>
        }
        placement="right"
        size={480}
        onClose={onClose}
        open={isOpen}
        extra={
          <Space>
            <Tooltip title="Plugin Scan Settings">
              <Button 
                type="text" 
                icon={<SettingOutlined />} 
                onClick={() => setShowSettings(true)}
              />
            </Tooltip>
          </Space>
        }
      >
        {/* Search Bar */}
        <Input
          className="minimal-surface"
          size="large"
          placeholder="Search plugins..."
          prefix={<SearchOutlined />}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{
            marginBottom: 16,
            borderRadius: 10,
            border: '1px solid rgba(255,255,255,0.32)',
            background: 'linear-gradient(135deg, rgba(255,255,255,0.2) 0%, rgba(255,255,255,0.14) 100%)',
          }}
          allowClear
        />

        {/* Filter Tabs */}
        <Tabs
          activeKey={filterFormat}
          onChange={(key) => setFilterFormat(key as any)}
          items={tabItems}
          style={{ marginBottom: 16 }}
        />

        {/* Plugin List — paddingBottom prevents content hiding under absolute footer */}
        <div style={{ paddingBottom: 90 }}>
        {isScanning ? (
          <div style={{ textAlign: 'center', padding: '60px 0' }}>
            <Spin size="large" />
            <div style={{ marginTop: 16, color: '#999' }}>
              Scanning for plugins...
            </div>
          </div>
        ) : filteredPlugins.length > 0 ? (
          <div style={{ marginTop: 16 }}>
            {authorKeys.map((author) => {
              const group = groupedByAuthor[author];
              const isCollapsed = !!collapsedGroups[author];
              const authorColor = getAuthorAccent(author);
              return (
                <div key={author} style={{ marginBottom: 12 }}>
                  <div
                    className="minimal-surface"
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      gap: 12,
                      padding: '10px 12px',
                      borderRadius: 10,
                      background: `linear-gradient(135deg, ${token.colorBgElevated} 0%, ${token.colorBgContainer} 100%)`,
                      border: `1px solid ${token.colorBorderSecondary}`,
                      boxShadow: 'none',
                      cursor: 'pointer',
                      transition: 'transform 160ms ease, border-color 160ms ease, background 160ms ease, box-shadow 160ms ease',
                    }}
                    onClick={() => setCollapsedGroups(prev => ({ ...prev, [author]: !prev[author] }))}
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
                          background: `linear-gradient(135deg, ${authorColor}22 0%, ${authorColor}12 100%)`,
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
                        <div
                          key={plugin.id}
                          className="minimal-surface plugin-list-item"
                          style={{
                            padding: '12px 16px',
                            marginBottom: 8,
                            background: `linear-gradient(135deg, ${token.colorBgElevated} 0%, ${token.colorBgContainer} 100%)`,
                            border: `1px solid ${token.colorBorderSecondary}`,
                            borderRadius: 10,
                            cursor: 'pointer',
                            transition: 'transform 180ms ease, border-color 180ms ease, box-shadow 180ms ease',
                          }}
                          onClick={() => setSelectedPlugin(plugin)}
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
                                    setSelectedPlugin(plugin);
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
                                    handleAddPlugin(plugin);
                                  }}
                                />
                              </Tooltip>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <Empty
              image={<AppstoreOutlined style={{ fontSize: 64, color: token.colorTextTertiary }} />}
            description={
              <Space orientation="vertical" size={0}>
                <Text type="secondary">No plugins found</Text>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  {searchQuery ? 'Try a different search' : 'Click "Scan Plugins" to find plugins'}
                </Text>
              </Space>
            }
          />
        )}
        </div>

        {/* Footer Actions */}
        <div style={{ 
          position: 'absolute', 
          bottom: 0, 
          left: 0, 
          right: 0, 
          padding: '16px 24px', 
          background: 'linear-gradient(135deg, var(--rh-minimal-bg-strong) 0%, var(--rh-minimal-bg) 100%)',
          backdropFilter: 'blur(10px)',
          borderTop: '1px solid var(--rh-minimal-border)'
        }}>
          {isChainInitializing && (
            <div style={{ textAlign: 'center', marginBottom: 8, color: '#999', fontSize: 12 }}>
              Initial chain is loading, adding plugins is temporarily locked.
            </div>
          )}
          <Button
            block
            size="large"
            icon={<ReloadOutlined />}
            onClick={scanPlugins}
            loading={isScanning}
            disabled={isMutating}
          >
            {isScanning ? 'Scanning...' : 'Scan for Plugins'}
          </Button>
          <div style={{ textAlign: 'center', marginTop: 8, color: '#999', fontSize: 12 }}>
            {availablePlugins.length} plugin{availablePlugins.length !== 1 ? 's' : ''} available
          </div>
        </div>
      </Drawer>

      {/* Plugin Settings Modal */}
      {showSettings && (
        <Suspense fallback={null}>
          <PluginSettings isOpen={showSettings} onClose={() => setShowSettings(false)} />
        </Suspense>
      )}

      {/* Plugin Info Modal */}
      {selectedPlugin && (
        <Suspense fallback={null}>
          <PluginInfoModal
            plugin={selectedPlugin}
            isOpen={true}
            onClose={() => setSelectedPlugin(null)}
            onLoad={() => {
              if (addLocked) return;
              handleAddPlugin(selectedPlugin);
            }}
          />
        </Suspense>
      )}
    </>
  );
}
