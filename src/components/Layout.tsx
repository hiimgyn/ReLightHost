import { ReactNode, useEffect, useState } from 'react';
import { ConfigProvider, theme as antTheme, App as AntApp, Space, Typography, Divider, Tooltip } from 'antd';
import { HeartFilled } from '@ant-design/icons';
import Header from './Header';
import { useThemeStore } from '../stores/themeStore';
import { useAudioStore } from '../stores/audioStore';
import { usePluginStore } from '../stores/pluginStore';
import { getSystemStats } from '../lib/tauri';
import type { SystemStats } from '../lib/types';

const { Text } = Typography;

interface LayoutProps {
  children: ReactNode;
}

export default function Layout({ children }: LayoutProps) {
  const { theme } = useThemeStore();
  const { status, toggleMonitoring, fetchStatus } = useAudioStore();
  const { pluginChain } = usePluginStore();

  // Auto-start the audio stream on app launch.
  // The stream stays running until the window closes.
  useEffect(() => {
    const start = async () => {
      try {
        await toggleMonitoring(true);
        await fetchStatus();
      } catch (e) {
        console.error('Failed to auto-start audio stream:', e);
      }
    };
    start();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <ConfigProvider
      theme={{
        algorithm: theme === 'dark' ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: {
          colorPrimary: '#1677ff',
          borderRadius: 8,
        },
      }}
    >
      <AntApp>
        <div
          className="flex flex-col h-screen"
          style={{ background: theme === 'dark' ? '#0a0a0a' : '#f5f5f5' }}
        >
          <Header />
          <div className="flex-1 overflow-hidden" style={{ position: 'relative' }}>
            <div style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '125%',
              height: '125%',
              transform: 'scale(0.8)',
              transformOrigin: 'top left',
            }}>
              {children}
            </div>
          </div>
          <Footer status={status} pluginCount={pluginChain.length} />
        </div>
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

function Footer({ status, pluginCount }: {
  status: { sample_rate: number; buffer_size: number; latency_ms: number };
  pluginCount: number;
}) {
  const { token } = antTheme.useToken();
  const [sys, setSys] = useState<SystemStats>({ cpu_percent: 0, ram_percent: 0, ram_used_mb: 0, ram_total_mb: 0 });

  useEffect(() => {
    const poll = async () => {
      try {
        const s = await getSystemStats();
        setSys(s);
      } catch { /* backend not ready yet */ }
    };
    poll();
    const id = setInterval(poll, 1000);
    return () => clearInterval(id);
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
        justifyContent: 'space-between',
        height: 28,
      }}
    >
      {/* Left: audio engine info */}
      <Space size={0}>
        <Text style={{ fontSize: 11, color: token.colorTextQuaternary }}>
          {(status.sample_rate / 1000).toFixed(1)} kHz
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextQuaternary }}>
          {status.buffer_size} smp
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextQuaternary }}>
          {status.latency_ms.toFixed(1)} ms
        </Text>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextQuaternary }}>
          {pluginCount} plugin{pluginCount !== 1 ? 's' : ''}
        </Text>
      </Space>

      {/* Right: real-time system meters */}
      <Space size={0}>
        {/* CPU meter */}
        <Tooltip title={`App CPU: ${sys.cpu_percent.toFixed(1)}%`}>
          <Space size={4} style={{ cursor: 'default' }}>
            <Text style={{ fontSize: 11, color: token.colorTextQuaternary, width: 28, display: 'inline-block' }}>
              CPU
            </Text>
            <MiniMeter value={sys.cpu_percent} color={cpuColor} width={56} />
            <Text style={{ fontSize: 11, color: token.colorTextQuaternary, fontFamily: 'monospace', width: 36, display: 'inline-block', textAlign: 'right' }}>
              {sys.cpu_percent.toFixed(0)}%
            </Text>
          </Space>
        </Tooltip>
        {sep}
        {/* RAM meter */}
        <Tooltip title={`App RAM: ${sys.ram_used_mb} MB`}>
          <Space size={4} style={{ cursor: 'default' }}>
            <Text style={{ fontSize: 11, color: token.colorTextQuaternary, width: 28, display: 'inline-block' }}>
              RAM
            </Text>
            <MiniMeter value={sys.ram_percent} color={ramColor} width={56} />
            <Text style={{ fontSize: 11, color: token.colorTextQuaternary, fontFamily: 'monospace', width: 36, display: 'inline-block', textAlign: 'right' }}>
              {sys.ram_percent.toFixed(0)}%
            </Text>
          </Space>
        </Tooltip>
        {sep}
        <Text style={{ fontSize: 11, color: token.colorTextQuaternary }}>
          ReLightHost · Made by <HeartFilled style={{ color: '#ff4d4f', fontSize: 10 }} /> Gyn
        </Text>
      </Space>
    </footer>
  );
}
