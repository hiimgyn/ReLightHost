import { useCallback, useEffect, useState } from 'react';
import { lazy, Suspense } from 'react';
import { Button, Space, message, theme, Typography } from 'antd';
import { listen } from '@tauri-apps/api/event';
import { AudioWaveform, GripVertical, Plus } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { usePluginStore } from '../../stores/pluginStore';
import { useAudioStore } from '../../stores/audioStore';
import CurvedArrow from './CurvedArrow';
import PluginCard from './PluginCard';
import ChainEndpointCard from './ChainEndpointCard';
import ChainToolbar from './ChainToolbar';
import { usePluginDragDrop } from './usePluginDragDrop';
import type { MouseEvent as ReactMouseEvent } from 'react';
const { Text } = Typography;
const PluginLibrary = lazy(() => import('../plugin/PluginLibrary'));
import * as tauri from '../../lib/tauri';
import type { PluginChainChangedEvent } from '../../lib/types';
import { useVisibleInterval } from '../../lib/useVisibleInterval';
import { useTranslation } from '../../i18n';

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
  } = useAudioStore(useShallow((s) => ({
    devices: s.devices,
    selectedInputDevice: s.selectedInputDevice,
    selectedDevice: s.selectedDevice,
    selectedVirtualOutputDevice: s.selectedVirtualOutputDevice,
    isMonitoring: s.status.is_monitoring,
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
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                alignItems: 'center',
                justifyContent: 'flex-start',
                alignContent: 'flex-start',
                columnGap: 0,
                rowGap: 24,
                minHeight: 192,
              }}
            >
              <ChainEndpointCard variant="in" tooltipTitle={inputDeviceName} active={isMonitoring} />

              {/* Plugin cards with drop zones between them */}
              {pluginChain.map((plugin, index) => {
                const isSwapTarget = draggedIndex !== null && swapTargetIndex === index && draggedIndex !== index;
                return (
                  <div key={plugin.instance_id} style={{ display: 'flex', alignItems: 'center' }}>
                    {/* Arrow separator (curved/dashed) */}
                    <div
                      data-plugin-arrow
                      data-plugin-arrow-pos={index}
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        margin: '0 6px',
                        flexShrink: 0,
                        padding: '4px 6px',
                        borderRadius: 999,
                        transform: showInsertAt(index) ? 'translateY(-2px) scale(1.12)' : 'none',
                        background: showInsertAt(index) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                        boxShadow: 'none',
                        transition: 'transform 140ms cubic-bezier(0.4, 0, 0.2, 1), background 140ms ease',
                      }}
                    >
                      <CurvedArrow
                        color={showInsertAt(index) ? token.colorPrimary : undefined}
                        active={isMonitoring && !plugin.bypassed}
                      />
                    </div>

                    {/* Card wrapper — drop target */}
                    <div
                      data-plugin-card-index={index}
                      style={{
                        position: 'relative',
                        width: 268,
                        height: 192,
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

              {/* Arrow after last card */}
              <div
                data-plugin-arrow
                data-plugin-arrow-pos={pluginChain.length}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  margin: '0 6px',
                  flexShrink: 0,
                  padding: '4px 6px',
                  borderRadius: 999,
                  transform: showInsertAt(pluginChain.length) ? 'translateY(-2px) scale(1.12)' : 'none',
                  background: showInsertAt(pluginChain.length) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                  boxShadow: 'none',
                  transition: 'transform 140ms cubic-bezier(0.4, 0, 0.2, 1), background 140ms ease',
                }}
              >
                <CurvedArrow
                  color={showInsertAt(pluginChain.length) ? token.colorPrimary : undefined}
                  active={isMonitoring}
                />
              </div>

              <ChainEndpointCard
                variant="out"
                tooltipTitle={<span>{t('chain.outputTooltipOutput', { name: outputDeviceName })} <br />{t('chain.outputTooltipVirtual', { name: virtualOutputDeviceName })}</span>}
                active={isMonitoring}
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
