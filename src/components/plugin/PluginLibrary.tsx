import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import { Drawer, Input, Button, Tabs, Space, Typography, Tooltip, Empty, Spin } from 'antd';
import {
  SearchOutlined,
  ReloadOutlined,
  SettingOutlined,
  AppstoreOutlined,
} from '@ant-design/icons';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginInfo } from '../../lib/types';
import { theme } from 'antd';
import PluginAuthorGroup from './PluginAuthorGroup';
import { usePluginLibraryFilters } from './usePluginLibraryFilters';

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
  const [addingPluginId, setAddingPluginId] = useState<string | null>(null);

  const { filteredPlugins, groupedByAuthor, authorKeys, tabItems } = usePluginLibraryFilters({
    availablePlugins,
    searchQuery,
    filterFormat,
  });

  const handleSelectPlugin = useCallback((plugin: PluginInfo) => {
    setSelectedPlugin(plugin);
  }, []);

  const handleAddPlugin = useCallback(async (plugin: PluginInfo) => {
    if (addLocked) return;
    setAddingPluginId(plugin.id);
    try {
      await addToChain(plugin);
    } catch (error) {
      console.error('Failed to add plugin:', error);
    } finally {
      setAddingPluginId(null);
    }
  }, [addLocked, addToChain]);

  useEffect(() => {
    if (isOpen && availablePlugins.length === 0) {
      scanPlugins();
    }
  }, [isOpen, availablePlugins.length, scanPlugins]);

  return (
    <>
      <Drawer
        title={
          <Space size={10} align="center">
            <div
              style={{
                width: 32,
                height: 32,
                borderRadius: 8,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                background: `${token.colorPrimary}1c`,
                border: `1px solid ${token.colorPrimary}38`,
              }}
            >
              <AppstoreOutlined style={{ fontSize: 16, color: token.colorPrimary }} />
            </div>
            <div>
              <Text strong style={{ fontSize: 15, display: 'block', lineHeight: 1.2, color: token.colorText }}>
                Plugin Library
              </Text>
              <Text type="secondary" style={{ fontSize: 11 }}>
                {availablePlugins.length} plugins available
              </Text>
            </div>
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
                icon={<SettingOutlined style={{ color: token.colorTextSecondary, fontSize: 16 }} />}
                onClick={() => setShowSettings(true)}
              />
            </Tooltip>
          </Space>
        }
      >
        {/* Search Bar */}
        <Input
          className="plugin-search-input"
          size="large"
          placeholder="Search plugins by name, manufacturer, category..."
          prefix={<SearchOutlined style={{ color: token.colorPrimary, fontSize: 16, marginRight: 4 }} />}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          style={{ marginBottom: 16 }}
          allowClear
        />

        {/* Filter Tabs */}
        <Tabs
          className="plugin-library-tabs"
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
            <div style={{ marginTop: 16, color: 'var(--rh-text-muted)' }}>
              Scanning for plugins...
            </div>
          </div>
        ) : filteredPlugins.length > 0 ? (
          <div style={{ marginTop: 16 }}>
            {authorKeys.map((author) => (
              <PluginAuthorGroup
                key={author}
                author={author}
                group={groupedByAuthor[author]}
                isCollapsed={!!collapsedGroups[author]}
                onToggleCollapse={() => setCollapsedGroups(prev => ({ ...prev, [author]: !prev[author] }))}
                addingPluginId={addingPluginId}
                addLocked={addLocked}
                onSelect={handleSelectPlugin}
                onAdd={handleAddPlugin}
              />
            ))}
          </div>
        ) : (
          <Empty
              image={<AppstoreOutlined style={{ fontSize: 64, color: token.colorTextTertiary }} />}
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
          padding: '14px 20px',
          background: token.colorBgElevated,
          borderTop: `1px solid ${token.colorBorderSecondary}`,
          boxShadow: '0 -8px 24px rgba(0, 0, 0, 0.25)',
        }}>
          {isChainInitializing && (
            <div style={{ textAlign: 'center', marginBottom: 8, color: token.colorWarning, fontSize: 12 }}>
              Initial chain is loading, adding plugins is temporarily locked.
            </div>
          )}
          <Button
            type="primary"
            block
            size="large"
            icon={<ReloadOutlined spin={isScanning} />}
            onClick={scanPlugins}
            loading={isScanning}
            disabled={isMutating}
            style={{ borderRadius: 10 }}
          >
            {isScanning ? 'Scanning...' : 'Scan for Plugins'}
          </Button>
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
