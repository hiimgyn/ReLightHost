import { useState, useEffect, useId } from "react";
import { Button, Space, Tooltip, Typography, theme, Badge, message } from "antd";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";

const { Text, Title } = Typography;
import {
  Sliders,
  Settings2,
  Sun,
  Moon,
  Loader2,
  RotateCw,
  Volume2,
  VolumeX,
  Headphones,
  Languages,
} from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useThemeStore } from "../../stores/themeStore";
import { useAudioStore } from "../../stores/audioStore";
import { usePluginStore } from "../../stores/pluginStore";
import { useTranslation } from "../../i18n";
import AudioSettings from "../audio/AudioSettings";
import AppSettings from "../settings/AppSettings";

const Logo = ({ src, size = 52 }: { src: string; size?: number }) => {
  const id = useId();
  const padding = 4;
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      style={{ display: "block" }}
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <filter
          id={`outline-${id}`}
          x="-20%"
          y="-20%"
          width="140%"
          height="140%"
        >
          
          <feMorphology
            in="SourceAlpha"
            operator="dilate"
            radius="1.2"
            result="D"
          />

          <feFlood floodColor="#b96ef7" result="F" />
          <feComposite in="F" in2="D" operator="in" result="outline" />
     
          <feGaussianBlur in="outline" stdDeviation="3" result="blur" />
          <feFlood floodColor="#b96ef7" floodOpacity="1" result="glowColor" />
          <feComposite in="glowColor" in2="blur" operator="in" result="glow" />

          <feMerge>
            <feMergeNode in="glow" />
            <feMergeNode in="outline" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      <image
        href={src}
        x={padding}
        y={padding}
        width={size - padding * 2}
        height={size - padding * 2}
        preserveAspectRatio="xMidYMid meet"
        filter={`url(#outline-${id})`}
      />
    </svg>
  );
};

