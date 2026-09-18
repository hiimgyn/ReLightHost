import { useState, useEffect } from 'react';
import { Modal, Button, Space, Typography, Tag, message, theme, Tooltip } from 'antd';
import {
  FolderOpenOutlined,
  DeleteOutlined,
  PlusOutlined,
  SettingOutlined,
  ReloadOutlined,
  CopyOutlined,
  InfoCircleOutlined,
} from '@ant-design/icons';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { usePluginStore } from '../../stores/pluginStore';

const { Text } = Typography;

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
  const modalWidth = typeof window === 'undefined' ? 440 : 'clamp(320px, 56vw, 440px)';

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
            <SettingOutlined style={{ color: token.colorPrimary, fontSize: 16 }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 15, display: 'block', lineHeight: 1.2 }}>
              Plugin Scan Paths
            </Text>
            <Text type="secondary" style={{ fontSize: 11 }}>
              Configure VST3, VST2, and CLAP directories
            </Text>
          </div>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 20, maxWidth: 440 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 200px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '14px 18px 18px',
        },
      }}
      zIndex={1200}
      footer={[
        <Button key="close" onClick={onClose} style={{ minWidth: 70, borderRadius: 8 }}>
          Close
        </Button>,
        <Button
          key="rescan"
          type="primary"
          icon={<ReloadOutlined spin={loading || isScanning} />}
          loading={loading || isScanning}
          onClick={rescanPlugins}
          style={{ borderRadius: 8 }}
        >
          Rescan All Plugins
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
        <InfoCircleOutlined style={{ color: token.colorPrimary, fontSize: 15, marginTop: 2, flexShrink: 0 }} />
        <Text style={{ fontSize: 12, lineHeight: 1.5, color: token.colorTextSecondary }}>
          Add custom directories where your VST3 and CLAP plugins are installed.
          These paths are scanned in addition to default system locations.
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
          Default System Paths
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
              <FolderOpenOutlined style={{ color: token.colorPrimary, fontSize: 13, flexShrink: 0 }} />
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
          Custom Paths
        </Text>
        <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={addPath} style={{ borderRadius: 6 }}>
          Add Path
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
              <FolderOpenOutlined style={{ color: token.colorPrimary, flexShrink: 0 }} />
              <Tooltip title={path}>
                <Text
                  style={{
                    flex: 1,
                    fontSize: 12,
                    fontFamily: 'monospace',
                    overflowWrap: 'anywhere',
                    wordBreak: 'break-all',
                  }}
                >
                  {path}
                </Text>
              </Tooltip>
              <Tooltip title="Copy path">
                <Button
                  type="text"
                  size="small"
                  icon={<CopyOutlined style={{ color: token.colorTextSecondary }} />}
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(path);
                      messageApi.success('Path copied');
                    } catch {
                      messageApi.error('Failed to copy');
                    }
                  }}
                />
              </Tooltip>
              <Tooltip title="Remove path">
                <Button
                  type="text"
                  danger
                  size="small"
                  icon={<DeleteOutlined />}
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
          <FolderOpenOutlined style={{ fontSize: 24, color: token.colorTextQuaternary, marginBottom: 6, display: 'inline-block' }} />
          <div><Text type="secondary" style={{ fontSize: 12 }}>No custom paths configured</Text></div>
          <div style={{ marginTop: 2 }}>
            <Text type="secondary" style={{ fontSize: 11 }}>Click "Add Path" to add a custom scan directory</Text>
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
