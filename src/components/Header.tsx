import { useState } from 'react';
import { Button, Space, Tooltip, Typography, theme, Badge } from 'antd';

const { Text } = Typography;
import {
  AudioOutlined,
  SettingOutlined,
  BulbOutlined,
  BulbFilled,
  LoadingOutlined,
  SoundOutlined,
  MutedOutlined,
} from '@ant-design/icons';
import { useThemeStore } from '../stores/themeStore';
import { useAudioStore } from '../stores/audioStore';
import AudioSettings from './AudioSettings';
import AppSettings from './AppSettings';
import { VUMeter } from './VUMeter';

export default function Header() {
  const { theme: appTheme, toggleTheme } = useThemeStore();
  const { token } = theme.useToken();
  const { status, isMuted, setMuted } = useAudioStore();
  const [showAudioSettings, setShowAudioSettings] = useState(false);
  const [showAppSettings, setShowAppSettings] = useState(false);

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
          <AudioOutlined style={{ fontSize: 22, color: token.colorPrimary }} />
          <Text strong style={{ fontSize: 18 }}>ReLightHost</Text>
          <Text
            style={{
              fontSize: 11,
              background: token.colorFillSecondary,
              padding: '1px 8px',
              borderRadius: token.borderRadiusSM,
              color: token.colorTextSecondary,
            }}
          >
            Beta
          </Text>
        </Space>
        {/* VU Meter */}
        <VUMeter compact />
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
              type={isMuted ? 'primary' : 'text'}
              danger={isMuted}
              icon={isMuted ? <MutedOutlined /> : <SoundOutlined />}
              onClick={() => setMuted(!isMuted)}
            />
          </Tooltip>
          {/* Theme toggle */}
          <Tooltip title={appTheme === 'dark' ? 'Switch to Light' : 'Switch to Dark'}>
            <Button
              type="text"
              icon={appTheme === 'dark' ? <BulbFilled /> : <BulbOutlined />}
              onClick={toggleTheme}
            />
          </Tooltip>

          {/* Audio Settings */}
          <Tooltip title="Audio Settings">
            <Button
              type="text"
              icon={<AudioOutlined />}
              onClick={() => setShowAudioSettings(true)}
            />
          </Tooltip>

          {/* App Settings */}
          <Tooltip title="Application Settings">
            <Button
              type="text"
              icon={<SettingOutlined />}
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
