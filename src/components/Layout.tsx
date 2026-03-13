import { ReactNode, useEffect, useState } from 'react';
import { ConfigProvider, theme as antTheme, App as AntApp, Space, Typography, Divider, Tooltip } from 'antd';
import { HeartFilled } from '@ant-design/icons';
import Header from './Header';
import { VUMeter } from './VUMeter';
import { useThemeStore } from '../stores/themeStore';
import { useAudioStore } from '../stores/audioStore';
import { usePluginStore } from '../stores/pluginStore';
import { getSystemStats } from '../lib/tauri';
import type { SystemStats } from '../lib/types';

const { Text } = Typography;

interface LayoutProps {
  children: ReactNode;
}

/** Inner shell — lives inside ConfigProvider so it can read design tokens. */
function AppShell({ children, isDark }: { children: ReactNode; isDark: boolean }) {
  const { token } = antTheme.useToken();
  const { status, fetchStatus } = useAudioStore();
  const { pluginChain } = usePluginStore();

  useEffect(() => {
    const poll = () => {
      if (document.visibilityState === 'visible') {
        fetchStatus();
      }
    };
    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        fetchStatus();
      }
    };

    poll();
    const id = setInterval(poll, 2000);
    document.addEventListener('visibilitychange', onVisible);

    return () => {
      clearInterval(id);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [fetchStatus]);

  return (
    <div
      className="flex flex-col h-screen"
      style={{ background: token.colorBgLayout, color: token.colorText }}
    >
      <Header />
      <div className="flex-1 overflow-hidden">
        {children}
      </div>
      <Footer status={status} pluginCount={pluginChain.length} isDark={isDark} />
    </div>
  );
}

