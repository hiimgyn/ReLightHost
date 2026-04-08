import { Card, Button, Space, Tag, Tooltip, theme, message, Input } from 'antd';
import type { InputRef } from 'antd';
import { 
  AppstoreOutlined,
  ApartmentOutlined,
  CloseOutlined, 
  PoweroffOutlined, 
  PlayCircleOutlined,
  LoadingOutlined,
  CheckCircleOutlined,
  FieldNumberOutlined,
  TagOutlined,
  WarningOutlined,
  ReloadOutlined,
  SettingOutlined,
  EditOutlined,
  CheckOutlined,
  CloseCircleOutlined,
  HolderOutlined,
} from '@ant-design/icons';
import { lazy, Suspense, useState, useEffect, useRef } from 'react';
import type { PluginInstanceInfo, PluginStatus } from '../../lib/types';
import * as tauri from '../../lib/tauri';

const NoiseSuppressorGui = lazy(() => import('../plugin-gui/NoiseSuppressorGui'));
const CompressorGui = lazy(() => import('../plugin-gui/CompressorGui'));
const VoiceGui = lazy(() => import('../plugin-gui/VoiceGui'));

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  crashStatus?: PluginStatus;
  interactionLocked?: boolean;
  onRemove: () => Promise<void> | void;
  onToggleBypass: () => Promise<void> | void;
  onCrashStatusChanged?: () => Promise<void> | void;
  onLaunch?: () => Promise<void> | void;
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
  isDragging?: boolean;
}

