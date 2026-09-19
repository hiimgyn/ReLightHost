import { Card, Button, Space, Tooltip, theme, message, Input } from 'antd';
import {
  X,
  Power,
  ExternalLink,
  Loader2,
  CheckCircle2,
  AlertTriangle,
  RotateCcw,
  Sliders,
  Pencil,
  Check,
  GripVertical,
} from 'lucide-react';
import { memo, useState, useEffect, useRef } from 'react';
import gsap from 'gsap';
import type { PluginInstanceInfo, PluginStatus } from '../../lib/types';
import * as tauri from '../../lib/tauri';
import PluginMetaChips from './PluginMetaChips';
import BuiltinPluginGuiSwitch from './BuiltinPluginGuiSwitch';
import { usePluginRename } from './usePluginRename';
import { usePluginLaunch } from './usePluginLaunch';
import { getPluginStatusPalette } from './pluginStatusPalette';
import { useTranslation } from '../../i18n';

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
  const { t } = useTranslation();
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
      messageApi.success(t('card.resetSuccess'));
    } catch (err) {
      console.debug('PluginCard: resetCrash failed', err);
      messageApi.error(t('card.resetFailed', { error: String(err) }));
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
      messageApi.error(t('card.bypassFailed', { error: String(err) }));
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
      messageApi.error(t('card.removeFailed', { error: String(err) }));
    } finally {
      setIsRemovingBusy(false);
    }
  };

  const [isClosingGui, setIsClosingGui] = useState(false);

  const handleCloseNativeGui = async () => {
    if (isControlLocked || isClosingGui) return;
    try {
      setIsClosingGui(true);
      await tauri.closePlugins([plugin.instance_id]);
      messageApi.info(t('card.guiClosed'));
    } catch (err) {
      messageApi.error(t('card.closeGuiFailed', { error: String(err) }));
    } finally {
      setIsClosingGui(false);
    }
  };

  const isControlLocked = interactionLocked || isLaunching || checkingStatus || isRenamingBusy || isBypassBusy || isRemovingBusy || isClosingGui;
  const effectivePlugin = { ...plugin, gui_open: plugin.gui_open || showBuiltinGui };
  const {
    isCrashed,
    isActive,
    statusKind,
    bypassButtonColor,
    bypassButtonBg,
    bypassButtonBorder,
    color: statusDotColor,
  } = getPluginStatusPalette(effectivePlugin, crashStatus, token);

  const statusText = statusKind === 'crashed'
    ? t('card.crashed')
    : statusKind === 'bypassed'
    ? t('card.bypassed')
    : statusKind === 'live'
    ? t('card.live')
    : t('card.active');

  const effectiveCrashStatus = crashStatus ?? { type: 'Ok' as const };

  // Determine launch button appearance
  const launchButtonProps = (() => {
    if (plugin.format === 'builtin') {
      return {
        icon: <Sliders size={14} />,
        label: t('card.settings'),
        tooltip: showBuiltinGui ? t('card.closeSettingsTooltip') : t('card.openSettingsTooltip'),
        onClick: () => setShowBuiltinGui((prev) => !prev),
      };
    }
    if (plugin.gui_open) {
      return {
        icon: <CheckCircle2 size={14} />,
        label: t('card.open'),
        tooltip: t('card.closeGuiTooltip'),
        onClick: handleCloseNativeGui,
      };
    }
    if (isLaunching) {
      return {
        icon: <Loader2 size={14} className="animate-spin" />,
        label: t('card.launching'),
        tooltip: t('card.launchingTooltip'),
        onClick: () => {},
      };
    }
    return {
      icon: <ExternalLink size={14} />,
      label: t('card.launch'),
      tooltip: t('card.launchTooltip'),
      onClick: handleLaunch,
    };
  })();

  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (cardRef.current) {
      gsap.fromTo(
        cardRef.current,
        { opacity: 0, y: 12, scale: 0.96 },
        { opacity: 1, y: 0, scale: 1, duration: 0.32, ease: 'power2.out' }
      );
    }
  }, []);

  useEffect(() => {
    if (cardRef.current) {
      gsap.to(cardRef.current, {
        opacity: plugin.bypassed ? 0.65 : 1,
        scale: plugin.bypassed ? 0.985 : 1,
        duration: 0.22,
        ease: 'power2.out',
      });
    }
  }, [plugin.bypassed]);

  return (
    <>
    {contextHolder}
    <div ref={cardRef} style={{ width: '100%', height: '100%' }}>
    <Card
      size="small"
      className="rh-plugin-card"
      style={{
        borderRadius: 12,
        background: isCrashed
          ? 'rgba(244, 63, 94, 0.12)'
          : token.colorBgElevated,
        display: 'flex',
        flexDirection: 'column',
        height: 192,
        border: isCrashed
          ? `1px solid rgba(244, 63, 94, 0.35)`
          : isActive
          ? `1px solid ${token.colorPrimary}44`
          : `1px solid ${token.colorBorder}`,
        boxShadow: isActive
          ? `0 6px 20px -2px rgba(0, 0, 0, 0.4), 0 0 16px ${token.colorPrimary}18`
          : '0 4px 16px rgba(0, 0, 0, 0.25)',
        transition: 'border-color 180ms ease, box-shadow 180ms ease',
      }}
      styles={{ body: { padding: '14px 16px', display: 'flex', flexDirection: 'column', height: '100%' } }}
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
                <Space size={4}>
                  <Check
                    size={14}
                    style={{ color: token.colorSuccess, cursor: 'pointer' }}
                    onClick={confirmRename}
                  />
                  <X
                    size={14}
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
                  fontSize: 14.5,
                  lineHeight: 1.25,
                  color: token.colorText,
                  flex: 1,
                  minWidth: 0,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
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
                <Tooltip title={t('card.rename')}>
                  <Pencil
                    size={12}
                    style={{ color: token.colorTextQuaternary, cursor: 'pointer', flexShrink: 0, marginTop: 2 }}
                    onClick={startRenaming}
                  />
                </Tooltip>
                {onDragHandlePointerDown && (
                  <Tooltip title={t('card.dragToReorder')}>
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
                        transition: 'all 160ms ease',
                      }}
                    >
                      <GripVertical size={12} />
                    </span>
                  </Tooltip>
                )}
              </Space>
            </>
          )}
        </div>

        {/* ── Meta chips row ──────────────────────────────────────── */}
        <PluginMetaChips plugin={plugin} />

        {/* ── Dedicated Status Pill row ───────────────────────────── */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6, minHeight: 22 }}>
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 5,
              padding: '2px 8px',
              borderRadius: 999,
              fontSize: 9.5,
              fontWeight: 700,
              letterSpacing: '0.04em',
              textTransform: 'uppercase',
              color: statusDotColor,
              background: bypassButtonBg,
              border: `1px solid ${bypassButtonBorder}`,
              boxShadow: isActive ? `0 0 8px ${statusDotColor}22` : 'none',
              transition: 'all 160ms ease',
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: statusDotColor,
                boxShadow: isActive ? `0 0 6px ${statusDotColor}` : 'none',
              }}
            />
            {statusText}
          </span>
          {plugin.format === 'builtin' && (
            <span style={{ fontSize: 9.5, color: token.colorTextTertiary, fontFamily: 'monospace', letterSpacing: 0.5 }}>
              {t('card.dspCore')}
            </span>
          )}
        </div>

        {/* ── Crash status if crashed ─────────────────────────────── */}
        {isCrashed ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              borderRadius: 6,
              padding: '4px 8px',
              fontSize: 10.5,
              color: token.colorErrorText,
              background: token.colorErrorBg,
              border: `1px solid ${token.colorErrorBorder}`,
            }}
          >
            <AlertTriangle size={14} style={{ flexShrink: 0 }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {effectiveCrashStatus.type === 'Timeout'
                ? t('card.timeout')
                : `${effectiveCrashStatus.type}: ${'data' in effectiveCrashStatus ? effectiveCrashStatus.data : ''}`}
            </span>
          </div>
        ) : null}

        {/* ── Bottom status + actions ────────────────────────────── */}
        <div style={{ display: 'flex', gap: 8, width: '100%', alignItems: 'center', marginTop: 'auto' }}>
          {isCrashed && (
            <Tooltip title={t('card.resetCrash')}>
              <Button
                size="small"
                icon={<RotateCcw size={14} />}
                onClick={handleResetCrash}
                loading={checkingStatus}
                disabled={interactionLocked}
                className="btn-pill btn-reset"
                style={{ flex: 1, height: 32 }}
              >
                {t('common.reset')}
              </Button>
            </Tooltip>
          )}

          {!isCrashed && (
            <Tooltip title={plugin.bypassed ? t('card.enablePlugin') : t('card.bypassPlugin')}>
              <Button
                type="text"
                size="small"
                icon={<Power size={14} strokeWidth={2.2} />}
                onClick={() => { void handleToggleBypassClick(); }}
                loading={isBypassBusy}
                className="btn-pill"
                disabled={isControlLocked}
                aria-label={plugin.bypassed ? t('card.enablePlugin') : t('card.bypassPlugin')}
                style={{
                  minWidth: 36,
                  width: 36,
                  height: 32,
                  justifyContent: 'center',
                  paddingInline: 0,
                  color: plugin.bypassed ? token.colorTextSecondary : bypassButtonColor,
                  background: bypassButtonBg,
                  borderColor: bypassButtonBorder,
                  boxShadow: isActive ? `0 0 10px ${bypassButtonColor}22` : 'none',
                }}
              />
            </Tooltip>
          )}

          {!isCrashed && (
            <Tooltip title={launchButtonProps.tooltip}>
              <Button
                size="small"
                icon={launchButtonProps.icon}
                onClick={launchButtonProps.onClick}
                loading={isLaunching || isClosingGui}
                disabled={isControlLocked}
                className="btn-pill btn-tonal"
                style={{
                  flex: 1,
                  height: 32,
                  color: (plugin.gui_open || showBuiltinGui) ? token.colorSuccess : undefined,
                  borderColor: (plugin.gui_open || showBuiltinGui) ? token.colorSuccessBorder : undefined,
                }}
              >
                {launchButtonProps.label}
              </Button>
            </Tooltip>
          )}

          <Tooltip title={t('card.removeFromChain')}>
            <Button
              type="text"
              size="small"
              icon={<X size={15} strokeWidth={2} />}
              onClick={() => { void handleRemoveClick(); }}
              loading={isRemovingBusy}
              disabled={isControlLocked}
              className="btn-icon"
              style={{ minWidth: 32, width: 32, height: 32 }}
            />
          </Tooltip>
        </div>
      </div>
    </Card>
    </div>

    <BuiltinPluginGuiSwitch
      plugin={plugin}
      open={showBuiltinGui}
      onClose={() => setShowBuiltinGui(false)}
    />
  </>
  );
}

export default memo(PluginCard);
