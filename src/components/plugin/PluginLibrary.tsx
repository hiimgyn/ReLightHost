import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import { Drawer, Input, Button, Tabs, Space, Typography, Tooltip, Empty, Spin } from 'antd';
import {
  Search,
  RotateCw,
  FolderCog,
  Boxes,
} from 'lucide-react';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginInfo } from '../../lib/types';
import { theme } from 'antd';
import PluginAuthorGroup from './PluginAuthorGroup';
import { usePluginLibraryFilters } from './usePluginLibraryFilters';
import { useTranslation } from '../../i18n';

const PluginSettings = lazy(() => import('./PluginSettings'));
const PluginInfoModal = lazy(() => import('./PluginInfoModal'));

const { Text } = Typography;

interface PluginLibraryProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginLibrary({ isOpen, onClose }: PluginLibraryProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
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
              <Boxes size={18} style={{ color: token.colorPrimary }} />
            </div>
            <div>
              <Text strong style={{ fontSize: 15, display: 'block', lineHeight: 1.2, color: token.colorText }}>
                {t('library.title')}
              </Text>
              <Text type="secondary" style={{ fontSize: 11 }}>
                {t('library.availableCount', { count: availablePlugins.length })}
              </Text>
            </div>
          </Space>
        }
        placement="right"
        width={540}
        onClose={onClose}
        open={isOpen}
        extra={
          <Space>
            <Tooltip title={t('library.scanSettingsTooltip')}>
              <Button
                type="text"
                icon={<FolderCog size={17} style={{ color: token.colorTextSecondary }} />}
                onClick={() => setShowSettings(true)}
              />
            </Tooltip>
          </Space>
        }
        styles={{
          body: {
            padding: 0,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
            height: '100%',
          },
          footer: {
            padding: '12px 20px',
            background: token.colorBgElevated,
            borderTop: `1px solid ${token.colorBorderSecondary}`,
          },
        }}
        footer={
          <div>
            {isChainInitializing && (
              <div style={{ textAlign: 'center', marginBottom: 8, color: token.colorWarning, fontSize: 12 }}>
                {t('library.initialChainLoading')}
              </div>
            )}
            <Button
              type="primary"
              block
              size="large"
              icon={<RotateCw size={16} className={isScanning ? "animate-spin" : ""} />}
              onClick={scanPlugins}
              loading={isScanning}
              disabled={isMutating}
              style={{ borderRadius: 10 }}
            >
              {isScanning ? t('library.scanningButton') : t('library.scanForPlugins')}
            </Button>
          </div>
        }
      >
        {/* Sticky Header: Search Bar & Format Filter Tabs */}
        <div
          style={{
            padding: '16px 20px 0',
            background: token.colorBgElevated,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            flexShrink: 0,
            zIndex: 10,
          }}
        >
          <Input
            className="plugin-search-input"
            size="large"
            placeholder={t('library.searchPlaceholder')}
            prefix={<Search size={16} style={{ color: token.colorPrimary, marginRight: 6 }} />}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            style={{ marginBottom: 12 }}
            allowClear
          />

          <Tabs
            className="plugin-library-tabs"
            activeKey={filterFormat}
            onChange={(key) => setFilterFormat(key as any)}
            items={tabItems}
            style={{ marginBottom: 0 }}
          />
        </div>

        {/* Scrollable Plugin List */}
        <div
          style={{
            flex: 1,
            minHeight: 0,
            overflowY: 'auto',
            padding: '16px 20px',
          }}
        >
          {isScanning ? (
            <div style={{ textAlign: 'center', padding: '60px 0' }}>
              <Spin size="large" />
              <div style={{ marginTop: 16, color: 'var(--rh-text-muted)' }}>
                {t('library.scanning')}
              </div>
            </div>
          ) : filteredPlugins.length > 0 ? (
            <div>
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
              image={<Boxes size={56} strokeWidth={1.5} style={{ color: token.colorTextTertiary, margin: '0 auto 12px' }} />}
              description={
                <Space direction="vertical" size={0}>
                  <Text type="secondary">{t('library.noPluginsFound')}</Text>
                  <Text type="secondary" style={{ fontSize: 12 }}>
                    {searchQuery ? t('library.tryDifferentSearch') : t('library.clickScanPlugins')}
                  </Text>
                </Space>
              }
            />
          )}
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
