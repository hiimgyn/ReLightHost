import { useState, useEffect } from 'react';
import { Button, Space, Tooltip, Typography, theme, Badge } from 'antd';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';

const { Text } = Typography;
import {
  AudioOutlined,
  SettingOutlined,
  BulbOutlined,
  BulbFilled,
  LoadingOutlined,
  SoundOutlined,
  MutedOutlined,
  RetweetOutlined,
} from '@ant-design/icons';
import { useThemeStore } from '../stores/themeStore';
import { useAudioStore } from '../stores/audioStore';
import AudioSettings from './AudioSettings';
import AppSettings from './AppSettings';

export default function Header() {
  const { theme: appTheme, toggleTheme } = useThemeStore();
  const { token } = theme.useToken();
  const { status, isMuted, setMuted, isLoopbackEnabled, setLoopback } = useAudioStore();
  const [showAudioSettings, setShowAudioSettings] = useState(false);
  const [showAppSettings, setShowAppSettings] = useState(false);
  const [appVersion, setAppVersion] = useState('');

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  // Listen for tray context-menu events
  useEffect(() => {
    const unlistens = [
      listen<boolean>('tray-mute-changed',     (e) => setMuted(e.payload)),
      listen<boolean>('tray-loopback-changed', (e) => setLoopback(e.payload)),
      listen('tray-open-audio-settings',       ()  => setShowAudioSettings(true)),
      listen('tray-open-app-settings',         ()  => setShowAppSettings(true)),
    ];
    return () => { unlistens.forEach(p => p.then(fn => fn())); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      <header
        style={{
          background: token.colorBgContainer,
          borderBottom: `1px solid ${token.colorBorderSecondary}`,
          padding: '0 24px',
          height: 56,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexShrink: 0,
        }}
      >
        {/* Logo */}
        <Space size="small">
          <img src="/logo.png" alt="ReLightHost" style={{ width: 64, height: 64 }} />
          <Text
            style={{
              fontSize: 11,
              background: token.colorFillSecondary,
              padding: '1px 8px',
              borderRadius: token.borderRadiusSM,
              color: token.colorTextSecondary,
            }}
          >
            {appVersion ? `v${appVersion}` : 'Beta'}
          </Text>
        </Space>
        {/* Controls */}
        <Space size="middle">
          {/* Stream status badge — read-only */}
          <Space size={6} align="center">
            {status.is_monitoring
              ? <Badge status="processing" color={token.colorSuccess} />
              : <LoadingOutlined style={{ fontSize: 12, color: token.colorTextSecondary }} />}
            <Text
              style={{
                fontSize: 12,
                color: status.is_monitoring ? token.colorSuccess : token.colorTextSecondary,
              }}
            >
              {status.is_monitoring ? 'Running' : 'Loading…'}
            </Text>
          </Space>
          {/* Mute toggle */}
          <Tooltip title={isMuted ? 'Unmute output' : 'Mute output'}>
            <Button
              type="text"
              icon={isMuted
                ? <MutedOutlined style={{ color: '#ff4d4f' }} />
                : <SoundOutlined style={{ color: '#52c41a' }} />}
              onClick={() => setMuted(!isMuted)}
            />
          </Tooltip>
          {/* Loopback toggle — routes processed audio to Hardware Out for monitoring */}
          <Tooltip title={isLoopbackEnabled ? 'Turn off monitoring (Hardware Out silent)' : 'Turn on monitoring (hear output through Hardware Out)'}>
            <Button
              type="text"
              icon={
                <RetweetOutlined
                  style={{ color: isLoopbackEnabled ? token.colorPrimary : token.colorTextSecondary }}
                />
              }
              onClick={() => setLoopback(!isLoopbackEnabled)}
            />
          </Tooltip>
          {/* Theme toggle */}
          <Tooltip title={appTheme === 'dark' ? 'Switch to Light' : 'Switch to Dark'}>
            <Button
              type="text"
              icon={appTheme === 'dark'
                ? <BulbFilled style={{ color: '#faad14' }} />
                : <BulbOutlined style={{ color: '#faad14' }} />}
              onClick={toggleTheme}
            />
          </Tooltip>

          {/* Audio Settings */}
          <Tooltip title="Audio Settings">
            <Button
              type="text"
              icon={<AudioOutlined style={{ color: '#1677ff' }} />}
              onClick={() => setShowAudioSettings(true)}
            />
          </Tooltip>

          {/* App Settings */}
          <Tooltip title="Application Settings">
            <Button
              type="text"
              icon={<SettingOutlined style={{ color: '#9b72cf' }} />}
              onClick={() => setShowAppSettings(true)}
            />
          </Tooltip>
        </Space>
      </header>

      <AudioSettings
        isOpen={showAudioSettings}
        onClose={() => setShowAudioSettings(false)}
      />
      <AppSettings
        isOpen={showAppSettings}
        onClose={() => setShowAppSettings(false)}
      />
    </>
  );
}
