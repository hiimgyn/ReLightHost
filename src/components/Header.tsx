import { useState, useEffect, useId } from "react";
import { Button, Space, Tooltip, Typography, theme, Badge } from "antd";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";

const { Text, Title } = Typography;
import {
  AudioOutlined,
  SettingOutlined,
  BulbOutlined,
  BulbFilled,
  LoadingOutlined,
  SoundOutlined,
  MutedOutlined,
  RetweetOutlined,
} from "@ant-design/icons";
import { useThemeStore } from "../stores/themeStore";
import { useAudioStore } from "../stores/audioStore";
import AudioSettings from "./AudioSettings";
import AppSettings from "./AppSettings";

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

      <mask id={`mask-${id}`}>
        <image
          href={src}
          x={padding}
          y={padding}
          width={size - padding * 2}
          height={size - padding * 2}
          preserveAspectRatio="xMidYMid meet"
        />
      </mask>

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
  const { token } = theme.useToken();
  const {
    status,
    isMuted,
    setMuted,
    isLoopbackEnabled,
    setLoopback,
    applyExternalMuteState,
    applyExternalLoopbackState,
  } = useAudioStore();
  const [showAudioSettings, setShowAudioSettings] = useState(false);
  const [showAppSettings, setShowAppSettings] = useState(false);
  const [appVersion, setAppVersion] = useState("");

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

  const isDark = appTheme === "dark";

  return (
    <>
      <header
        style={{
          margin: "12px 16px 0",
          padding: "12px 20px",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          minHeight: 64,
          borderRadius: token.borderRadiusLG * 1.25,
          background: isDark
            ? "rgba(22, 22, 29, 0.92)"
            : "rgba(252, 252, 255, 0.92)",
          border: `1px solid ${isDark ? "rgba(155,114,207,0.18)" : "rgba(155,114,207,0.14)"}`,
          boxShadow: isDark
            ? "0 8px 32px rgba(0,0,0,0.45), 0 0 0 1px rgba(255,255,255,0.03) inset"
            : "0 8px 28px rgba(15,23,42,0.08), 0 0 0 1px rgba(255,255,255,0.7) inset",
          backdropFilter: "blur(16px)",
          WebkitBackdropFilter: "blur(16px)",
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
                  textTransform: "uppercase",
                  color: token.colorPrimary,
                  background: isDark
                    ? "rgba(155,114,207,0.14)"
                    : "rgba(155,114,207,0.12)",
                  padding: "1px 8px",
                  borderRadius: 20,
                  border: `1px solid ${isDark ? "rgba(155,114,207,0.28)" : "rgba(155,114,207,0.2)"}`,
                }}
              >
                {appVersion ? `v${appVersion}` : "Beta"}
              </Text>
            </Space>
          </div>
        </Space>

        {/* Controls */}
        <Space size={10} wrap style={{ justifyContent: "flex-end" }}>
          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 8,
              padding: "6px 12px",
              borderRadius: 999,
              background: isDark
                ? "rgba(255,255,255,0.04)"
                : "rgba(15,23,42,0.04)",
              border: `1px solid ${token.colorBorderSecondary}`,
            }}
          >
            {status.is_monitoring ? (
              <Badge status="processing" color={token.colorSuccess} />
            ) : (
              <LoadingOutlined
                style={{ fontSize: 12, color: token.colorTextSecondary }}
              />
            )}
            <Text
              style={{
                fontSize: 12,
                fontWeight: 600,
                color: status.is_monitoring
                  ? token.colorSuccess
                  : token.colorTextSecondary,
              }}
            >
              {status.is_monitoring ? "Engine on" : "Starting…"}
            </Text>
          </div>

          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 2,
              padding: 4,
              borderRadius: 12,
              background: isDark
                ? "rgba(255,255,255,0.05)"
                : "rgba(15,23,42,0.045)",
              border: `1px solid ${token.colorBorderSecondary}`,
            }}
          >
            <Tooltip title={isMuted ? "Unmute output" : "Mute output"}>
              <Button
                type="text"
                size="small"
                icon={
                  isMuted ? (
                    <MutedOutlined style={{ color: "#ff4d4f" }} />
                  ) : (
                    <SoundOutlined style={{ color: "#52c41a" }} />
                  )
                }
                onClick={() => setMuted(!isMuted)}
              />
            </Tooltip>
            <Tooltip
              title={
                isLoopbackEnabled
                  ? "Monitoring off — hardware out silent"
                  : "Monitoring on — hear processed audio on hardware out"
              }
            >
              <Button
                type="text"
                size="small"
                icon={
                  <RetweetOutlined
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
            <Tooltip title={appTheme === "dark" ? "Light theme" : "Dark theme"}>
              <Button
                type="text"
                size="small"
                icon={
                  appTheme === "dark" ? (
                    <BulbFilled style={{ color: "#faad14" }} />
                  ) : (
                    <BulbOutlined style={{ color: "#faad14" }} />
                  )
                }
                onClick={toggleTheme}
              />
            </Tooltip>
            <Tooltip title="Audio devices & engine">
              <Button
                type="text"
                size="small"
                icon={<AudioOutlined style={{ color: token.colorInfo }} />}
                onClick={() => setShowAudioSettings(true)}
              />
            </Tooltip>
            <Tooltip title="Application settings">
              <Button
                type="text"
                size="small"
                icon={<SettingOutlined style={{ color: token.colorPrimary }} />}
                onClick={() => setShowAppSettings(true)}
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