export default function PluginCard({
  plugin,
  crashStatus,
  interactionLocked = false,
  onRemove,
  onToggleBypass,
  onCrashStatusChanged,
  onLaunch,
  onDragHandlePointerDown,
  isDragging = false,
}: PluginCardProps) {
  const { token } = theme.useToken();
  const [messageApi, contextHolder] = message.useMessage();
  const [checkingStatus, setCheckingStatus] = useState(false);
  const [showBuiltinGui, setShowBuiltinGui] = useState(false);

  // GUI launching state — reset once plugin.gui_open becomes true or after timeout
  const [isLaunching, setIsLaunching] = useState(false);
  const launchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [isBypassBusy, setIsBypassBusy] = useState(false);
  const [isRemovingBusy, setIsRemovingBusy] = useState(false);

  // Inline rename state
  const [isRenaming, setIsRenaming] = useState(false);
  const [isRenamingBusy, setIsRenamingBusy] = useState(false);
  const [editName, setEditName] = useState(plugin.name);
  const renameInputRef = useRef<InputRef | null>(null);

  // Sync edit name when plugin name changes externally
  useEffect(() => {
    if (!isRenaming) setEditName(plugin.name);
  }, [plugin.name, isRenaming]);

  // When gui_open becomes true, clear the launching spinner
  useEffect(() => {
    if (plugin.gui_open && isLaunching) {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  }, [plugin.gui_open, isLaunching]);

  useEffect(() => () => {
    if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
  }, []);

  useEffect(() => {
    if (isRenaming) {
      setTimeout(() => renameInputRef.current?.focus(), 0);
    }
  }, [isRenaming]);

  const handleResetCrash = async () => {
    if (interactionLocked) return;
    try {
      console.debug('PluginCard: resetCrash clicked', { instanceId: plugin.instance_id, name: plugin.name });
      setCheckingStatus(true);
      await tauri.resetPluginCrashProtection(plugin.instance_id);
      await onCrashStatusChanged?.();
      messageApi.success('Plugin crash protection reset');
    } catch (err) {
      console.debug('PluginCard: resetCrash failed', err);
      messageApi.error(`Failed to reset: ${err}`);
    } finally {
      setCheckingStatus(false);
    }
  };

  const handleLaunch = async () => {
    if (interactionLocked) return;
    if (isLaunching) return;
    if (plugin.gui_open) return; // already open — do nothing
    setIsLaunching(true);
    // Safety fallback: clear spinner after 8 s if gui_open never becomes true
    launchTimerRef.current = setTimeout(() => setIsLaunching(false), 8000);
    try {
      console.debug('PluginCard: launch clicked', { instanceId: plugin.instance_id, name: plugin.name });
      await onLaunch?.();
    } catch {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  };

  const handleRenameConfirm = async () => {
    if (interactionLocked || isRenamingBusy) return;
    const trimmed = editName.trim();
    if (trimmed && trimmed !== plugin.name) {
      try {
        setIsRenamingBusy(true);
        console.debug('PluginCard: rename confirm', { instanceId: plugin.instance_id, from: plugin.name, to: trimmed });
        await tauri.renamePlugin(plugin.instance_id, trimmed);
        } catch (err) {
        messageApi.error(`Rename failed: ${err}`);
        setEditName(plugin.name);
      } finally {
        setIsRenamingBusy(false);
      }
    } else if (!trimmed) {
      setEditName(plugin.name);
    }
    setIsRenaming(false);
  };

  const handleRenameCancel = () => {
    if (isRenamingBusy) return;
    console.debug('PluginCard: rename cancelled', { instanceId: plugin.instance_id, name: plugin.name });
    setEditName(plugin.name);
    setIsRenaming(false);
  };

  const handleToggleBypassClick = async () => {
    if (isControlLocked) return;
    console.debug('PluginCard: toggle bypass clicked', { instanceId: plugin.instance_id, name: plugin.name, currentlyBypassed: plugin.bypassed });
    try {
      setIsBypassBusy(true);
      await onToggleBypass();
    } catch (err) {
      messageApi.error(`Bypass failed: ${err}`);
    } finally {
      setIsBypassBusy(false);
    }
  };

  const handleRemoveClick = async () => {
    if (isControlLocked) return;
    console.debug('PluginCard: remove clicked', { instanceId: plugin.instance_id, name: plugin.name });
    try {
      setIsRemovingBusy(true);
      await onRemove();
    } catch (err) {
      messageApi.error(`Remove failed: ${err}`);
    } finally {
      setIsRemovingBusy(false);
    }
  };

  const effectiveCrashStatus = crashStatus ?? { type: 'Ok' };
  const isCrashed = effectiveCrashStatus.type !== 'Ok';
  const isControlLocked = interactionLocked || isLaunching || checkingStatus || isRenamingBusy || isBypassBusy || isRemovingBusy;
  const isActive = !isCrashed && !plugin.bypassed;
  const statusKind: 'crashed' | 'bypassed' | 'live' | 'active' = isCrashed
    ? 'crashed'
    : plugin.bypassed
    ? 'bypassed'
    : plugin.gui_open
    ? 'live'
    : 'active';
  const statusPalette = {
    crashed: {
      color: token.colorError,
      bg: 'rgba(255,77,79,0.15)',
      border: 'rgba(255,77,79,0.26)',
      glow: 'rgba(255,77,79,0.18)',
    },
    bypassed: {
      color: token.colorTextTertiary,
      bg: token.colorFillQuaternary,
      border: token.colorBorderSecondary,
      glow: 'rgba(0,0,0,0.08)',
    },
    live: {
      color: token.colorWarning,
      bg: 'rgba(250,173,20,0.18)',
      border: 'rgba(250,173,20,0.3)',
      glow: 'rgba(250,173,20,0.18)',
    },
    active: {
      color: token.colorSuccess,
      bg: 'rgba(110,200,166,0.18)',
      border: 'rgba(110,200,166,0.3)',
      glow: 'rgba(110,200,166,0.18)',
    },
  }[statusKind];
  const statusText = statusKind === 'crashed'
    ? 'Crashed'
    : statusKind === 'bypassed'
    ? 'Bypassed'
    : statusKind === 'live'
    ? 'Live'
    : 'Active';
  const bypassButtonColor = statusPalette.color;
  const bypassButtonBg = statusKind === 'bypassed'
    ? 'linear-gradient(135deg, rgba(255,255,255,0.2) 0%, rgba(255,255,255,0.14) 100%)'
    : `linear-gradient(135deg, ${statusPalette.bg} 0%, ${statusPalette.bg} 100%)`;
  const bypassButtonBorder = statusPalette.border;
  const statusDotColor = statusPalette.color;
  const statusTextColor = statusPalette.color;

  type MetaChip = {
    key: string;
    label: string;
    tooltip?: string;
    icon: React.ReactNode;
  };

  const getFormatPalette = (format: PluginInstanceInfo['format']) => {
    switch (format) {
      case 'vst3':
        return {
          border: 'rgba(138, 92, 255, 0.42)',
          background: 'linear-gradient(135deg, rgba(138, 92, 255, 0.18) 0%, rgba(138, 92, 255, 0.08) 100%)',
          color: '#8a5cff',
        };
      case 'clap':
        return {
          border: 'rgba(34, 197, 94, 0.42)',
          background: 'linear-gradient(135deg, rgba(34, 197, 94, 0.18) 0%, rgba(34, 197, 94, 0.08) 100%)',
          color: '#22c55e',
        };
      case 'builtin':
        return {
          border: 'rgba(20, 184, 166, 0.42)',
          background: 'linear-gradient(135deg, rgba(20, 184, 166, 0.18) 0%, rgba(20, 184, 166, 0.08) 100%)',
          color: '#14b8a6',
        };
      case 'vst':
      default:
        return {
          border: 'rgba(245, 158, 11, 0.42)',
          background: 'linear-gradient(135deg, rgba(245, 158, 11, 0.18) 0%, rgba(245, 158, 11, 0.08) 100%)',
          color: '#f59e0b',
        };
    }
  };

  const normalizeManufacturerLabel = (value?: string) => {
    if (!value) return null;
    const normalized = value.trim();
    if (!normalized) return null;
    if (plugin.format === 'builtin' && normalized.toLowerCase().includes('built')) {
      return 'System';
    }
    return normalized;
  };

  const metaChipIconFor = (key: string): React.ReactNode => {
    switch (key) {
      case 'format':
        return <AppstoreOutlined />;
      case 'manufacture':
        return <ApartmentOutlined />;
      case 'version':
        return <FieldNumberOutlined />;
      case 'category':
        return <TagOutlined />;
      default:
        return <TagOutlined />;
    }
  };

  const metaChipStyleFor = (chip: MetaChip): React.CSSProperties => {
    const isPrimary = chip.key === 'format';
    const isManufacturer = chip.key === 'manufacture';
    const formatPalette = getFormatPalette(plugin.format);

    return {
      display: 'inline-flex',
      alignItems: 'center',
      gap: 3,
      minWidth: 0,
      padding: isPrimary ? '2px 6px' : '1px 5px',
      margin: 0,
      borderRadius: 999,
      border: `1px solid ${isPrimary ? formatPalette.border : token.colorBorderSecondary}`,
      background: isPrimary
        ? formatPalette.background
        : isManufacturer
        ? `linear-gradient(135deg, ${token.colorBgElevated} 0%, ${token.colorBgContainer} 100%)`
        : `linear-gradient(135deg, ${token.colorBgContainer} 0%, ${token.colorBgElevated} 100%)`,
      color: isPrimary ? formatPalette.color : token.colorTextSecondary,
      fontSize: isPrimary ? 8.5 : 7.5,
      fontWeight: isPrimary ? 700 : 600,
      letterSpacing: isPrimary ? 0.3 : 0.08,
      textTransform: isPrimary ? 'uppercase' : 'none',
      boxShadow: 'none',
      backdropFilter: 'blur(4px)',
      WebkitBackdropFilter: 'blur(4px)',
      overflow: 'visible',
      whiteSpace: 'nowrap',
      flexShrink: 0,
      lineHeight: 1,
      minHeight: 18,
    };
  };

  const metaChips: MetaChip[] = [
    { key: 'format', label: plugin.format === 'builtin' ? 'SYSTEM' : plugin.format.toUpperCase(), icon: metaChipIconFor('format') },
    normalizeManufacturerLabel(plugin.manufacture)
      ? { key: 'manufacture', label: normalizeManufacturerLabel(plugin.manufacture) as string, tooltip: plugin.manufacture, icon: metaChipIconFor('manufacture') }
      : null,
    plugin.version
      ? { key: 'version', label: `v${plugin.version}`, tooltip: `v${plugin.version}`, icon: metaChipIconFor('version') }
      : null,
    plugin.category && plugin.category !== 'Unknown'
      ? { key: 'category', label: plugin.category, tooltip: plugin.category, icon: metaChipIconFor('category') }
      : null,
  ].filter((chip): chip is MetaChip => chip !== null);

  // Determine launch button appearance
  const launchButtonProps = (() => {
    if (plugin.format === 'builtin') {
      return { icon: <SettingOutlined />, label: 'Settings', onClick: () => setShowBuiltinGui(true) };
    }
    if (plugin.gui_open) {
      return { icon: <CheckCircleOutlined />, label: 'Open', onClick: () => {} };
    }
    if (isLaunching) {
      return { icon: <LoadingOutlined />, label: 'Launching', onClick: () => {} };
    }
    return { icon: <PlayCircleOutlined />, label: 'Launch', onClick: handleLaunch };
  })();

  return (
    <>
    {contextHolder}
    <Card
      size="small"
      className={`glass-card transition-all ${plugin.bypassed ? 'opacity-70' : ''} ${isActive && plugin.gui_open ? 'rh-plugin-live-pulse' : ''}`}
      style={{
        borderRadius: 12,
        background: isCrashed
          ? 'linear-gradient(135deg, rgba(255,77,79,0.15) 0%, rgba(255,77,79,0.08) 100%)'
          : isActive
          ? `linear-gradient(135deg, rgba(99,103,255,0.24) 0%, rgba(132,148,255,0.2) 50%, rgba(255,255,255,0.14) 100%)`
          : `linear-gradient(135deg, rgba(255,255,255,0.2) 0%, rgba(255,255,255,0.14) 100%)`,
        display: 'flex',
        flexDirection: 'column',
        height: 174,
        border: isCrashed
          ? `1px solid rgba(255,77,79,0.2)`
          : isActive
          ? `1px solid rgba(255,255,255,0.34)`
          : `1px solid rgba(255,255,255,0.3)`,
        backdropFilter: 'blur(16px) saturate(1.1)',
        WebkitBackdropFilter: 'blur(16px) saturate(1.1)',
        boxShadow: isCrashed
          ? `0 0 0 1px rgba(255,77,79,0.2), 0 12px 28px rgba(255,77,79,0.15), inset 0 1px 0 rgba(255,255,255,0.1)`
          : isActive
          ? `0 0 0 1px rgba(99,103,255,0.3), 0 12px 28px rgba(99,103,255,0.2), inset 0 1px 0 rgba(255,255,255,0.16)`
          : `0 8px 20px rgba(0,0,0,0.12), inset 0 1px 0 rgba(255,255,255,0.16)`,
      }}
      styles={{ body: { padding: '16px', display: 'flex', flexDirection: 'column', height: '100%' } }}
    >
      <div style={{ width: '100%', display: 'flex', flexDirection: 'column', height: '100%', gap: 8 }}>

        {/* ── Name row ────────────────────────────────────────────── */}
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8, minHeight: 32 }}>
          {isRenaming ? (
            <Input
              ref={renameInputRef}
              size="small"
              disabled={isControlLocked}
              value={editName}
              onChange={e => setEditName(e.target.value)}
              onPressEnter={handleRenameConfirm}
              onKeyDown={e => e.key === 'Escape' && handleRenameCancel()}
              style={{ flex: 1, fontWeight: 600, fontSize: 14 }}
              suffix={
                <Space size={2}>
                  <CheckOutlined
                    style={{ color: token.colorSuccess, cursor: 'pointer' }}
                    onClick={handleRenameConfirm}
                  />
                  <CloseCircleOutlined
                    style={{ color: token.colorTextSecondary, cursor: 'pointer' }}
                    onClick={handleRenameCancel}
                  />
                </Space>
              }
            />
          ) : (
            <>
              <Tooltip title={plugin.name}>
                <span style={{
                  fontWeight: 700,
                  fontSize: 15,
                  lineHeight: 1.25,
                  color: token.colorText,
                  flex: 1,
                  minWidth: 0,
                  overflow: 'hidden',
                  textOverflow: 'clip',
                  display: '-webkit-box',
                  WebkitLineClamp: 2,
                  WebkitBoxOrient: 'vertical',
                  wordBreak: 'break-word',
                  fontFamily: 'Inter, system-ui, sans-serif',
                }}>
                  {plugin.name}
                </span>
              </Tooltip>
              <Space size={4} style={{ alignItems: 'flex-start', flexShrink: 0 }}>
                <Tooltip title="Rename">
                  <EditOutlined
                    style={{ fontSize: 12, color: token.colorTextQuaternary, cursor: 'pointer', flexShrink: 0, marginTop: 1 }}
                    onClick={() => {
                      if (isControlLocked) return;
                      setEditName(plugin.name);
                      setIsRenaming(true);
                    }}
                  />
                </Tooltip>
                {onDragHandlePointerDown && (
                  <Tooltip title="Drag to reorder">
                    <span
                      onPointerDown={onDragHandlePointerDown}
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        width: 22,
                        height: 22,
                        borderRadius: 999,
                        marginLeft: 2,
                        color: isDragging ? token.colorTextLightSolid : token.colorTextTertiary,
                        background: isDragging ? token.colorPrimary : token.colorFillQuaternary,
                        cursor: 'grab',
                        flexShrink: 0,
                      }}
                    >
                      <HolderOutlined style={{ fontSize: 10 }} />
                    </span>
                  </Tooltip>
                )}
              </Space>
            </>
          )}
        </div>

        {/* ── Meta tags ───────────────────────────────────────────── */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', overflow: 'visible', maxHeight: 30, width: '100%', paddingTop: 1 }}>
          {metaChips.map((chip) => (
            <Tooltip title={chip.tooltip ?? chip.label} key={chip.key}>
              <Tag
                color="default"
                style={metaChipStyleFor(chip)}
              >
                <span
                  style={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 3,
                    minWidth: 0,
                    whiteSpace: 'nowrap',
                  }}
                >
                  <span style={{ display: 'inline-flex', alignItems: 'center', fontSize: 8, opacity: 0.88 }}>
                    {chip.icon}
                  </span>
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{chip.label}</span>
                </span>
              </Tag>
            </Tooltip>
          ))}
        </div>

        {/* ── Crash status ────────────────────────────────────────── */}
        {isCrashed ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              borderRadius: 6,
              padding: '6px 8px',
              fontSize: 11,
              color: token.colorErrorText,
              background: token.colorErrorBg,
              border: `1px solid ${token.colorErrorBorder}`,
            }}
          >
            <WarningOutlined />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {effectiveCrashStatus.type === 'Timeout'
                ? 'TIMEOUT'
                : `${effectiveCrashStatus.type}: ${effectiveCrashStatus.data ?? ''}`}
            </span>
          </div>
        ) : null}

        {/* ── Bottom status + actions ────────────────────────────── */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4, width: '100%', marginTop: 'auto' }}>
          <div
            style={{
              minHeight: 16,
              display: 'flex',
              alignItems: 'center',
              gap: 5,
              fontSize: 8.5,
              color: statusTextColor,
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: statusDotColor,
                boxShadow: `0 0 10px ${statusPalette.glow}`,
              }}
            />
            <span style={{ fontWeight: 600, letterSpacing: 0.18, textTransform: 'uppercase' }}>{statusText}</span>
          </div>

          <div style={{ display: 'flex', gap: 8, width: '100%', alignItems: 'center' }}>
          {isCrashed && (
            <Tooltip title="Reset Crash Protection">
              <Button
                size="small"
                icon={<ReloadOutlined />}
                onClick={handleResetCrash}
                loading={checkingStatus}
                disabled={interactionLocked}
                className="btn-pill btn-reset"
                style={{ flex: 1, height: 32 }}
              >
                Reset
              </Button>
            </Tooltip>
          )}

          {!isCrashed && (
            <Tooltip title={plugin.bypassed ? 'Enable Plugin' : 'Bypass Plugin'}>
              <Button
                type="text"
                size="small"
                icon={<PoweroffOutlined />}
                onClick={() => { void handleToggleBypassClick(); }}
                loading={isBypassBusy}
                className="btn-pill"
                disabled={isControlLocked}
                aria-label={plugin.bypassed ? 'Enable plugin' : 'Bypass plugin'}
                style={{
                  minWidth: 36,
                  width: 36,
                  height: 32,
                  justifyContent: 'center',
                  paddingInline: 0,
                  color: plugin.bypassed ? token.colorTextSecondary : bypassButtonColor,
                  background: bypassButtonBg,
                  borderColor: bypassButtonBorder,
                  boxShadow: plugin.bypassed
                    ? '0 1px 2px rgba(0, 0, 0, 0.08)'
                    : `0 4px 16px ${statusPalette.glow}`,
                }}
              >
              </Button>
            </Tooltip>
          )}

          {!isCrashed && (
            <Tooltip title={plugin.gui_open ? 'GUI already open' : launchButtonProps.label}>
              <Button
                size="small"
                icon={launchButtonProps.icon}
                onClick={launchButtonProps.onClick}
                disabled={isControlLocked}
                className="btn-pill btn-tonal"
                style={{ flex: 1, height: 32, color: plugin.gui_open ? token.colorSuccess : undefined }}
              >
                {launchButtonProps.label}
              </Button>
            </Tooltip>
          )}

          <Tooltip title="Remove from Chain">
            <Button
              type="text"
              size="small"
              icon={<CloseOutlined />}
              onClick={() => { void handleRemoveClick(); }}
              loading={isRemovingBusy}
              disabled={isControlLocked}
              className="btn-icon"
              style={{ minWidth: 32, width: 32, height: 32 }}
            />
          </Tooltip>
          </div>
        </div>
      </div>
    </Card>

    {showBuiltinGui && plugin.format === 'builtin' && plugin.plugin_id === 'builtin::noise_suppressor' && (
      <Suspense fallback={null}>
        <NoiseSuppressorGui
          plugin={plugin}
          isOpen={showBuiltinGui}
          onClose={() => setShowBuiltinGui(false)}
        />
      </Suspense>
    )}
    {showBuiltinGui && plugin.format === 'builtin' && plugin.plugin_id === 'builtin::compressor' && (
      <Suspense fallback={null}>
        <CompressorGui
          plugin={plugin}
          isOpen={showBuiltinGui}
          onClose={() => setShowBuiltinGui(false)}
        />
      </Suspense>
    )}
    {showBuiltinGui && plugin.format === 'builtin' && plugin.plugin_id === 'builtin::voice' && (
      <Suspense fallback={null}>
        <VoiceGui
          plugin={plugin}
          isOpen={showBuiltinGui}
          onClose={() => setShowBuiltinGui(false)}
        />
      </Suspense>
    )}
  </>
  );
}
