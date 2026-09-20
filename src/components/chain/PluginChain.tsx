import { useCallback, useEffect, useRef, useState } from 'react';
import { lazy, Suspense } from 'react';
import { Button, Space, Tag, Tooltip, message, theme, Typography } from 'antd';
import { listen } from '@tauri-apps/api/event';
import { AudioWaveform, GripVertical, Mic, Plus, Volume2 } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { usePluginStore } from '../../stores/pluginStore';
import { useAudioStore } from '../../stores/audioStore';
import CurvedArrow from './CurvedArrow';
import PluginCard from './PluginCard';
import ChainToolbar from './ChainToolbar';
import { usePluginDragDrop } from './usePluginDragDrop';
import { useRowBreaks } from './useRowBreaks';
import { isAsioId } from '../audio/audioDeviceDisplay';
import type { MouseEvent as ReactMouseEvent } from 'react';
const { Text } = Typography;
const PluginLibrary = lazy(() => import('../plugin/PluginLibrary'));
import * as tauri from '../../lib/tauri';
import type { PluginChainChangedEvent } from '../../lib/types';
import { useVisibleInterval } from '../../lib/useVisibleInterval';
import { useTranslation } from '../../i18n';

/** Fixed IN/OUT indicator pinned in the toolbar — not a row item, so it
 * never competes with plugin cards for space and stays put regardless of
 * how many plugins there are or whether the chain wraps. */
function EndpointPill({
  variant,
  deviceName,
  channelLabel,
  secondaryLabel,
  active,
  onClick,
}: {
  variant: 'in' | 'out';
  deviceName: string;
  channelLabel?: string;
  /** Monitor/virtual output device — 'out' only, shown in the tooltip. */
  secondaryLabel?: string;
  active: boolean;
  onClick: () => void;
}) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const isIn = variant === 'in';
  const Icon = isIn ? Mic : Volume2;
  const accent = isIn ? token.colorSuccess : token.colorPrimary;
  const tooltipTitle = secondaryLabel
    ? <span>{t('chain.changeDeviceTooltip')} <br />{t('chain.monitorOutputTooltip', { name: secondaryLabel })}</span>
    : t('chain.changeDeviceTooltip');

  return (
    <Tooltip title={tooltipTitle}>
      <button
        type="button"
        onClick={onClick}
        style={{
          all: 'unset',
          display: 'inline-flex',
          alignItems: 'center',
          gap: 6,
          padding: '4px 10px',
          borderRadius: 999,
          cursor: 'pointer',
          maxWidth: 220,
          background: active ? `${accent}14` : token.colorFillQuaternary,
          border: `1px solid ${active ? `${accent}40` : token.colorBorderSecondary}`,
        }}
      >
        <Icon size={12} style={{ color: accent, flexShrink: 0 }} />
        <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.6, color: active ? accent : token.colorTextTertiary, flexShrink: 0 }}>
          {isIn ? t('chain.inBadge') : t('chain.outBadge')}
        </span>
        <span
          style={{
            fontSize: 11,
            fontWeight: 500,
            color: token.colorTextSecondary,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            minWidth: 0,
          }}
        >
          {deviceName}
        </span>
        {channelLabel && (
          <Tag
            style={{
              margin: 0,
              flexShrink: 0,
              borderRadius: 999,
              fontSize: 9,
              fontWeight: 600,
              padding: '0 6px',
              lineHeight: '16px',
              background: 'transparent',
              border: `1px solid ${token.colorBorderSecondary}`,
              color: token.colorTextTertiary,
            }}
          >
            {channelLabel}
          </Tag>
        )}
      </button>
    </Tooltip>
  );
}

