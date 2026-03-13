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
} from '@ant-design/icons';
import { lazy, Suspense, useState, useEffect, useRef } from 'react';
import type { PluginInstanceInfo, PluginStatus } from '../lib/types';
import * as tauri from '../lib/tauri';

const NoiseSuppressorGui = lazy(() => import('./NoiseSuppressorGui'));
const CompressorGui = lazy(() => import('./CompressorGui'));
const VoiceGui = lazy(() => import('./VoiceGui'));

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  crashStatus?: PluginStatus;
  interactionLocked?: boolean;
  onRemove: () => void;
  onToggleBypass: () => void;
  onCrashStatusChanged?: () => Promise<void> | void;
  onLaunch?: () => void;
}

function getFormatColor(format: string) {
  if (format === 'vst3') return 'purple';
  if (format === 'vst')  return 'blue';
  if (format === 'clap') return 'cyan';
  return 'green';
}

export default function PluginCard({
  plugin,
  crashStatus,
  interactionLocked = false,
  onRemove,
  onToggleBypass,
  onCrashStatusChanged,
  onLaunch,
}: PluginCardProps) {
  const { token } = theme.useToken();
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
      setCheckingStatus(true);
      await tauri.resetPluginCrashProtection(plugin.instance_id);
      await onCrashStatusChanged?.();
      message.success('Plugin crash protection reset');
    } catch (err) {
      message.error(`Failed to reset: ${err}`);
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
        await tauri.renamePlugin(plugin.instance_id, trimmed);
      } catch (err) {
        message.error(`Rename failed: ${err}`);
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
    setEditName(plugin.name);
    setIsRenaming(false);
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
    <Card
      size="small"
      className={`transition-all ${plugin.bypassed ? 'opacity-70' : ''}`}
      style={{
        borderWidth: 1,
        borderRadius: 10,
        borderColor: isCrashed
          ? token.colorError
          : plugin.gui_open
          ? token.colorSuccess
          : plugin.bypassed
          ? token.colorBorderSecondary
          : token.colorPrimary,
        background: token.colorBgContainer,
        boxShadow: isCrashed
          ? `0 0 0 1px ${token.colorErrorBorder}, 0 8px 18px ${token.colorErrorBg}`
          : plugin.gui_open
          ? `0 0 0 1px ${token.colorSuccessBorder}, 0 8px 18px rgba(82,196,26,0.12)`
          : `0 6px 14px ${token.colorFillSecondary}`,
      }}
      styles={{ body: { padding: '10px 12px' } }}
    >
      <Space direction="vertical" size={8} style={{ width: '100%' }}>

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
                fontWeight: 600,
                fontSize: 14,
                color: token.colorText,
                flex: 1,
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
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
                    ? token.colorTextQuaternary
                    : plugin.gui_open
                    ? token.colorSuccess
                    : token.colorPrimary,
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
            </>
          )}
        </div>

        {/* ── Meta tags ───────────────────────────────────────────── */}
        <Space size={4} wrap style={{ lineHeight: 1 }}>
          <Tag color={getFormatColor(plugin.format)} style={{ fontSize: 10, margin: 0, fontWeight: 600 }}>
            {plugin.format.toUpperCase()}
          </Tag>
          {plugin.manufacture && (
            <Tag color="default" style={{ fontSize: 10, margin: 0 }}>
              {plugin.manufacture}
            </Tag>
          )}
          {plugin.version && (
            <Tag color="default" style={{ fontSize: 10, margin: 0, opacity: 0.7 }}>
              v{plugin.version}
            </Tag>
          )}
          {plugin.category && plugin.category !== 'Unknown' && (
            <Tag color="geekblue" style={{ fontSize: 10, margin: 0 }}>
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
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr auto', gap: 6, width: '100%' }}>
          {isCrashed && (
            <Tooltip title="Reset Crash Protection">
              <Button
                size="small"
                icon={<ReloadOutlined />}
                onClick={handleResetCrash}
                loading={checkingStatus}
                disabled={interactionLocked}
                style={{ gridColumn: '1 / span 3' }}
              >
                Reset
              </Button>
            </Tooltip>
          )}

          <Tooltip title={plugin.bypassed ? 'Enable Plugin' : 'Bypass Plugin'}>
            <Button
              type="primary"
              size="small"
              icon={<PoweroffOutlined />}
              onClick={onToggleBypass}
              style={{
                width: '100%',
                ...(plugin.bypassed
                  ? { background: 'transparent', borderColor: token.colorBorder, color: token.colorTextSecondary }
                  : { background: token.colorSuccess, borderColor: token.colorSuccess, color: '#fff' }),
              }}
              disabled={isCrashed || isControlLocked}
            >
              {plugin.bypassed ? 'Bypassed' : 'Active'}
            </Button>
          </Tooltip>

          <Tooltip title={plugin.gui_open ? 'GUI already open' : launchButtonProps.label}>
            <Button
              type="default"
              size="small"
              icon={launchButtonProps.icon}
              onClick={launchButtonProps.onClick}
              disabled={isCrashed || isControlLocked}
              style={{
                width: '100%',
                color: plugin.gui_open ? token.colorSuccess : undefined,
                borderColor: plugin.gui_open ? token.colorSuccess : undefined,
              }}
            >
              {launchButtonProps.label}
            </Button>
          </Tooltip>

          <Tooltip title="Remove from Chain">
            <Button
              type="primary"
              danger
              size="small"
              icon={<CloseOutlined />}
              onClick={onRemove}
              disabled={isControlLocked}
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
