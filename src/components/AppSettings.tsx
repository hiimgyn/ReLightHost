import { useState, useEffect, useRef } from 'react';
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

const KEYS = {
  startup: 'appSettings.runOnStartup',
  showOnStartup: 'appSettings.showOnStartup',
  minimize: 'minimizeToTray',
} as const;

function readCachedBool(key: string, fallback: boolean): boolean {
  const raw = localStorage.getItem(key);
  return raw == null ? fallback : raw === 'true';
}

interface AppSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AppSettings({ isOpen, onClose }: AppSettingsProps) {
  const [runOnStartup, setRunOnStartup] = useState(() => readCachedBool(KEYS.startup, false));
  const [showAppOnStartup, setShowAppOnStartup] = useState(() => readCachedBool(KEYS.showOnStartup, true));
  const [minimizeToTray, setMinimizeToTray] = useState(() => readCachedBool(KEYS.minimize, false));
  const [appVersion, setAppVersion] = useState('');
  const [updateInfo, setUpdateInfo] = useState<{ available: boolean; version?: string; notes?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  const hasLoadedRef = useRef(false);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  useEffect(() => {
    if (isOpen && !hasLoadedRef.current) {
      hasLoadedRef.current = true;
      loadSettings();
    }
  }, [isOpen]);

  const loadSettings = async () => {
    try {
      const [startupEnabled, minimizeEnabled, showOnStartupEnabled] = await Promise.all([
        invoke<boolean>('is_startup_enabled'),
        invoke<boolean>('get_minimize_to_tray'),
        invoke<boolean>('get_show_app_on_startup'),
      ]);
      setRunOnStartup(startupEnabled);
      setMinimizeToTray(minimizeEnabled);
      setShowAppOnStartup(showOnStartupEnabled);
      localStorage.setItem(KEYS.startup, String(startupEnabled));
      localStorage.setItem(KEYS.minimize, String(minimizeEnabled));
      localStorage.setItem(KEYS.showOnStartup, String(showOnStartupEnabled));
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  };

  function makeToggleHandler<T>(
    setter: React.Dispatch<React.SetStateAction<T>>,
    invokeCmd: string,
    cacheKey: string,
    argName: string,
  ) {
    return async (checked: T) => {
      setter(checked);
      try {
        await invoke(invokeCmd, { [argName]: checked });
        localStorage.setItem(cacheKey, String(checked));
      } catch (error) {
        console.error(`Failed to invoke ${invokeCmd}:`, error);
        setter((prev) => !prev as T);
      }
    };
  }

  const handleStartupToggle = makeToggleHandler(
    setRunOnStartup as React.Dispatch<React.SetStateAction<boolean>>,
    'toggle_startup',
    KEYS.startup,
    'enable',
  );

  const handleMinimizeToggle = makeToggleHandler(
    setMinimizeToTray as React.Dispatch<React.SetStateAction<boolean>>,
    'set_minimize_to_tray',
    KEYS.minimize,
    'enabled',
  );

  const handleShowAppOnStartupToggle = makeToggleHandler(
    setShowAppOnStartup as React.Dispatch<React.SetStateAction<boolean>>,
    'set_show_app_on_startup',
    KEYS.showOnStartup,
    'enabled',
  );

  const handleCheckUpdate = async () => {
    setChecking(true);
    setUpdateInfo(null);
    try {
      const info = await invoke<{ available: boolean; version?: string; notes?: string }>('check_for_update');
      setUpdateInfo(info);
    } catch {
      setUpdateInfo({ available: false });
    } finally {
      setChecking(false);
    }
  };

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      await invoke('install_update');
    } catch {
      setInstalling(false);
    }
  };

  const settingRowStyle: React.CSSProperties = {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '12px 16px',
    background: 'rgba(255, 255, 255, 0.02)',
    borderRadius: 8,
    border: '1px solid rgba(255, 255, 255, 0.06)',
  };

  return (
    <Modal
      title={
        <Space>
          <SettingOutlined style={{ color: '#9b72cf' }} />
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
          <div style={settingRowStyle}>
            <div style={{ flex: 1 }}>
              <Text strong>Run on System Startup</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                Automatically launch ReLightHost when your computer starts
              </Paragraph>
            </div>
            <Switch checked={runOnStartup} onChange={handleStartupToggle} />
          </div>

          <div style={{ ...settingRowStyle, opacity: runOnStartup ? 1 : 0.6 }}>
            <div style={{ flex: 1 }}>
              <Text strong>Show App Window on Startup</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                When enabled, the app window appears after login. When disabled, it starts hidden in the system tray.
              </Paragraph>
            </div>
            <Switch
              checked={showAppOnStartup}
              onChange={handleShowAppOnStartupToggle}
              disabled={!runOnStartup}
            />
          </div>

          <div style={settingRowStyle}>
            <div style={{ flex: 1 }}>
              <Text strong>Minimize to System Tray</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                Keep ReLightHost running in the background when closed
              </Paragraph>
            </div>
            <Switch checked={minimizeToTray} onChange={handleMinimizeToggle} />
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