import { useState, useEffect } from 'react';
import { Modal, Button, Space, Typography, Tag, Divider, message } from 'antd';
import { FolderOpenOutlined, DeleteOutlined, PlusOutlined, SettingOutlined, ReloadOutlined } from '@ant-design/icons';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { usePluginStore } from '../stores/pluginStore';

const { Text, Paragraph } = Typography;

interface PluginSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PluginSettings({ isOpen, onClose }: PluginSettingsProps) {
  const [customPaths, setCustomPaths] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const { scanPlugins, isScanning } = usePluginStore();

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
        message.success('Path added');
      }
    } catch (err) {
      message.error(`Failed to add path: ${err}`);
    }
  };

  const removePath = async (path: string) => {
    try {
      await invoke('remove_custom_scan_path', { path });
      await loadPaths();
      message.success('Path removed');
    } catch (err) {
      message.error(`Failed to remove path: ${err}`);
    }
  };

  const rescanPlugins = async () => {
    setLoading(true);
    try {
      await scanPlugins();
      message.success('Plugin scan completed');
      onClose();
    } catch (err) {
      message.error(`Scan failed: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const DEFAULT_PATHS = [
    'C:\\Program Files\\Common Files\\VST3',
    'C:\\Program Files\\Common Files\\CLAP',
    '%LOCALAPPDATA%\\Programs\\Common\\VST3',
    '%LOCALAPPDATA%\\Programs\\Common\\CLAP',
  ];

  return (
    <Modal
      title={
        <Space>
          <SettingOutlined style={{ color: '#9b72cf' }} />
          <span>Plugin Scan Paths</span>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={580}
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
      <Divider />

      {/* Info banner */}
      <div style={{
        background: 'rgba(155, 114, 207, 0.1)',
        border: '1px solid rgba(155, 114, 207, 0.3)',
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
          <Text key={p} code style={{ fontSize: 12 }}>{p}</Text>
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
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                padding: '8px 12px',
                background: 'rgba(255,255,255,0.03)',
                border: '1px solid rgba(255,255,255,0.08)',
                borderRadius: 6,
              }}
            >
              <FolderOpenOutlined style={{ color: '#9b72cf', flexShrink: 0 }} />
              <Text style={{ flex: 1, fontSize: 12, fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {path}
              </Text>
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
        <div style={{
          textAlign: 'center',
          padding: '24px 0',
          color: '#666',
          border: '1px dashed rgba(255,255,255,0.1)',
          borderRadius: 8,
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
