import { useState, useEffect } from 'react';
import { Modal, Switch, Descriptions, Divider, Space, Typography, Card } from 'antd';
import { 
  SettingOutlined, 
  RocketOutlined, 
  InfoCircleOutlined 
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';

const { Text, Paragraph } = Typography;

interface AppSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AppSettings({ isOpen, onClose }: AppSettingsProps) {
  const [runOnStartup, setRunOnStartup] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(false);

  useEffect(() => {
    if (isOpen) {
      loadSettings();
    }
  }, [isOpen]);

  const loadSettings = async () => {
    try {
      const startupEnabled = await invoke<boolean>('is_startup_enabled');
      setRunOnStartup(startupEnabled);
      
      const minimizeEnabled = localStorage.getItem('minimizeToTray') === 'true';
      setMinimizeToTray(minimizeEnabled);
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  };

  const handleStartupToggle = async (checked: boolean) => {
    try {
      await invoke('toggle_startup', { enable: checked });
      setRunOnStartup(checked);
    } catch (error) {
      console.error('Failed to toggle startup:', error);
    }
  };

  const handleMinimizeToggle = (checked: boolean) => {
    setMinimizeToTray(checked);
    localStorage.setItem('minimizeToTray', String(checked));
  };

  return (
    <Modal
      title={
        <Space>
          <SettingOutlined style={{ color: '#1677ff' }} />
          <span>Application Settings</span>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={700}
      footer={null}
    >
      <Divider />
      
      {/* Startup Settings */}
      <Card 
        title={
          <Space>
            <RocketOutlined />
            <span>Startup Behavior</span>
          </Space>
        }
        style={{ marginBottom: 24 }}
      >
        <Space direction="vertical" style={{ width: '100%' }} size="middle">
          <div style={{ 
            display: 'flex', 
            justifyContent: 'space-between', 
            alignItems: 'center',
            padding: '12px 16px',
            background: 'rgba(255, 255, 255, 0.02)',
            borderRadius: 8,
            border: '1px solid rgba(255, 255, 255, 0.06)'
          }}>
            <div style={{ flex: 1 }}>
              <Text strong>Run on System Startup</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                Automatically launch ReLightHost when your computer starts
              </Paragraph>
            </div>
            <Switch 
              checked={runOnStartup}
              onChange={handleStartupToggle}
            />
          </div>

          <div style={{ 
            display: 'flex', 
            justifyContent: 'space-between', 
            alignItems: 'center',
            padding: '12px 16px',
            background: 'rgba(255, 255, 255, 0.02)',
            borderRadius: 8,
            border: '1px solid rgba(255, 255, 255, 0.06)'
          }}>
            <div style={{ flex: 1 }}>
              <Text strong>Minimize to System Tray</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                Keep ReLightHost running in the background when closed
              </Paragraph>
            </div>
            <Switch 
              checked={minimizeToTray}
              onChange={handleMinimizeToggle}
            />
          </div>
        </Space>
      </Card>

      {/* About Section */}
      <Card 
        title={
          <Space>
            <InfoCircleOutlined />
            <span>About</span>
          </Space>
        }
      >
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label="Application">
            <Text strong>ReLightHost</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Version">
            <Text strong>1.0.0</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Author">
            <Text strong>LightHost Team</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Framework">
            <Text strong>Tauri + React</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Description">
            <Paragraph style={{ marginBottom: 0 }} type="secondary">
              A modern VST/CLAP plugin host built with Rust and TypeScript.
              Supports VST2, VST3, and CLAP plugin formats.
            </Paragraph>
          </Descriptions.Item>
        </Descriptions>
      </Card>
    </Modal>
  );
}
