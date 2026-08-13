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
    try {
      await addToChain(plugin);
    } catch (error) {
      console.error('Failed to add plugin:', error);
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
            border: '1px solid var(--rh-surface-soft-border-strong)',
            background: 'var(--rh-surface-soft-gradient)',
          }}
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
                isMutating={isMutating}
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
          borderTop: '1px solid var(--rh-minimal-border)'
        }}>
          {isChainInitializing && (
            <div style={{ textAlign: 'center', marginBottom: 8, color: 'var(--rh-text-muted)', fontSize: 12 }}>
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
          <div style={{ textAlign: 'center', marginTop: 8, color: 'var(--rh-text-muted)', fontSize: 12 }}>
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