export default function Layout({ children }: LayoutProps) {
  const { theme } = useThemeStore();

  // Keep the dark CSS class in sync with the persisted theme value.
  useEffect(() => {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [theme]);

  const isDark = theme === 'dark';

  return (
    <ConfigProvider
      theme={{
        algorithm: isDark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: {
          colorPrimary: '#9b72cf',
          colorInfo: '#1890ff',
          borderRadius: 8,
          fontFamily:
            'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
          // Layout background
          colorBgLayout: isDark ? '#0a0a0a' : '#f0f2f5',
          // Container / card background
          colorBgContainer: isDark ? '#141414' : '#ffffff',
          // Elevated (modal, dropdown) background
          colorBgElevated: isDark ? '#1f1f1f' : '#ffffff',
          // Border
          colorBorder: isDark ? '#303030' : '#d9d9d9',
          colorBorderSecondary: isDark ? '#1f1f1f' : '#f0f0f0',
          // Text hierarchy
          colorText: isDark ? 'rgba(255,255,255,0.88)' : 'rgba(0,0,0,0.88)',
          colorTextSecondary: isDark ? 'rgba(255,255,255,0.55)' : 'rgba(0,0,0,0.60)',
          colorTextTertiary: isDark ? 'rgba(255,255,255,0.35)' : 'rgba(0,0,0,0.45)',
          colorTextQuaternary: isDark ? 'rgba(255,255,255,0.25)' : 'rgba(0,0,0,0.30)',
        },
        components: {
          Card: {
            headerBg: 'transparent',
          },
          Modal: {
            contentBg: isDark ? '#141414' : '#ffffff',
            headerBg: isDark ? '#141414' : '#ffffff',
          },
          Select: {
            optionSelectedBg: isDark ? '#2a1f3d' : '#f5f0ff',
          },
          Slider: {
            trackBg: '#9b72cf',
            trackHoverBg: '#b08ee0',
            handleColor: '#9b72cf',
          },
        },
      }}
    >
      <AntApp>
        <AppShell isDark={isDark}>{children}</AppShell>
      </AntApp>
    </ConfigProvider>
  );
}

function MiniMeter({ value, color, width = 60 }: { value: number; color: string; width?: number }) {
  const { token } = antTheme.useToken();
  const clamped = Math.min(100, Math.max(0, value));
  return (
    <div
      style={{
        width,
        height: 6,
        background: token.colorFillTertiary,
        borderRadius: 3,
        overflow: 'hidden',
        display: 'inline-block',
        verticalAlign: 'middle',
      }}
    >
      <div
        style={{
          width: `${clamped}%`,
          height: '100%',
          background: color,
          borderRadius: 3,
          transition: 'width 0.4s ease',
        }}
      />
    </div>
  );
}

function Footer({ status, pluginCount, isDark }: {
  status: { sample_rate: number; buffer_size: number; latency_ms: number };
  pluginCount: number;
  isDark: boolean;
}) {
  const { token } = antTheme.useToken();
  const [sys, setSys] = useState<SystemStats>({ cpu_percent: 0, ram_percent: 0, ram_used_mb: 0, ram_total_mb: 0 });

  useEffect(() => {
    const poll = async () => {
      if (document.visibilityState !== 'visible') return;
      try {
        const s = await getSystemStats();
        setSys(s);
      } catch { /* backend not ready yet */ }
    };
    poll();
    const id = setInterval(poll, 2000);
    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        poll();
      }
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      clearInterval(id);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, []);

  const cpuColor = sys.cpu_percent > 80 ? '#ff4d4f' : sys.cpu_percent > 50 ? '#faad14' : '#52c41a';
  const ramColor = sys.ram_percent > 85 ? '#ff4d4f' : sys.ram_percent > 65 ? '#faad14' : '#1677ff';

  const sep = <Divider type="vertical" style={{ margin: '0 6px', height: 12 }} />;

  return (
    <footer
      style={{
        background: token.colorBgContainer,
        borderTop: `1px solid ${token.colorBorderSecondary}`,
        padding: '0 24px',
        flexShrink: 0,
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        height: 28,
      }}
    >
      {/* Left: audio engine info */}
      <Space size={0} style={{ flex: 1, minWidth: 0 }}>
        <Text style={{ fontSize: 11, color: token.colorTextTertiary }}>
          {(status.sample_rate / 1000).toFixed(1)} kHz
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextTertiary }}>
          {status.buffer_size} smp
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextTertiary }}>
          {status.latency_ms.toFixed(1)} ms
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextTertiary }}>
          {pluginCount} plugin{pluginCount !== 1 ? 's' : ''}
        </Text>
      </Space>

      {/* Center: VU Meter */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          pointerEvents: 'none',
        }}
      >
        <VUMeter isDark={isDark} />
      </div>

      {/* Right: real-time system meters */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', justifyContent: 'flex-end' }}>
      <Space size={0}>
        {/* CPU meter */}
        <Tooltip title={`CPU: ${sys.cpu_percent.toFixed(1)}% (app)`}>
          <Space size={4} style={{ cursor: 'default' }}>
            <Text style={{ fontSize: 11, color: token.colorTextTertiary, width: 28, display: 'inline-block' }}>
              CPU
            </Text>
            <MiniMeter value={sys.cpu_percent} color={cpuColor} width={56} />
            <Text style={{ fontSize: 11, color: token.colorTextTertiary, fontFamily: 'monospace', width: 36, display: 'inline-block', textAlign: 'right' }}>
              {sys.cpu_percent.toFixed(0)}%
            </Text>
          </Space>
        </Tooltip>
        {sep}
        {/* RAM meter */}
        <Tooltip title={`RAM: ${sys.ram_used_mb} MB used`}>
          <Space size={4} style={{ cursor: 'default' }}>
            <Text style={{ fontSize: 11, color: token.colorTextTertiary, width: 28, display: 'inline-block' }}>
              RAM
            </Text>
            <MiniMeter value={sys.ram_percent} color={ramColor} width={56} />
            <Text style={{ fontSize: 11, color: token.colorTextTertiary, fontFamily: 'monospace', width: 36, display: 'inline-block', textAlign: 'right' }}>
              {sys.ram_percent.toFixed(0)}%
            </Text>
          </Space>
        </Tooltip>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextSecondary }}>
          <HeartFilled style={{ color: '#ff4d4f', fontSize: 10 }} /> HiimGyn
        </Text>
      </Space>
      </div>
    </footer>
  );
}