export default function PluginChain() {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const [messageApi, contextHolder] = message.useMessage();
  // Selector-scoped subscriptions — only re-render when these specific fields
  // change, instead of on every store update (e.g. mutationCount/isScanning).
  const {
    pluginChain,
    crashStatusByInstanceId,
    removeFromChain,
    toggleBypass,
    reorderChain,
    swapChain,
    fetchChain,
    fetchCrashStatuses,
    isChainInitializing,
  } = usePluginStore(useShallow((s) => ({
    pluginChain: s.pluginChain,
    crashStatusByInstanceId: s.crashStatusByInstanceId,
    removeFromChain: s.removeFromChain,
    toggleBypass: s.toggleBypass,
    reorderChain: s.reorderChain,
    swapChain: s.swapChain,
    fetchChain: s.fetchChain,
    fetchCrashStatuses: s.fetchCrashStatuses,
    isChainInitializing: s.isChainInitializing,
  })));
  const {
    devices,
    selectedInputDevice,
    selectedDevice,
    selectedVirtualOutputDevice,
    isMonitoring,
    inputChannelOffset,
    outputChannelOffset,
  } = useAudioStore(useShallow((s) => ({
    devices: s.devices,
    selectedInputDevice: s.selectedInputDevice,
    selectedDevice: s.selectedDevice,
    selectedVirtualOutputDevice: s.selectedVirtualOutputDevice,
    isMonitoring: s.status.is_monitoring,
    inputChannelOffset: s.inputChannelOffset,
    outputChannelOffset: s.outputChannelOffset,
  })));
  const [showPluginLibrary, setShowPluginLibrary] = useState(false);
  const [isDeleteAllBusy, setIsDeleteAllBusy] = useState(false);
  const addLocked = isChainInitializing || isDeleteAllBusy;

  const {
    draggedIndex,
    swapTargetIndex,
    dragPointer,
    dragLabel,
    draggingRef,
    startPointerDrag,
    showInsertAt,
  } = usePluginDragDrop({
    pluginChain,
    isChainInitializing,
    isDeleteAllBusy,
    reorderChain,
    swapChain,
    messageApi,
  });

  const getDeviceName = (deviceId: string | null) => {
    if (!deviceId) return 'None';
    return devices.find((device) => device.id === deviceId)?.name ?? deviceId;
  };

  const inputDeviceName = getDeviceName(selectedInputDevice);
  const outputDeviceName = getDeviceName(selectedDevice);
  const virtualOutputDeviceName = getDeviceName(selectedVirtualOutputDevice);

  // ASIO is full-duplex: the "in" and "out" endpoint nodes are really the
  // same physical device, just its input side and its output side — unlike
  // WASAPI where they're genuinely two separate devices.
  const isAsioMode = isAsioId(selectedInputDevice) || isAsioId(selectedDevice);
  const formatChannelLabel = (offset: number) => `CH ${offset + 1}-${offset + 2}`;
  // Endpoint cards open the same Audio Settings modal as the header's gear
  // icon, via a plain DOM event — no need to lift modal state up when a
  // browser-native event bus already does the job.
  const openAudioSettings = () => window.dispatchEvent(new CustomEvent('rh:open-audio-settings'));

  const rowRef = useRef<HTMLDivElement>(null);
  const wrapBreaks = useRowBreaks(rowRef, [pluginChain.map((p) => p.instance_id).join(','), isChainInitializing]);

  useEffect(() => {
    const unlistenPromise = listen<PluginChainChangedEvent>('plugin-chain-changed', (event) => {
      if (draggingRef.current) return;
      const reason = event.payload?.reason;
      if (reason === 'parameter' || reason === 'parameter_update' || reason?.startsWith('parameter')) {
        return;
      }
      fetchChain();
      fetchCrashStatuses();
    });

    fetchChain();
    fetchCrashStatuses();

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [fetchChain, fetchCrashStatuses, draggingRef]);

  useVisibleInterval(() => {
    fetchCrashStatuses();
  }, 10000, pluginChain.length > 0, [pluginChain.length, fetchCrashStatuses]);

  // Stable callback identities (keyed by instanceId passed at call time) so
  // PluginCard's memo() isn't defeated by a fresh closure every render.
  const handleRemovePlugin = useCallback(async (instanceId: string) => {
    await removeFromChain(instanceId);
  }, [removeFromChain]);

  const handleToggleBypassPlugin = useCallback(async (instanceId: string) => {
    await toggleBypass(instanceId);
  }, [toggleBypass]);

  const handleLaunchPlugin = useCallback(async (instanceId: string) => {
    try {
      await tauri.launchPlugin(instanceId);
    } catch {
      messageApi.error(t('chain.launchFailed'));
    }
  }, [messageApi, t]);

  const handleContextMenu = (e: ReactMouseEvent) => {
    e.preventDefault();
    if (addLocked) return;
    setShowPluginLibrary(true);
  };

  const handleDeleteAll = async () => {
    if (isDeleteAllBusy || isChainInitializing) return;
    console.debug('PluginChain: Delete All clicked', { pluginCount: pluginChain.length });
    try {
      setIsDeleteAllBusy(true);
      // Remove sequentially to avoid overwhelming backend/mutations
      for (const p of [...pluginChain]) {
        // eslint-disable-next-line no-await-in-loop
        console.debug('PluginChain: removing', { instanceId: p.instance_id, name: p.name });
        await removeFromChain(p.instance_id);
      }
      messageApi.success(t('chain.removeAllSuccess'));
    } catch (err) {
      console.debug('PluginChain: deleteAll error', err);
      console.error(err);
      messageApi.error(t('chain.removeAllFailed'));
    } finally {
      setIsDeleteAllBusy(false);
    }
  };

  return (
    <div
      className="signal-chain-panel glass-panel h-full flex flex-col min-h-0"
      style={{
        borderRadius: 16,
        border: '1px solid var(--rh-surface-soft-border-strong)',
        background: 'var(--rh-surface-soft-gradient)',
        boxShadow: 'var(--rh-chain-panel-shadow)',
        overflow: 'hidden',
      }}
    >
      {contextHolder}

      {/* Integrated Rack Header / Toolbar */}
      <ChainToolbar
        isChainInitializing={isChainInitializing}
        addLocked={addLocked}
        isDeleteAllBusy={isDeleteAllBusy}
        pluginChainLength={pluginChain.length}
        onAddPlugin={() => setShowPluginLibrary(true)}
        onDeleteAll={handleDeleteAll}
        inSlot={
          <EndpointPill
            variant="in"
            deviceName={inputDeviceName}
            channelLabel={isAsioMode ? formatChannelLabel(inputChannelOffset) : undefined}
            active={isMonitoring}
            onClick={openAudioSettings}
          />
        }
        outSlot={
          <EndpointPill
            variant="out"
            deviceName={outputDeviceName}
            channelLabel={isAsioMode ? formatChannelLabel(outputChannelOffset) : undefined}
            secondaryLabel={selectedVirtualOutputDevice ? virtualOutputDeviceName : undefined}
            active={isMonitoring}
            onClick={openAudioSettings}
          />
        }
      />

      {/* Signal Chain Rack Canvas */}
      <div
        className="signal-chain-container flex-1 min-h-0 overflow-auto p-4 md:p-6"
        onContextMenu={handleContextMenu}
        style={{
          position: 'relative',
        }}
      >
        {pluginChain.length > 0 ? (
          <>
            <div
              ref={rowRef}
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                alignItems: 'center',
                justifyContent: 'flex-start',
                alignContent: 'flex-start',
                columnGap: 0,
                rowGap: 20,
                minHeight: 158,
              }}
            >
              {/* Plugin cards with drop zones between them — no leading IN
                  card/arrow: the first plugin implicitly receives from IN. */}
              {pluginChain.map((plugin, index) => {
                const isSwapTarget = draggedIndex !== null && swapTargetIndex === index && draggedIndex !== index;
                const isWrapBreak = wrapBreaks.has(index);
                return (
                  <div
                    key={plugin.instance_id}
                    data-plugincombinedpos={index}
                    style={{ display: 'flex', alignItems: 'center' }}
                  >
                    {/* Arrow separator — rotates into a down-turn connector
                        when this plugin starts a new wrapped row, instead of
                        reading as a stray horizontal arrow at the row edge.
                        Before the first plugin the connecting line itself is
                        hidden (nothing to connect from — IN is no longer a
                        row item), but the drop-zone stays so a plugin can
                        still be inserted at the very start by dragging here. */}
                    <div
                      data-plugin-arrow
                      data-plugin-arrow-pos={index}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        margin: isWrapBreak ? '0 6px 0 0' : '0 6px',
                        flexShrink: 0,
                        padding: '4px 6px',
                        borderRadius: 999,
                        minWidth: index === 0 ? 20 : undefined,
                        transform: showInsertAt(index) ? 'translateY(-2px) scale(1.12)' : 'none',
                        background: showInsertAt(index) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                        boxShadow: 'none',
                        transition: 'transform 140ms cubic-bezier(0.4, 0, 0.2, 1), background 140ms ease',
                      }}
                    >
                      {index > 0 && (
                        <span style={{ display: 'block', transform: isWrapBreak ? 'rotate(90deg)' : 'none' }}>
                          <CurvedArrow
                            color={showInsertAt(index) ? token.colorPrimary : undefined}
                            active={isMonitoring && !plugin.bypassed}
                          />
                        </span>
                      )}
                    </div>

                    {/* Card wrapper — drop target */}
                    <div
                      data-plugin-card-index={index}
                      style={{
                        position: 'relative',
                        width: 216,
                        height: 158,
                        display: 'flex',
                        flexDirection: 'column',
                        flexShrink: 0,
                        opacity: draggedIndex === index ? 0.25 : 1,
                        transform: draggedIndex === index
                          ? 'translateY(-2px) scale(1.01)'
                          : isSwapTarget
                          ? 'translateY(-8px) scale(1.02)'
                          : 'none',
                        transition: 'opacity 0.15s ease, transform 0.16s cubic-bezier(0.4, 0, 0.2, 1)',
                        borderRadius: 12,
                        zIndex: isSwapTarget ? 2 : 1,
                      }}
                    >
                      <PluginCard
                        plugin={plugin}
                        crashStatus={crashStatusByInstanceId[plugin.instance_id]}
                        interactionLocked={isChainInitializing || isDeleteAllBusy || draggedIndex !== null}
                        onRemove={handleRemovePlugin}
                        onToggleBypass={handleToggleBypassPlugin}
                        onCrashStatusChanged={fetchCrashStatuses}
                        onDragHandlePointerDown={(e) => startPointerDrag(e, index)}
                        isDragging={draggedIndex === index}
                        onLaunch={handleLaunchPlugin}
                      />
                    </div>
                  </div>
                );
              })}

              {/* Drop-zone after the last card — lets a plugin be appended
                  at the end by dragging here. No visible connector: the last
                  plugin implicitly sends to OUT, which is no longer a row item. */}
              <div
                data-plugin-arrow
                data-plugin-arrow-pos={pluginChain.length}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  margin: '0 6px',
                  flexShrink: 0,
                  padding: '4px 6px',
                  minWidth: 20,
                  borderRadius: 999,
                  transform: showInsertAt(pluginChain.length) ? 'translateY(-2px) scale(1.12)' : 'none',
                  background: showInsertAt(pluginChain.length) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                  boxShadow: 'none',
                  transition: 'transform 140ms cubic-bezier(0.4, 0, 0.2, 1), background 140ms ease',
                }}
              />
            </div>

            {dragPointer && (
              <div
                className="rh-floating-tip"
                style={{
                  left: dragPointer.x,
                  top: dragPointer.y,
                }}
              >
                <Space size={6}>
                  <GripVertical size={14} />
                  <span>{dragLabel}</span>
                </Space>
              </div>
            )}
          </>
        ) : (
          <div
            style={{
              height: '100%',
              minHeight: 280,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              textAlign: 'center',
              padding: '32px 16px',
              userSelect: 'none',
            }}
          >
            <div
              style={{
                width: 68,
                height: 68,
                borderRadius: 20,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                background: `${token.colorPrimary}14`,
                border: `1px solid ${token.colorPrimary}30`,
                marginBottom: 16,
                boxShadow: `0 8px 24px ${token.colorPrimary}12`,
              }}
            >
              <AudioWaveform size={34} strokeWidth={1.8} style={{ color: token.colorPrimary }} />
            </div>
            <Text strong style={{ fontSize: 16, color: token.colorText, marginBottom: 4 }}>
              {t('chain.noPlugins')}
            </Text>
            <Text type="secondary" style={{ fontSize: 13, maxWidth: 380, lineHeight: 1.5, marginBottom: 18, color: token.colorTextTertiary }}>
              {t('chain.noPluginsDesc')}
            </Text>
            <Button
              type="primary"
              size="middle"
              icon={<Plus size={15} strokeWidth={2.5} />}
              className="btn-pill"
              onClick={() => setShowPluginLibrary(true)}
              disabled={addLocked}
              style={{ padding: '0 20px', height: 38 }}
            >
              {t('chain.addPlugin')}
            </Button>
            <Text type="secondary" style={{ fontSize: 11, marginTop: 12, color: token.colorTextQuaternary }}>
              {t('chain.reorderTip')}
            </Text>
          </div>
        )}
      </div>

      {showPluginLibrary && (
        <Suspense fallback={null}>
          <PluginLibrary
            isOpen={showPluginLibrary}
            onClose={() => setShowPluginLibrary(false)}
          />
        </Suspense>
      )}
    </div>
  );
}
