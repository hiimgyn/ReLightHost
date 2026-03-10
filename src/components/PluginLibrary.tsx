import { useEffect, useState } from 'react';
import { Drawer, Input, Button, Tabs, List, Tag, Space, Typography, Tooltip, Empty, Spin } from 'antd';
import { 
  SearchOutlined, 
  ReloadOutlined, 
  SettingOutlined, 
  InfoCircleOutlined,
  PlusCircleOutlined,
  AppstoreOutlined
} from '@ant-design/icons';
import { usePluginStore } from '../stores/pluginStore';
import type { PluginInfo } from '../lib/types';
import PluginSettings from './PluginSettings';
import PluginInfoModal from './PluginInfoModal';

const { Text } = Typography;

interface PluginLibraryProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginLibrary({ isOpen, onClose }: PluginLibraryProps) {
  const { availablePlugins, isScanning, scanPlugins, addToChain } = usePluginStore();
  const [searchQuery, setSearchQuery] = useState('');
  const [filterFormat, setFilterFormat] = useState<'all' | 'vst3' | 'vst' | 'clap'>('all');
  const [showSettings, setShowSettings] = useState(false);
  const [selectedPlugin, setSelectedPlugin] = useState<PluginInfo | null>(null);

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

  const handleAddPlugin = async (plugin: PluginInfo) => {
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
            <AppstoreOutlined style={{ fontSize: 20, color: '#1677ff' }} />
            <span>Plugin Library</span>
          </Space>
        }
        placement="right"
        width={480}
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
          <List
            dataSource={filteredPlugins}
            renderItem={(plugin) => (
              <List.Item
                style={{ 
                  padding: '12px 16px',
                  marginBottom: 8,
                  background: 'rgba(255, 255, 255, 0.04)',
                  borderRadius: 8,
                  cursor: 'pointer',
                  transition: 'all 0.2s',
                }}
                className="plugin-list-item"
                actions={[
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
                  </Tooltip>,
                  <Tooltip title="Add to Chain" key="add">
                    <Button
                      type="primary"
                      size="small"
                      icon={<PlusCircleOutlined />}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleAddPlugin(plugin);
                      }}
                    />
                  </Tooltip>,
                ]}
              >
                <List.Item.Meta
                  title={
                    <Space direction="vertical" size={2}>
                      <Text strong>{plugin.name}</Text>
                      <Text type="secondary" style={{ fontSize: 12 }}>
                        {plugin.manufacture}
                      </Text>
                    </Space>
                  }
                  description={
                    <Space size={4} wrap>
                      <Tag color={getFormatColor(plugin.format)}>
                        {plugin.format.toUpperCase()}
                      </Tag>
                      <Tag>{plugin.category}</Tag>
                      {plugin.version && (
                        <Tag color="default">v{plugin.version}</Tag>
                      )}
                    </Space>
                  }
                />
              </List.Item>
            )}
          />
        ) : (
          <Empty
            image={<AppstoreOutlined style={{ fontSize: 64, color: '#666' }} />}
            description={
              <Space direction="vertical" size={0}>
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
          <Button
            block
            size="large"
            icon={<ReloadOutlined />}
            onClick={scanPlugins}
            loading={isScanning}
          >
            {isScanning ? 'Scanning...' : 'Scan for Plugins'}
          </Button>
          <div style={{ textAlign: 'center', marginTop: 8, color: '#999', fontSize: 12 }}>
            {availablePlugins.length} plugin{availablePlugins.length !== 1 ? 's' : ''} available
          </div>
        </div>
      </Drawer>

      {/* Plugin Settings Modal */}
      <PluginSettings isOpen={showSettings} onClose={() => setShowSettings(false)} />

      {/* Plugin Info Modal */}
      {selectedPlugin && (
        <PluginInfoModal
          plugin={selectedPlugin}
          isOpen={true}
          onClose={() => setSelectedPlugin(null)}
          onLoad={() => handleAddPlugin(selectedPlugin)}
        />
      )}
    </>
  );
}
