import { Card, Button, Space, Tag, Tooltip, theme, message, Input } from 'antd';
import type { InputRef } from 'antd';
import { 
  CloseOutlined, 
  PoweroffOutlined, 
  PlayCircleOutlined,
  LoadingOutlined,
  CheckCircleOutlined,
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
  onRemove: () => void;
  onToggleBypass: () => void;
  onCrashStatusChanged?: () => Promise<void> | void;
  onLaunch?: () => void;
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

  const handleToggleBypassClick = () => {
    console.debug('PluginCard: toggle bypass clicked', { instanceId: plugin.instance_id, name: plugin.name, currentlyBypassed: plugin.bypassed });
    onToggleBypass();
  };

  const handleRemoveClick = () => {
    console.debug('PluginCard: remove clicked', { instanceId: plugin.instance_id, name: plugin.name });
    onRemove();
  };

  const effectiveCrashStatus = crashStatus ?? { type: 'Ok' };
  const isCrashed = effectiveCrashStatus.type !== 'Ok';
  const isControlLocked = interactionLocked || isLaunching || checkingStatus || isRenamingBusy;

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
      className={`transition-all ${plugin.bypassed ? 'opacity-70' : ''}`}
      style={{
        borderRadius: 12,
        background: token.colorBgElevated,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        // Softer shadow + subtle glow when active/crashed
        boxShadow: isCrashed
          ? `0 0 0 1px ${token.colorErrorBorder}, 0 18px 40px rgba(0,0,0,0.6)`
          : plugin.gui_open
          ? `0 10px 36px rgba(155,114,207,0.08)`
          : `0 8px 22px rgba(2,6,23,0.55)`,
      }}
      styles={{ body: { padding: '16px 16px 22px' } }}
    >
      <Space orientation="vertical" size={8} style={{ width: '100%', display: 'flex', flexDirection: 'column', height: '100%' }}>

        {/* ── Name row ────────────────────────────────────────────── */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
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
              <span style={{
                fontWeight: 700,
                fontSize: 15,
                color: token.colorText,
                flex: 1,
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                fontFamily: 'Inter, system-ui, sans-serif',
              }}>
                {plugin.name}
              </span>
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: '50%',
                  flexShrink: 0,
                  background: isCrashed
                    ? token.colorError
                    : plugin.bypassed
                    ? token.colorTextTertiary
                    : plugin.gui_open
                    ? token.colorSuccess
                    : token.colorTextTertiary,
                }}
              />
              <Tooltip title="Rename">
                <EditOutlined
                  style={{ fontSize: 12, color: token.colorTextQuaternary, cursor: 'pointer', flexShrink: 0 }}
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
            </>
          )}
        </div>

        {/* ── Meta tags ───────────────────────────────────────────── */}
        <Space size={6} wrap style={{ lineHeight: 1 }}>
          <Tag
            color="default"
            style={{ fontSize: 10, margin: 0, fontWeight: 600, background: 'transparent', border: `1px solid ${token.colorBorderSecondary}`, color: token.colorTextSecondary, padding: '2px 8px' }}
          >
            {plugin.format.toUpperCase()}
          </Tag>
          {plugin.manufacture && (
            <Tag color="default" style={{ fontSize: 10, margin: 0, background: 'transparent', border: `1px solid ${token.colorBorderSecondary}`, color: token.colorTextSecondary, padding: '2px 8px' }}>
              {plugin.manufacture}
            </Tag>
          )}
          {plugin.version && (
            <Tag color="default" style={{ fontSize: 10, margin: 0, opacity: 0.7, background: 'transparent', border: `1px solid ${token.colorBorderSecondary}`, color: token.colorTextSecondary, padding: '2px 8px' }}>
              v{plugin.version}
            </Tag>
          )}
          {plugin.category && plugin.category !== 'Unknown' && (
            <Tag color="default" style={{ fontSize: 10, margin: 0, background: 'transparent', border: `1px solid ${token.colorBorderSecondary}`, color: token.colorTextSecondary, padding: '2px 8px' }}>
              {plugin.category}
            </Tag>
          )}
        </Space>

        {/* ── Crash status ────────────────────────────────────────── */}
        {isCrashed && (
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
        )}

        {/* ── Action Buttons ──────────────────────────────────────── */}
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr auto', gap: 6, width: '100%', marginTop: 'auto' }}>
          {isCrashed && (
            <Tooltip title="Reset Crash Protection">
              <Button
                size="small"
                icon={<ReloadOutlined />}
                onClick={handleResetCrash}
                loading={checkingStatus}
                disabled={interactionLocked}
                className="btn-pill btn-reset"
                style={{ gridColumn: '1 / span 3' }}
              >
                Reset
              </Button>
            </Tooltip>
          )}

          <Tooltip title={plugin.bypassed ? 'Enable Plugin' : 'Bypass Plugin'}>
            <Button
              size="small"
              icon={<PoweroffOutlined />}
              onClick={handleToggleBypassClick}
              className={plugin.bypassed ? 'btn-pill btn-off' : 'btn-pill btn-active'}
              disabled={isCrashed || isControlLocked}
            >
              {plugin.bypassed ? 'Off' : 'On'}
            </Button>
          </Tooltip>

          <Tooltip title={plugin.gui_open ? 'GUI already open' : launchButtonProps.label}>
            <Button
              size="small"
              icon={launchButtonProps.icon}
              onClick={launchButtonProps.onClick}
              disabled={isCrashed || isControlLocked}
              className="btn-pill btn-tonal"
              style={{ width: '100%', color: plugin.gui_open ? token.colorSuccess : undefined }}
            >
              {launchButtonProps.label}
            </Button>
          </Tooltip>

          <Tooltip title="Remove from Chain">
            <Button
              type="text"
              size="small"
              icon={<CloseOutlined />}
              onClick={handleRemoveClick}
              disabled={isControlLocked}
              className="btn-icon"
              style={{ minWidth: 34 }}
            />
          </Tooltip>
        </div>
      </Space>
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
