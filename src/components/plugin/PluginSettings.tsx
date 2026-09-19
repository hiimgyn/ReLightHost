import { useState, useEffect } from 'react';
import { Modal, Button, Space, Typography, Tag, message, theme, Tooltip } from 'antd';
import {
  FolderOpen,
  Trash2,
  Plus,
  FolderCog,
  RefreshCw,
  Copy,
  Info,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { usePluginStore } from '../../stores/pluginStore';
import { useTranslation } from '../../i18n';

const { Text } = Typography;

interface PluginSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginSettings({ isOpen, onClose }: PluginSettingsProps) {
  const { t } = useTranslation();
  const [customPaths, setCustomPaths] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const { scanPlugins, isScanning } = usePluginStore();
  const [messageApi, contextHolder] = message.useMessage();
  const { token } = theme.useToken();
  const modalWidth = typeof window === 'undefined' ? 640 : 'clamp(480px, 60vw, 660px)';

  useEffect(() => {
    if (isOpen) loadPaths();
  }, [isOpen]);

  const loadPaths = async () => {
    try {
      const paths = await invoke<string[]>('get_custom_scan_paths');
      setCustomPaths(paths);
    } catch (err) {
      console.error('Failed to load custom paths:', err);
    }
  };

  const addPath = async () => {
    try {
      const selected = await open({ directory: true, multiple: false, title: t('scanSettings.selectDirectory') });
      if (selected && typeof selected === 'string') {
        await invoke('add_custom_scan_path', { path: selected });
        await loadPaths();
        messageApi.success(t('scanSettings.pathAdded'));
      }
    } catch (err) {
      messageApi.error(t('scanSettings.pathAddFailed', { error: String(err) }));
    }
  };

  const removePath = async (path: string) => {
    try {
      await invoke('remove_custom_scan_path', { path });
      await loadPaths();
      messageApi.success(t('scanSettings.pathRemoved'));
    } catch (err) {
      messageApi.error(t('scanSettings.pathRemoveFailed', { error: String(err) }));
    }
  };

  const rescanPlugins = async () => {
    setLoading(true);
    try {
      await scanPlugins();
      messageApi.success(t('scanSettings.scanCompleted'));
      onClose();
    } catch (err) {
      messageApi.error(t('scanSettings.scanFailed', { error: String(err) }));
    } finally {
      setLoading(false);
    }
  };

  const ua = typeof window !== 'undefined' ? window.navigator.userAgent : '';
  const isWindows = ua.includes('Windows');
  const isMac = ua.includes('Macintosh') || ua.includes('Mac OS');

  const DEFAULT_PATHS = isWindows
    ? [
        'C:\\Program Files\\Common Files\\VST3',
        'C:\\Program Files\\Common Files\\CLAP',
        '%LOCALAPPDATA%\\Programs\\Common\\VST3',
        '%LOCALAPPDATA%\\Programs\\Common\\CLAP',
      ]
    : isMac
    ? ['/Library/Audio/Plug-Ins/VST3', '/Library/Audio/Plug-Ins/Components']
    : ['/usr/lib/vst', '/usr/local/lib/vst'];

  return (
    <Modal
      title={
        <Space size={12} align="center">
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 10,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${token.colorPrimary}1c`,
              border: `1px solid ${token.colorPrimary}38`,
            }}
          >
            <FolderCog size={18} style={{ color: token.colorPrimary }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 16, display: 'block', lineHeight: 1.2 }}>
              {t('scanSettings.title')}
            </Text>
            <Text type="secondary" style={{ fontSize: 11.5 }}>
              {t('scanSettings.subtitle')}
            </Text>
          </div>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 28, maxWidth: 660 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 180px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '16px 22px 22px',
        },
      }}
      zIndex={1200}
      footer={[
        <Button key="close" onClick={onClose} style={{ minWidth: 70, borderRadius: 8 }}>
          {t('scanSettings.close')}
        </Button>,
        <Button
          key="rescan"
          type="primary"
          icon={<RefreshCw size={15} className={loading || isScanning ? "animate-spin" : ""} />}
          loading={loading || isScanning}
          onClick={rescanPlugins}
          style={{ borderRadius: 8 }}
        >
          {t('scanSettings.rescanAll')}
        </Button>,
      ]}
    >
      {contextHolder}

      {/* Info banner */}
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: 12,
          padding: '12px 14px',
          borderRadius: 10,
          background: `${token.colorPrimary}12`,
          border: `1px solid ${token.colorPrimary}30`,
          marginBottom: 18,
        }}
      >
        <Info size={15} style={{ color: token.colorPrimary, marginTop: 2, flexShrink: 0 }} />
        <Text style={{ fontSize: 12, lineHeight: 1.5, color: token.colorTextSecondary }}>
          {t('scanSettings.infoBanner')}
        </Text>
      </div>

      {/* Default paths */}
      <div style={{ marginBottom: 18 }}>
        <Text
          strong
          style={{
            fontSize: 11,
            letterSpacing: '0.06em',
            textTransform: 'uppercase',
            color: token.colorTextSecondary,
          }}
        >
          {t('scanSettings.defaultPaths')}
        </Text>
        <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 6 }}>
          {DEFAULT_PATHS.map((p) => (
            <div
              key={p}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '7px 10px',
                borderRadius: 6,
                background: token.colorBgContainer,
                border: `1px solid ${token.colorBorderSecondary}`,
                fontFamily: 'monospace',
                fontSize: 11.5,
                color: token.colorText,
              }}
            >
              <FolderOpen size={13} style={{ color: token.colorPrimary, flexShrink: 0 }} />
              <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {p}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Custom paths */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
        <Text
          strong
          style={{
            fontSize: 11,
            letterSpacing: '0.06em',
            textTransform: 'uppercase',
            color: token.colorTextSecondary,
          }}
        >
          {t('scanSettings.customPaths')}
        </Text>
        <Button size="small" type="dashed" icon={<Plus size={13} strokeWidth={2} />} onClick={addPath} style={{ borderRadius: 6 }}>
          {t('scanSettings.addPath')}
        </Button>
      </div>

      {customPaths.length > 0 ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {customPaths.map((path, i) => (
            <div
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '8px 12px',
                background: token.colorBgContainer,
                border: `1px solid ${token.colorBorderSecondary}`,
                borderRadius: 8,
                transition: 'all 160ms ease',
              }}
            >
              <FolderOpen size={14} style={{ color: token.colorPrimary, flexShrink: 0 }} />
              <Tooltip title={path}>
                <Text
                  style={{
                    flex: 1,
                    fontSize: 12,
                    fontFamily: 'JetBrains Mono, monospace',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    color: token.colorText,
                  }}
                >
                  {path}
                </Text>
              </Tooltip>
              <Tooltip title={t('scanSettings.copyPath')}>
                <Button
                  type="text"
                  size="small"
                  icon={<Copy size={13} style={{ color: token.colorTextSecondary }} />}
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(path);
                      messageApi.success(t('scanSettings.pathCopied'));
                    } catch {
                      messageApi.error(t('scanSettings.copyFailed'));
                    }
                  }}
                />
              </Tooltip>
              <Tooltip title={t('scanSettings.removePath')}>
                <Button
                  type="text"
                  danger
                  size="small"
                  icon={<Trash2 size={13} />}
                  onClick={() => removePath(path)}
                />
              </Tooltip>
            </div>
          ))}
        </div>
      ) : (
        <div
          style={{
            textAlign: 'center',
            padding: '22px 16px',
            color: token.colorTextTertiary,
            border: `1px dashed ${token.colorBorderSecondary}`,
            borderRadius: 10,
            background: 'rgba(255, 255, 255, 0.02)',
          }}
        >
          <FolderOpen size={24} style={{ color: token.colorTextQuaternary, marginBottom: 6, display: 'inline-block' }} />
          <div><Text type="secondary" style={{ fontSize: 12 }}>{t('scanSettings.noCustomPaths')}</Text></div>
          <div style={{ marginTop: 2 }}>
            <Text type="secondary" style={{ fontSize: 11 }}>{t('scanSettings.clickAddPath')}</Text>
          </div>
        </div>
      )}

      {/* Format tags */}
      <div
        style={{
          marginTop: 18,
          paddingTop: 14,
          borderTop: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 8,
        }}
      >
        <Text type="secondary" style={{ fontSize: 11 }}>Supported formats:</Text>
        <Space size={6}>
          <Tag color="purple">VST3</Tag>
          <Tag color="blue">VST2 (.dll)</Tag>
          <Tag color="green">CLAP</Tag>
        </Space>
      </div>
    </Modal>
  );
}
