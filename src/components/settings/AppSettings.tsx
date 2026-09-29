import { useState, useEffect } from 'react';
import { Modal, Switch, Space, Typography, Button, Select, Tabs, theme } from 'antd';
import { 
  Settings2, 
  Info,
  RefreshCw,
  Download,
  ExternalLink,
  Sliders,
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
  parallelVst3Loading: 'appSettings.parallelVst3Loading',
  wasapiExclusive: 'appSettings.wasapiExclusive',
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
  const [parallelVst3Loading, setParallelVst3LoadingState] = useState(() => readCachedBool(KEYS.parallelVst3Loading, false));
  const [wasapiExclusive, setWasapiExclusive] = useState(() => readCachedBool(KEYS.wasapiExclusive, false));
  const [appVersion, setAppVersion] = useState('');
  const [updateInfo, setUpdateInfo] = useState<{ available: boolean; version?: string; notes?: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  const { token } = theme.useToken();
  const modalWidth = typeof window === 'undefined' ? 560 : 'clamp(480px, 52vw, 560px)';

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
      const [startupEnabled, minimizeEnabled, showOnStartupEnabled, parallelVst3Enabled, wasapiExclusiveEnabled] = await Promise.all([
        invoke<boolean>('is_startup_enabled'),
        invoke<boolean>('get_minimize_to_tray'),
        invoke<boolean>('get_show_app_on_startup'),
        invoke<boolean>('get_parallel_vst3_loading'),
        invoke<boolean>('get_wasapi_exclusive'),
      ]);
      setRunOnStartup(startupEnabled);
      setMinimizeToTray(minimizeEnabled);
      setShowAppOnStartup(showOnStartupEnabled);
      setParallelVst3LoadingState(parallelVst3Enabled);
      setWasapiExclusive(wasapiExclusiveEnabled);
      localStorage.setItem(KEYS.startup, String(startupEnabled));
      localStorage.setItem(KEYS.minimize, String(minimizeEnabled));
      localStorage.setItem(KEYS.showOnStartup, String(showOnStartupEnabled));
      localStorage.setItem(KEYS.parallelVst3Loading, String(parallelVst3Enabled));
      localStorage.setItem(KEYS.wasapiExclusive, String(wasapiExclusiveEnabled));
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

  const handleParallelVst3LoadingToggle = makeToggleHandler(
    setParallelVst3LoadingState as React.Dispatch<React.SetStateAction<boolean>>,
    'set_parallel_vst3_loading',
    KEYS.parallelVst3Loading,
    'enabled',
  );

  const handleWasapiExclusiveToggle = makeToggleHandler(
    setWasapiExclusive as React.Dispatch<React.SetStateAction<boolean>>,
    'set_wasapi_exclusive',
    KEYS.wasapiExclusive,
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
    padding: '10px 14px',
    background: token.colorBgContainer,
    borderRadius: 8,
    border: `1px solid ${token.colorBorderSecondary}`,
    transition: 'all 160ms ease',
  };

  const generalTab = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 4 }}>
      {/* Language */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.language')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
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

      {/* Run on Startup */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.runOnStartup')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.runOnStartupDesc')}
          </Paragraph>
        </div>
        <Switch checked={runOnStartup} onChange={handleStartupToggle} />
      </div>

      {/* Show Window on Startup */}
      <div style={{ ...settingRowStyle, opacity: runOnStartup ? 1 : 0.55 }}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.showOnStartup')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.showOnStartupDesc')}
          </Paragraph>
        </div>
        <Switch
          checked={showAppOnStartup}
          onChange={handleShowAppOnStartupToggle}
          disabled={!runOnStartup}
        />
      </div>

      {/* Minimize to Tray */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.minimizeToTray')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.minimizeToTrayDesc')}
          </Paragraph>
        </div>
        <Switch checked={minimizeToTray} onChange={handleMinimizeToggle} />
      </div>

      {/* Parallel VST3 Loading (experimental) */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.parallelVst3Loading')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.parallelVst3LoadingDesc')}
          </Paragraph>
        </div>
        <Switch checked={parallelVst3Loading} onChange={handleParallelVst3LoadingToggle} />
      </div>

      {/* WASAPI exclusive mode */}
      <div style={settingRowStyle}>
        <div style={{ flex: 1, paddingRight: 16 }}>
          <Text strong style={{ fontSize: 13 }}>{t('appSettings.wasapiExclusive')}</Text>
          <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11.5 }}>
            {t('appSettings.wasapiExclusiveDesc')}
          </Paragraph>
        </div>
        <Switch checked={wasapiExclusive} onChange={handleWasapiExclusiveToggle} />
      </div>
    </div>
  );

  const aboutTab = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginTop: 4 }}>
      {/* App Identity Banner */}
      <div
        style={{
          padding: '12px 14px',
          background: token.colorBgContainer,
          borderRadius: 8,
          border: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 16,
        }}
      >
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
            <Text strong style={{ fontSize: 15 }}>{APP_NAME}</Text>
            <span
              style={{
                fontSize: 10,
                fontWeight: 700,
                fontFamily: 'JetBrains Mono, monospace',
                padding: '1px 6px',
                borderRadius: 4,
                background: `${token.colorPrimary}22`,
                color: token.colorPrimary,
                border: `1px solid ${token.colorPrimary}44`,
              }}
            >
              {appVersion ? `v${appVersion}` : 'v1.0.0'}
            </span>
          </div>
          <Paragraph type="secondary" style={{ marginBottom: 4, fontSize: 11.5, lineHeight: 1.4 }}>
            {t('appSettings.appDescription')}
          </Paragraph>
          <Text type="secondary" style={{ fontSize: 11 }}>
            {t('appSettings.author')}: <Text strong style={{ fontSize: 11 }}>{APP_AUTHOR}</Text>
          </Text>
        </div>

        <Button
          icon={<ExternalLink size={13} />}
          size="small"
          style={{ borderRadius: 6, flexShrink: 0 }}
          onClick={async () => {
            try {
              await openExternalUrl(APP_GITHUB_URL);
            } catch (error) {
              console.error('Failed to open GitHub repository:', error);
            }
          }}
        >
          GitHub
        </Button>
      </div>

      {/* Updates Section */}
      <div
        style={{
          padding: '12px 14px',
          background: token.colorBgContainer,
          borderRadius: 8,
          border: `1px solid ${token.colorBorderSecondary}`,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
        }}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <Text strong style={{ fontSize: 13, display: 'block' }}>{t('appSettings.updates')}</Text>
          {updateInfo?.available ? (
            <div>
              <Text type="success" style={{ fontSize: 11.5, fontWeight: 600 }}>
                {t('appSettings.versionAvailable', { version: updateInfo.version || '' })}
              </Text>
              {updateInfo.notes && (
                <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 11 }}>
                  {updateInfo.notes}
                </Paragraph>
              )}
            </div>
          ) : updateInfo !== null ? (
            <Text type="secondary" style={{ fontSize: 11.5 }}>{t('appSettings.upToDate')}</Text>
          ) : (
            <Text type="secondary" style={{ fontSize: 11.5 }}>ReLightHost update channel</Text>
          )}
        </div>

        {updateInfo?.available ? (
          <Button
            type="primary"
            icon={<Download size={13} />}
            loading={installing}
            onClick={handleInstallUpdate}
            style={{ borderRadius: 6 }}
            size="small"
          >
            {t('appSettings.installRestart')}
          </Button>
        ) : (
          <Button
            icon={<RefreshCw size={12} className={checking ? 'animate-spin' : ''} />}
            loading={checking}
            onClick={handleCheckUpdate}
            style={{ borderRadius: 6 }}
            size="small"
          >
            {checking ? t('appSettings.checkingUpdates') : t('appSettings.checkForUpdates')}
          </Button>
        )}
      </div>
    </div>
  );

  const tabItems = [
    {
      key: 'general',
      label: (
        <Space size={6}>
          <Sliders size={14} />
          <span>{t('appSettings.general') || 'General'}</span>
        </Space>
      ),
      children: generalTab,
    },
    {
      key: 'about',
      label: (
        <Space size={6}>
          <Info size={14} />
          <span>{t('appSettings.about')}</span>
        </Space>
      ),
      children: aboutTab,
    },
  ];

  return (
    <Modal
      title={
        <Space size={12} align="center">
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
            <Settings2 size={16} style={{ color: token.colorPrimary }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 15, display: 'block', lineHeight: 1.2, color: token.colorText }}>
              {t('appSettings.title')}
            </Text>
            <Text type="secondary" style={{ fontSize: 11 }}>
              {t('appSettings.subtitle')}
            </Text>
          </div>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={modalWidth}
      centered
      styles={{
        body: {
          padding: '4px 18px 16px',
        },
      }}
      footer={null}
    >
      <Tabs className="app-settings-tabs" defaultActiveKey="general" items={tabItems} />
    </Modal>
  );
}