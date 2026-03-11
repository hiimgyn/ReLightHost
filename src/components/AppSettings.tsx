import { useState, useEffect } from 'react';
import { Modal, Switch, Descriptions, Divider, Space, Typography, Card, Button } from 'antd';
import { 
  SettingOutlined, 
  RocketOutlined, 
  InfoCircleOutlined,
  SyncOutlined,
  CloudDownloadOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';

const { Text, Paragraph } = Typography;

interface AppSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AppSettings({ isOpen, onClose }: AppSettingsProps) {
  const [runOnStartup, setRunOnStartup] = useState(false);
  const [minimizeToTray, setMinimizeToTray] = useState(false);
  const [appVersion, setAppVersion] = useState('');
  const [updateInfo, setUpdateInfo] = useState<{ available: boolean; version?: string; notes?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  useEffect(() => {
    if (isOpen) {
      loadSettings();
    }
  }, [isOpen]);

  const loadSettings = async () => {
    try {
      const [startupEnabled, minimizeEnabled] = await Promise.all([
        invoke<boolean>('is_startup_enabled'),
        invoke<boolean>('get_minimize_to_tray'),
      ]);
      setRunOnStartup(startupEnabled);
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

  const handleMinimizeToggle = async (checked: boolean) => {
    try {
      await invoke('set_minimize_to_tray', { enabled: checked });
      setMinimizeToTray(checked);
      localStorage.setItem('minimizeToTray', String(checked));
    } catch (error) {
      console.error('Failed to save minimize_to_tray:', error);
    }
  };

  const handleCheckUpdate = async () => {
    setChecking(true);
    setUpdateInfo(null);
    try {
      const info = await invoke<{ available: boolean; version?: string; notes?: string }>('check_for_update');
      setUpdateInfo(info);
    } catch (error) {
      console.error('Failed to check for updates:', error);
      setUpdateInfo({ available: false });
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      await invoke('install_update');
    } catch (error) {
      console.error('Failed to install update:', error);
      setInstalling(false);
    }
  };

  return (
    <Modal
      title={
        <Space>
          <SettingOutlined style={{ color: '#9b72cf ' }} />
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
            <Text strong>{appVersion ? `v${appVersion}` : 'Beta'}</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Author">
            <Text strong>HiimGyn</Text>
          </Descriptions.Item>
          <Descriptions.Item label="Updates">
            <Space direction="vertical" size={4}>
              {updateInfo?.available ? (
                <Space direction="vertical" size={4}>
                  <Space>
                    <Text type="success">v{updateInfo.version} available</Text>
                    <Button
                      size="small"
                      type="primary"
                      icon={<CloudDownloadOutlined />}
                      loading={installing}
                      onClick={handleInstallUpdate}
                    >
                      Install &amp; Restart
                    </Button>
                  </Space>
                  {updateInfo.notes && (
                    <Text type="secondary" style={{ fontSize: 11 }}>{updateInfo.notes}</Text>
                  )}
                </Space>
              ) : (
                <Space>
                  {updateInfo !== null && (
                    <Text type="secondary" style={{ fontSize: 12 }}>You're up to date</Text>
                  )}
                  <Button
                    size="small"
                    icon={<SyncOutlined />}
                    loading={checking}
                    onClick={handleCheckUpdate}
                  >
                    Check for updates
                  </Button>
                </Space>
              )}
            </Space>
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