export default function Header() {
  const { theme: appTheme, toggleTheme } = useThemeStore();
  const { t, locale, toggleLocale } = useTranslation();
  const { token } = theme.useToken();
  const {
    status,
    isMuted,
    setMuted,
    isLoopbackEnabled,
    setLoopback,
    applyExternalMuteState,
    applyExternalLoopbackState,
    reloadDeviceConfig,
    toggleMonitoring,
  } = useAudioStore(useShallow((s) => ({
    status: s.status,
    isMuted: s.isMuted,
    setMuted: s.setMuted,
    isLoopbackEnabled: s.isLoopbackEnabled,
    setLoopback: s.setLoopback,
    applyExternalMuteState: s.applyExternalMuteState,
    applyExternalLoopbackState: s.applyExternalLoopbackState,
    reloadDeviceConfig: s.reloadDeviceConfig,
    toggleMonitoring: s.toggleMonitoring,
  })));
  const { isChainInitializing, pluginChain, restoreTargetCount } = usePluginStore(useShallow((s) => ({
    isChainInitializing: s.isChainInitializing,
    pluginChain: s.pluginChain,
    restoreTargetCount: s.restoreTargetCount,
  })));
  const [showAudioSettings, setShowAudioSettings] = useState(false);
  const [showAppSettings, setShowAppSettings] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [isReloadingDevice, setIsReloadingDevice] = useState(false);
  const [isTogglingEngine, setIsTogglingEngine] = useState(false);

  const isEngineReady = status.is_monitoring && !isChainInitializing;
  const restoredCount = restoreTargetCount == null
    ? pluginChain.length
    : Math.min(pluginChain.length, restoreTargetCount);
  const engineLabel = status.is_monitoring
    ? (isChainInitializing
      ? (restoreTargetCount != null
        ? `${t('header.preparingPlugins')} ${restoredCount}/${restoreTargetCount} ${t('header.pluginsRestored')}`
        : t('header.preparingPlugins'))
      : t('header.engineOn'))
    : t('header.engineOff');

  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => {});
  }, []);

  useEffect(() => {
    const unlistens = [
      listen<boolean>("tray-mute-changed", (e) =>
        applyExternalMuteState(e.payload),
      ),
      listen<boolean>("tray-loopback-changed", (e) =>
        applyExternalLoopbackState(e.payload),
      ),
      listen("tray-open-audio-settings", () => setShowAudioSettings(true)),
      listen("tray-open-app-settings", () => setShowAppSettings(true)),
    ];
    return () => {
      unlistens.forEach((p) => p.then((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleToggleEngine = async () => {
    if (isChainInitializing || isTogglingEngine) return;
    setIsTogglingEngine(true);
    try {
      const nextState = !status.is_monitoring;
      await toggleMonitoring(nextState);
      message.info(nextState ? t('header.engineStarted') : t('header.engineStopped'));
    } catch (error) {
      console.error("Failed to toggle engine:", error);
      message.error(t('header.engineToggleFailed'));
    } finally {
      setIsTogglingEngine(false);
    }
  };

  const handleReloadDevice = async () => {
    setIsReloadingDevice(true);
    try {
      await reloadDeviceConfig();
      message.success(t('header.deviceReloaded'));
    } catch (error) {
      console.error("Failed to reload audio device:", error);
      message.error(t('header.deviceReloadFailed'));
    } finally {
      setIsReloadingDevice(false);
    }
  };

  return (
    <>
      <header
        className="glass-panel"
        style={{
          margin: 0,
          padding: "10px 20px",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          minHeight: 64,
          borderRadius: 0,
          background: 'var(--rh-surface-soft-gradient)',
          border: 'none',
          boxShadow: 'var(--rh-header-shadow)',
        }}
      >
        {/* Brand */}
        <Space size={14} align="center" style={{ minWidth: 0 }}>
          <div
            style={{
              width: 52,
              height: 52,
              overflow: "hidden",
              flexShrink: 0,
            }}
          >
            <Logo src="/logo.png" size={52} />
          </div>
          <div style={{ minWidth: 0 }}>
            <Title
              level={4}
              style={{
                margin: 0,
                fontSize: 19,
                fontWeight: 700,
                letterSpacing: "-0.02em",
                lineHeight: 1.15,
                color: token.colorText,
              }}
            >
              ReLight<span style={{ color: token.colorPrimary }}>Host</span>
            </Title>
            <Space size={8} style={{ marginTop: 4 }} wrap>
              <Text style={{ fontSize: 11, color: token.colorTextTertiary }}>
                VST · VST3 · CLAP
              </Text>
              <Text
                style={{
                  fontSize: 10,
                  fontWeight: 600,
                  letterSpacing: 0.04,
                  textTransform: 'uppercase',
                  color: token.colorPrimary,
                  background: token.colorBgContainer,
                  padding: '1px 8px',
                  borderRadius: 20,
                  border: `1px solid ${token.colorBorderSecondary}`,
                }}
              >
                {appVersion ? `v${appVersion}` : 'Beta'}
              </Text>
            </Space>
          </div>
        </Space>

        {/* Controls */}
        <Space size={10} wrap style={{ justifyContent: "flex-end" }}>
          <Tooltip title={status.is_monitoring ? t('header.clickToStopEngine') : t('header.clickToStartEngine')}>
            <div
              role="button"
              tabIndex={0}
              onClick={handleToggleEngine}
              onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && handleToggleEngine()}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 8,
                padding: "6px 12px",
                borderRadius: 999,
                background: token.colorBgContainer,
                border: `1px solid ${status.is_monitoring ? token.colorPrimaryBorder : token.colorBorderSecondary}`,
                cursor: isChainInitializing || isTogglingEngine ? "not-allowed" : "pointer",
                transition: "all 0.2s cubic-bezier(0.4, 0, 0.2, 1)",
                userSelect: "none",
                boxShadow: status.is_monitoring ? `0 0 10px ${token.colorPrimary}22` : "none",
              }}
              className="rh-engine-badge"
            >
              {isTogglingEngine ? (
                <Loader2 className="animate-spin" size={13} style={{ color: token.colorPrimary }} />
              ) : isEngineReady ? (
                <Badge status="processing" color={token.colorSuccess} />
              ) : status.is_monitoring ? (
                <Loader2 className="animate-spin" size={13} style={{ color: token.colorWarning }} />
              ) : (
                <Badge status="default" color={token.colorTextQuaternary} />
              )}
              <Text
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: "inherit",
                  color: isEngineReady
                    ? token.colorSuccess
                    : status.is_monitoring
                    ? token.colorWarning
                    : token.colorTextSecondary,
                }}
              >
                {engineLabel}
              </Text>
            </div>
          </Tooltip>

          <div style={{ display: 'inline-flex', alignItems: 'center', gap: 2, padding: 4, borderRadius: 12, background: token.colorBgContainer, border: `1px solid ${token.colorBorderSecondary}` }}>
            <Tooltip title={isMuted ? t('header.unmuteOutput') : t('header.muteOutput')}>
              <Button
                type="text"
                size="small"
                icon={
                  isMuted ? (
                    <VolumeX size={15} style={{ color: token.colorError }} />
                  ) : (
                    <Volume2 size={15} style={{ color: token.colorSuccess }} />
                  )
                }
                onClick={() => setMuted(!isMuted)}
              />
            </Tooltip>
            <Tooltip
              title={
                isLoopbackEnabled
                  ? t('header.monitorOutputOn')
                  : t('header.monitorOutputOff')
              }
            >
              <Button
                type="text"
                size="small"
                icon={
                  <Headphones
                    size={15}
                    style={{
                      color: isLoopbackEnabled
                        ? token.colorPrimary
                        : token.colorTextSecondary,
                    }}
                  />
                }
                onClick={() => setLoopback(!isLoopbackEnabled)}
              />
            </Tooltip>
            <Tooltip title={t('header.reloadDeviceTooltip')}>
              <Button
                type="text"
                size="small"
                icon={<RotateCw size={14} className={isReloadingDevice ? "animate-spin" : ""} style={{ color: token.colorInfo }} />}
                loading={isReloadingDevice}
                onClick={handleReloadDevice}
              />
            </Tooltip>
          </div>

          <div style={{ display: 'inline-flex', alignItems: 'center', gap: 2, padding: 4, borderRadius: 12, background: token.colorBgContainer, border: `1px solid ${token.colorBorderSecondary}` }}>
             <Tooltip title={t('header.audioSettingsTooltip')}>
              <Button
                type="text"
                size="small"
                icon={<Sliders size={15} style={{ color: token.colorInfo }} />}
                onClick={() => setShowAudioSettings(true)}
              />
            </Tooltip>
            <Tooltip title={t('header.appSettingsTooltip')}>
              <Button
                type="text"
                size="small"
                icon={<Settings2 size={15} style={{ color: token.colorPrimary }} />}
                onClick={() => setShowAppSettings(true)}
              />
            </Tooltip>
          </div>

          <div style={{ display: 'inline-flex', alignItems: 'center', gap: 2, padding: 4, borderRadius: 12, background: token.colorBgContainer, border: `1px solid ${token.colorBorderSecondary}` }}>
            <Tooltip title={t('header.languageTooltip')}>
              <Button
                type="text"
                size="small"
                onClick={toggleLocale}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 4,
                  fontSize: 11,
                  fontWeight: 700,
                  color: token.colorPrimary,
                  padding: '0 6px',
                }}
              >
                <Languages size={14} />
                <span>{locale.toUpperCase()}</span>
              </Button>
            </Tooltip>
            <Tooltip title={appTheme === "dark" ? t('header.lightThemeTooltip') : t('header.darkThemeTooltip')}>
              <Button
                type="text"
                size="small"
                icon={
                  appTheme === "dark" ? (
                    <Moon size={15} style={{ color: token.colorWarning }} />
                  ) : (
                    <Sun size={15} style={{ color: token.colorWarning }} />
                  )
                }
                onClick={toggleTheme}
              />
            </Tooltip>
          </div>
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
