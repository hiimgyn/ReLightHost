import { ReactNode, useEffect, useState } from 'react';
import { ConfigProvider, theme as antTheme, App as AntApp, Space, Typography, Divider, Tooltip } from 'antd';
import { useShallow } from 'zustand/react/shallow';
import { getThemeTokens, applyThemeCssVars } from '../../theme';
import { HeartFilled } from '@ant-design/icons';
import Header from './Header';
import { VUMeter } from './VUMeter';
import { useThemeStore } from '../../stores/themeStore';
import { useAudioStore } from '../../stores/audioStore';
import { usePluginStore } from '../../stores/pluginStore';
import { getSystemStats, openExternalUrl } from '../../lib/tauri';
import { APP_AUTHOR, APP_GITHUB_URL } from '../../lib/appInfo';
import type { SystemStats } from '../../lib/types';
import { useVisibleInterval } from '../../lib/useVisibleInterval';

const { Text } = Typography;

interface LayoutProps {
  children: ReactNode;
}

/** Inner shell — lives inside ConfigProvider so it can read design tokens. */
function AppShell({ children, isDark }: { children: ReactNode; isDark: boolean }) {
  const { token } = antTheme.useToken();
  const { status, fetchStatus } = useAudioStore(useShallow((s) => ({
    status: s.status,
    fetchStatus: s.fetchStatus,
  })));
  const pluginChain = usePluginStore((s) => s.pluginChain);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  useVisibleInterval(() => {
    fetchStatus();
  }, status.is_monitoring ? 4000 : 12000, true, [status.is_monitoring, fetchStatus]);

  return (
    <div
      className="flex flex-col h-screen min-h-0"
      style={{
        background: 'transparent',
        color: token.colorText,
      }}
    >
      <Header />
      <div className="flex-1 overflow-hidden min-h-0">{children}</div>
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
    // Apply CSS variables to :root for non-AntD styles
    const tokens = getThemeTokens(theme === 'dark');
    applyThemeCssVars(tokens, theme === 'dark');
  }, [theme]);

  const isDark = theme === 'dark';
  // Prepare tokens and provider theme so components can reference shared values
  const themeTokens = getThemeTokens(isDark);

  const providerTheme = {
    algorithm: isDark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
    token: themeTokens,
    components: {
      Card: {
        headerBg: 'transparent',
      },
      Button: {
        primaryShadow: isDark ? '0 4px 18px -2px rgba(107, 112, 255, 0.55)' : '0 4px 14px -2px rgba(99, 103, 255, 0.45)',
      },
      Modal: {
        contentBg: isDark ? '#181b2c' : themeTokens.colorBgElevated,
        headerBg: 'transparent',
      },
      Drawer: {
        colorBgElevated: isDark ? '#141624' : themeTokens.colorBgElevated,
      },
      Input: {
        colorBgContainer: isDark ? '#10121d' : '#ffffff',
        colorBorder: isDark ? 'rgba(255, 255, 255, 0.12)' : 'rgba(99, 103, 255, 0.2)',
        activeBorderColor: themeTokens.colorPrimary,
        hoverBorderColor: isDark ? 'rgba(140, 145, 255, 0.45)' : 'rgba(99, 103, 255, 0.4)',
      },
      Select: {
        colorBgContainer: isDark ? '#10121d' : '#ffffff',
        colorBorder: isDark ? 'rgba(255, 255, 255, 0.12)' : 'rgba(99, 103, 255, 0.2)',
        optionSelectedBg: isDark ? '#22273e' : '#eef0ff',
      },
      Slider: {
        trackBg: themeTokens.colorPrimary,
        trackHoverBg: themeTokens.colorPrimarySoft,
        handleColor: themeTokens.colorPrimary,
      },
      Tooltip: {
        colorBgSpotlight: isDark ? '#1e2136' : 'rgba(255, 255, 255, 0.96)',
        colorTextLightSolid: isDark ? '#ffffff' : '#1F2333',
      },
    },
  };

  return (
    <ConfigProvider theme={providerTheme}>
      <AntApp>
        <AppShell isDark={isDark}>{children}</AppShell>
      </AntApp>
    </ConfigProvider>
  );
}

function MiniMeter({ value, color, width = 60 }: { value: number; color: string; width?: number }) {
  const clamped = Math.min(100, Math.max(0, value));
  return (
    <div
      style={{
        width,
        height: 6,
        background: 'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.08) 100%)',
        borderRadius: 3,
        overflow: 'hidden',
        display: 'inline-block',
        verticalAlign: 'middle',
        border: '1px solid rgba(255,255,255,0.1)',
        boxShadow: 'none',
      }}
    >
      <div
        style={{
          width: `${clamped}%`,
          height: '100%',
          background: color,
          borderRadius: 3,
          transition: 'width 0.55s linear',
          boxShadow: 'none',
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

  const handleOpenGitHub = async () => {
    try {
      await openExternalUrl(APP_GITHUB_URL);
    } catch (error) {
      console.error('Failed to open GitHub repository:', error);
    }
  };

  useEffect(() => {
    void getSystemStats()
      .then(setSys)
      .catch(() => { /* backend not ready yet */ });
  }, []);

  useVisibleInterval(() => {
    void (async () => {
      try {
        const s = await getSystemStats();
        setSys(s);
      } catch { /* backend not ready yet */ }
    })();
  }, 4000, true);

  const cpuColor = sys.cpu_percent > 80 ? 'var(--rh-error)' : sys.cpu_percent > 50 ? 'var(--rh-warning)' : 'var(--rh-success)';
  const ramColor = sys.ram_percent > 85 ? 'var(--rh-error)' : sys.ram_percent > 65 ? 'var(--rh-warning)' : 'var(--rh-info)';

  const sep = <Divider orientation="vertical" style={{ margin: '0 6px', height: 12 }} />;

  return (
    <footer
      className="glass-panel"
      style={{
        margin: 0,
        borderRadius: 0,
        background: 'var(--rh-surface-soft-gradient)',
        border: 'none',
        boxShadow: 'none',
        padding: '0 20px',
        flexShrink: 0,
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        height: 36,
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
          <HeartFilled style={{ color: token.colorError, fontSize: 10 }} />{' '}
          <Tooltip title="Open GitHub repository">
            <button
              type="button"
              onClick={handleOpenGitHub}
              style={{
                color: 'inherit',
                textDecoration: 'none',
                background: 'transparent',
                border: 'none',
                padding: 0,
                cursor: 'pointer',
                font: 'inherit',
              }}
            >
              {APP_AUTHOR}
            </button>
          </Tooltip>
        </Text>
      </Space>
      </div>
    </footer>
  );
}
