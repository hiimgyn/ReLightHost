import { lazy, Suspense, useEffect, useState } from 'react';
import { Drawer, Input, Button, Tabs, Tag, Space, Typography, Tooltip, Empty, Spin } from 'antd';
import { 
  SearchOutlined, 
  ReloadOutlined, 
  SettingOutlined, 
  InfoCircleOutlined,
  PlusCircleOutlined,
  AppstoreOutlined
} from '@ant-design/icons';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginInfo } from '../../lib/types';

const PluginSettings = lazy(() => import('./PluginSettings'));
const PluginInfoModal = lazy(() => import('./PluginInfoModal'));

const { Text } = Typography;

interface PluginLibraryProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginLibrary({ isOpen, onClose }: PluginLibraryProps) {
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
        title={
          <Space>
            <AppstoreOutlined style={{ fontSize: 20, color: '#9b72cf' }} />
            <span>Plugin Library</span>
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
          size="large"
          placeholder="Search plugins..."
          prefix={<SearchOutlined />}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ marginBottom: 16 }}
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
              return (
                <div key={author} style={{ marginBottom: 12 }}>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '8px 12px',
                      borderRadius: 8,
                      background: 'rgba(255,255,255,0.02)',
                      cursor: 'pointer',
                    }}
                    onClick={() => setCollapsedGroups(prev => ({ ...prev, [author]: !prev[author] }))}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                      <AppstoreOutlined style={{ color: '#9b72cf' }} />
                      <Text strong style={{ maxWidth: 240, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{author}</Text>
                      <Tag>{group.length}</Tag>
                    </div>
                    <div style={{ color: '#999', fontSize: 12 }}>{isCollapsed ? 'Collapsed' : 'Expanded'}</div>
                  </div>

                  {!isCollapsed && (
                    <div style={{ marginTop: 8 }}>
                      {group.map((plugin) => (
                        <div
                          key={plugin.id}
                          style={{
                            padding: '12px 16px',
                            marginBottom: 8,
                            background: 'rgba(255, 255, 255, 0.04)',
                            borderRadius: 8,
                            cursor: 'pointer',
                            transition: 'all 0.2s',
                          }}
                          className="plugin-list-item"
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
            image={<AppstoreOutlined style={{ fontSize: 64, color: '#666' }} />}
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
          background: 'rgba(0, 0, 0, 0.45)',
          backdropFilter: 'blur(10px)',
          borderTop: '1px solid rgba(255, 255, 255, 0.1)'
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
