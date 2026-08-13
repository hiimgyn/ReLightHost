import { Card, Button, Space, Tooltip, theme, message, Input } from 'antd';
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
import { memo, useState } from 'react';
import type { PluginInstanceInfo, PluginStatus } from '../../lib/types';
import * as tauri from '../../lib/tauri';
import PluginMetaChips from './PluginMetaChips';
import BuiltinPluginGuiSwitch from './BuiltinPluginGuiSwitch';
import { usePluginRename } from './usePluginRename';
import { usePluginLaunch } from './usePluginLaunch';
import { getPluginStatusPalette } from './pluginStatusPalette';

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  crashStatus?: PluginStatus;
  interactionLocked?: boolean;
  onRemove: (instanceId: string) => Promise<void> | void;
  onToggleBypass: (instanceId: string) => Promise<void> | void;
  onCrashStatusChanged?: () => Promise<void> | void;
  onLaunch?: (instanceId: string) => Promise<void> | void;
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
  isDragging?: boolean;
}

function PluginCard({
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
  const [isBypassBusy, setIsBypassBusy] = useState(false);
  const [isRemovingBusy, setIsRemovingBusy] = useState(false);

  const { isLaunching, handleLaunch } = usePluginLaunch({
    instanceId: plugin.instance_id,
    pluginName: plugin.name,
    guiOpen: plugin.gui_open,
    interactionLocked,
    onLaunch,
  });

  const {
    isRenaming,
    isRenamingBusy,
    editName,
    setEditName,
    renameInputRef,
    startRenaming,
    confirmRename,
    cancelRename,
  } = usePluginRename({
    instanceId: plugin.instance_id,
    pluginName: plugin.name,
    interactionLocked,
    messageApi,
  });

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

  const handleToggleBypassClick = async () => {
    if (isControlLocked) return;
    console.debug('PluginCard: toggle bypass clicked', { instanceId: plugin.instance_id, name: plugin.name, currentlyBypassed: plugin.bypassed });
    try {
      setIsBypassBusy(true);
      await onToggleBypass(plugin.instance_id);
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
      await onRemove(plugin.instance_id);
    } catch (err) {
      messageApi.error(`Remove failed: ${err}`);
    } finally {
      setIsRemovingBusy(false);
    }
  };

  const isControlLocked = interactionLocked || isLaunching || checkingStatus || isRenamingBusy || isBypassBusy || isRemovingBusy;
  const {
    isCrashed,
    isActive,
    statusText,
    bypassButtonColor,
    bypassButtonBg,
    bypassButtonBorder,
    color: statusDotColor,
  } = getPluginStatusPalette(plugin, crashStatus, token);
  const effectiveCrashStatus = crashStatus ?? { type: 'Ok' as const };

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
      className={`glass-card transition-colors ${plugin.bypassed ? 'opacity-70' : ''}`}
      style={{
        borderRadius: 12,
        background: isCrashed ? 'rgba(255,77,79,0.12)' : token.colorBgElevated,
        display: 'flex',
        flexDirection: 'column',
        height: 174,
        border: isCrashed
          ? `1px solid rgba(255,77,79,0.2)`
          : isActive
          ? `1px solid var(--rh-surface-soft-border-strong)`
          : `1px solid var(--rh-surface-soft-border)`,
        boxShadow: 'none',
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
              onPressEnter={confirmRename}
              onKeyDown={e => e.key === 'Escape' && cancelRename()}
              style={{ flex: 1, fontWeight: 600, fontSize: 14 }}
              suffix={
                <Space size={2}>
                  <CheckOutlined
                    style={{ color: token.colorSuccess, cursor: 'pointer' }}
                    onClick={confirmRename}
                  />
                  <CloseCircleOutlined
                    style={{ color: token.colorTextSecondary, cursor: 'pointer' }}
                    onClick={cancelRename}
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
                    onClick={startRenaming}
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
        <PluginMetaChips plugin={plugin} />

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
                : `${effectiveCrashStatus.type}: ${'data' in effectiveCrashStatus ? effectiveCrashStatus.data : ''}`}
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
              color: statusDotColor,
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: statusDotColor,
                boxShadow: 'none',
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
                  boxShadow: 'none',
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

    <BuiltinPluginGuiSwitch
      plugin={plugin}
      open={showBuiltinGui}
      onClose={() => setShowBuiltinGui(false)}
    />
  </>
  );
}

export default memo(PluginCard);
