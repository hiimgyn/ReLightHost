import { useState, useEffect } from 'react';
import { Modal, Button, Space, Typography, Tag, Divider, message, theme, Tooltip } from 'antd';
import { FolderOpenOutlined, DeleteOutlined, PlusOutlined, SettingOutlined, ReloadOutlined, CopyOutlined } from '@ant-design/icons';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { usePluginStore } from '../../stores/pluginStore';

const { Text, Paragraph } = Typography;

interface PluginSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginSettings({ isOpen, onClose }: PluginSettingsProps) {
  const [customPaths, setCustomPaths] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const { scanPlugins, isScanning } = usePluginStore();
  const [messageApi, contextHolder] = message.useMessage();
  const { token } = theme.useToken();
  const modalWidth = typeof window === 'undefined' ? 416 : 'clamp(300px, 54vw, 416px)';

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
      const selected = await open({ directory: true, multiple: false, title: 'Select Plugin Directory' });
      if (selected && typeof selected === 'string') {
        await invoke('add_custom_scan_path', { path: selected });
        await loadPaths();
        messageApi.success('Path added');
      }
    } catch (err) {
      messageApi.error(`Failed to add path: ${err}`);
    }
  };

  const removePath = async (path: string) => {
    try {
      await invoke('remove_custom_scan_path', { path });
      await loadPaths();
      messageApi.success('Path removed');
    } catch (err) {
      messageApi.error(`Failed to remove path: ${err}`);
    }
  };

  const rescanPlugins = async () => {
    setLoading(true);
    try {
      await scanPlugins();
      messageApi.success('Plugin scan completed');
      onClose();
    } catch (err) {
      messageApi.error(`Scan failed: ${err}`);
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
      className="minimal-panel"
      title={
        <Space>
          <SettingOutlined style={{ color: token.colorPrimary }} />
          <Text strong style={{ fontSize: 15, letterSpacing: '-0.01em', color: token.colorText }}>Plugin Scan Paths</Text>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 12, maxWidth: 416 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 220px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '12px 14px 14px',
        },
      }}
      zIndex={1200}
      footer={[
        <Button key="close" onClick={onClose}>Close</Button>,
        <Button
          key="rescan"
          type="primary"
          icon={<ReloadOutlined />}
          loading={loading || isScanning}
          onClick={rescanPlugins}
        >
          Rescan All Plugins
        </Button>,
      ]}
    >
      {contextHolder}


      {/* Info banner */}
      <div className="minimal-surface" style={{
        background: 'var(--rh-primary-glow)',
        border: `1px solid ${token.colorPrimary}`,
        borderRadius: 8,
        padding: '10px 14px',
        marginBottom: 20,
      }}>
        <Paragraph style={{ margin: 0, fontSize: 13 }} type="secondary">
          Add custom directories where your VST3 and CLAP plugins are installed.
          These paths are scanned in addition to the default system paths.
        </Paragraph>
      </div>

      {/* Default paths */}
      <Text strong style={{ fontSize: 12, letterSpacing: '0.05em' }}>DEFAULT SYSTEM PATHS</Text>
      <div style={{ marginTop: 8, marginBottom: 20, display: 'flex', flexDirection: 'column', gap: 4 }}>
        {DEFAULT_PATHS.map(p => (
          <Tooltip key={p} title={p}>
            <Text code style={{ fontSize: 12, display: 'inline-block', maxWidth: '100%', overflowWrap: 'anywhere' }}>{p}</Text>
          </Tooltip>
        ))}
      </div>

      {/* Custom paths */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
        <Text strong style={{ fontSize: 12, letterSpacing: '0.05em' }}>CUSTOM PATHS</Text>
        <Button size="small" icon={<PlusOutlined />} onClick={addPath}>Add Path</Button>
      </div>

      {customPaths.length > 0 ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {customPaths.map((path, i) => (
            <div
              className="minimal-surface"
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '8px 12px',
                background: 'var(--rh-surface-soft-gradient)',
                border: `1px solid var(--rh-surface-soft-border)`,
                borderRadius: 6,
                transition: 'all 200ms ease',
              }}
            >
              <FolderOpenOutlined style={{ color: token.colorPrimary, flexShrink: 0 }} />
              <Tooltip title={path}>
                <Text style={{ flex: 1, fontSize: 12, fontFamily: 'monospace', overflowWrap: 'anywhere', wordBreak: 'break-all' }}>
                  {path}
                </Text>
              </Tooltip>
              <Button
                type="text"
                size="small"
                icon={<CopyOutlined />}
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(path);
                    messageApi.success('Path copied');
                  } catch {
                    messageApi.error('Failed to copy');
                  }
                }}
              />
              <Button
                type="text"
                danger
                size="small"
                icon={<DeleteOutlined />}
                onClick={() => removePath(path)}
              />
            </div>
          ))}
        </div>
      ) : (
        <div className="minimal-surface" style={{
          textAlign: 'center',
          padding: '24px 0',
          color: token.colorTextTertiary,
          border: `1px dashed ${token.colorBorderSecondary}`,
          borderRadius: 8,
          background: token.colorBgContainer,
        }}>
          <Text type="secondary" style={{ fontSize: 13 }}>No custom paths configured</Text><br />
          <Text type="secondary" style={{ fontSize: 12 }}>Click "Add Path" to add a custom scan directory</Text>
        </div>
      )}

      {/* Format tags */}
      <Divider />
      <Space size={6}>
        <Text type="secondary" style={{ fontSize: 12 }}>Supported formats:</Text>
        <Tag color="purple">VST3</Tag>
        <Tag color="blue">VST2 (.dll)</Tag>
        <Tag color="green">CLAP</Tag>
      </Space>
    </Modal>
  );
}
