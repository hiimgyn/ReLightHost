import { useState, useEffect } from 'react';
import { Modal, Switch, Descriptions, Space, Typography, Card, Button, Select, theme } from 'antd';
import { 
  Settings2, 
  Rocket, 
  Info,
  RefreshCw,
  Download,
  Languages,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { APP_AUTHOR, APP_GITHUB_URL, APP_NAME } from '../../lib/appInfo';
import { openExternalUrl } from '../../lib/tauri';
import { useTranslation } from '../../i18n';

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
  const { t, locale, setLocale } = useTranslation();
  const [runOnStartup, setRunOnStartup] = useState(() => readCachedBool(KEYS.startup, false));
  const [showAppOnStartup, setShowAppOnStartup] = useState(() => readCachedBool(KEYS.showOnStartup, true));
  const [minimizeToTray, setMinimizeToTray] = useState(() => readCachedBool(KEYS.minimize, false));
  const [appVersion, setAppVersion] = useState('');
  const [updateInfo, setUpdateInfo] = useState<{ available: boolean; version?: string; notes?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  const { token } = theme.useToken();
  const modalWidth = typeof window === 'undefined' ? 660 : 'clamp(520px, 65vw, 680px)';

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
    background: token.colorBgContainer,
    borderRadius: 10,
    border: `1px solid ${token.colorBorderSecondary}`,
    transition: 'all 160ms ease',
  };

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
            <Settings2 size={18} style={{ color: token.colorPrimary }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 16, display: 'block', lineHeight: 1.2, color: token.colorText }}>
              {t('appSettings.title')}
            </Text>
            <Text type="secondary" style={{ fontSize: 11.5 }}>
              {t('appSettings.subtitle')}
            </Text>
          </div>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      style={{ top: 28, maxWidth: 680 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 180px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '16px 22px 22px',
        },
      }}
      footer={null}
    >
      {/* Language Settings */}
      <Card
        title={
          <Space>
            <Languages size={16} style={{ color: token.colorPrimary }} />
            <span>{t('appSettings.language')}</span>
          </Space>
        }
        style={{ marginBottom: 20 }}
        styles={{ body: { padding: 16 } }}
      >
        <div style={settingRowStyle}>
          <div style={{ flex: 1, paddingRight: 16 }}>
            <Text strong>{t('appSettings.language')}</Text>
            <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
              {t('appSettings.languageDesc')}
            </Paragraph>
          </div>
          <Select
            value={locale}
            onChange={(val) => setLocale(val)}
            style={{ width: 140 }}
            options={[
              { value: 'en', label: t('appSettings.languageEnglish') },
              { value: 'vi', label: t('appSettings.languageVietnamese') },
            ]}
          />
        </div>
      </Card>

      {/* Startup Settings */}
      <Card
        title={
          <Space>
            <Rocket size={16} style={{ color: token.colorPrimary }} />
            <span>{t('appSettings.startupBehavior')}</span>
          </Space>
        }
        style={{ marginBottom: 20 }}
        styles={{ body: { padding: 16 } }}
      >
        <Space direction="vertical" style={{ width: '100%' }} size="middle">
          <div style={settingRowStyle}>
            <div style={{ flex: 1, paddingRight: 16 }}>
              <Text strong>{t('appSettings.runOnStartup')}</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                {t('appSettings.runOnStartupDesc')}
              </Paragraph>
            </div>
            <Switch checked={runOnStartup} onChange={handleStartupToggle} />
          </div>

          <div style={{ ...settingRowStyle, opacity: runOnStartup ? 1 : 0.6 }}>
            <div style={{ flex: 1, paddingRight: 16 }}>
              <Text strong>{t('appSettings.showOnStartup')}</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                {t('appSettings.showOnStartupDesc')}
              </Paragraph>
            </div>
            <Switch
              checked={showAppOnStartup}
              onChange={handleShowAppOnStartupToggle}
              disabled={!runOnStartup}
            />
          </div>

          <div style={settingRowStyle}>
            <div style={{ flex: 1, paddingRight: 16 }}>
              <Text strong>{t('appSettings.minimizeToTray')}</Text>
              <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 12 }}>
                {t('appSettings.minimizeToTrayDesc')}
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
            <Info size={16} />
            <span>{t('appSettings.about')}</span>
          </Space>
        }
        styles={{ body: { padding: 16 } }}
      >
        <Descriptions bordered column={1} size="small">
          <Descriptions.Item label={t('appSettings.application')}>
            <Text strong>{APP_NAME}</Text>
          </Descriptions.Item>
          <Descriptions.Item label={t('appSettings.version')}>
            <Text strong>{appVersion ? `v${appVersion}` : 'Beta'}</Text>
          </Descriptions.Item>
          <Descriptions.Item label={t('appSettings.author')}>
            <Text strong>{APP_AUTHOR}</Text>
          </Descriptions.Item>
          <Descriptions.Item label={t('appSettings.github')}>
            <Button
              type="link"
              style={{ padding: 0, height: 'auto' }}
              onClick={async () => {
                try {
                  await openExternalUrl(APP_GITHUB_URL);
                } catch (error) {
                  console.error('Failed to open GitHub repository:', error);
                }
              }}
            >
              {APP_GITHUB_URL}
            </Button>
          </Descriptions.Item>
          <Descriptions.Item label={t('appSettings.updates')}>
            <Space direction="vertical" size={4}>
              {updateInfo?.available ? (
                <Space direction="vertical" size={4}>
                  <Space>
                    <Text type="success">{t('appSettings.versionAvailable', { version: updateInfo.version || '' })}</Text>
                    <Button
                      size="small"
                      type="primary"
                      icon={<Download size={14} />}
                      loading={installing}
                      onClick={handleInstallUpdate}
                    >
                      {t('appSettings.installRestart')}
                    </Button>
                  </Space>
                  {updateInfo.notes && (
                    <Text type="secondary" style={{ fontSize: 11 }}>{updateInfo.notes}</Text>
                  )}
                </Space>
              ) : (
                <Space>
                  {updateInfo !== null && (
                    <Text type="secondary" style={{ fontSize: 12 }}>{t('appSettings.upToDate')}</Text>
                  )}
                  <Button
                    size="small"
                    icon={<RefreshCw size={13} className={checking ? "animate-spin" : ""} />}
                    loading={checking}
                    onClick={handleCheckUpdate}
                  >
                    {checking ? t('appSettings.checkingUpdates') : t('appSettings.checkForUpdates')}
                  </Button>
                </Space>
              )}
            </Space>
          </Descriptions.Item>
          <Descriptions.Item label={t('appSettings.description')}>
            <Paragraph style={{ marginBottom: 0 }} type="secondary">
              {t('appSettings.appDescription')}
            </Paragraph>
          </Descriptions.Item>
        </Descriptions>
      </Card>
    </Modal>
  );
}