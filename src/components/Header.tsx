import { useState } from 'react';
import { Button, Space, Tooltip, Typography, theme } from 'antd';

const { Text } = Typography;
import {
  AudioOutlined,
  SettingOutlined,
  BulbOutlined,
  BulbFilled,
} from '@ant-design/icons';
import { useThemeStore } from '../stores/themeStore';
import AudioSettings from './AudioSettings';
import AppSettings from './AppSettings';

export default function Header() {
  const { theme: appTheme, toggleTheme } = useThemeStore();
  const { token } = theme.useToken();
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
            v0.1.0
          </Text>
        </Space>

        {/* Controls */}
        <Space size="middle">
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
