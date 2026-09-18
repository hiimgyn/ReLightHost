import { useCallback, useEffect, useState } from 'react';
import { lazy, Suspense } from 'react';
import { Card, Empty, Space, message, theme, Typography } from 'antd';
import { listen } from '@tauri-apps/api/event';
import { AudioOutlined, HolderOutlined, SwapOutlined } from '@ant-design/icons';
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

export default function PluginChain() {
  const { token } = theme.useToken();
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
      messageApi.error('Failed to launch plugin');
    }
  }, [messageApi]);

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
      messageApi.success('Removed all plugins from chain');
    } catch (err) {
      console.debug('PluginChain: deleteAll error', err);
      console.error(err);
      messageApi.error('Failed to remove all plugins');
    } finally {
      setIsDeleteAllBusy(false);
    }
  };

  return (
    <div className="h-full flex flex-col min-h-0 signal-chain-container">
      {contextHolder}
      <ChainToolbar
        isChainInitializing={isChainInitializing}
        addLocked={addLocked}
        isDeleteAllBusy={isDeleteAllBusy}
        pluginChainLength={pluginChain.length}
        onAddPlugin={() => setShowPluginLibrary(true)}
        onDeleteAll={handleDeleteAll}
      />

      {/* Plugin Chain Area */}
      <Card
        className="glass-card"
        style={{
          flex: 1,
          minHeight: 0,
          background: 'var(--rh-surface-soft-gradient)',
          borderRadius: `0 0 ${token.borderRadiusLG * 1.25}px ${token.borderRadiusLG * 1.25}px`,
          border: '1px solid var(--rh-surface-soft-border-strong)',
          borderTop: 'none',
          boxShadow: 'var(--rh-chain-panel-shadow)',
        }}
        styles={{ body: { height: '100%', padding: '20px', overflow: 'hidden' } }}
        onContextMenu={handleContextMenu}
      >
        {pluginChain.length > 0 ? (
          <>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                marginBottom: 12,
                color: token.colorTextTertiary,
                fontSize: 11,
                fontWeight: 500,
              }}
            >
              <SwapOutlined style={{ color: token.colorPrimary, opacity: 0.85 }} />
              <span>Use the handle inside each card to reorder</span>
            </div>

            <div
              style={{
                height: 'calc(100% - 15px)',
                border: '1px dashed var(--rh-chain-well-border)',
                borderRadius: 10,
                padding: 12,
                overflowX: 'hidden',
                overflowY: 'auto',
                background: 'var(--rh-chain-well-bg)',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  flexWrap: 'wrap',
                  alignItems: 'center',
                  justifyContent: 'flex-start',
                  alignContent: 'flex-start',
                  columnGap: 0,
                  rowGap: 30,
                  minHeight: 138,
                }}
              >

            <ChainEndpointCard variant="in" tooltipTitle={inputDeviceName} active={isMonitoring} />

            {/* Plugin cards with drop zones between theme*/}
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
                    padding: '3px 4px',
                    borderRadius: 999,
                    transform: showInsertAt(index) ? 'translateY(-2px) scale(1.08)' : 'none',
                    background: showInsertAt(index) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                    boxShadow: 'none',
                    transition: 'transform 120ms ease, background 120ms ease',
                  }}
                >
                  <CurvedArrow
                    color={showInsertAt(index) ? token.colorPrimary : undefined}
                    active={isMonitoring && !plugin.bypassed}
                  />
                </div>

                {/* Card wrapper — full drop target */}
                <div
                  data-plugin-card-index={index}
                  style={{
                    position: 'relative',
                    width: 252,
                    height: 220,
                    display: 'flex',
                    flexDirection: 'column',
                    flexShrink: 0,
                    opacity: draggedIndex === index ? 0.2 : 1,
                    transform: draggedIndex === index
                      ? 'translateY(-2px) scale(1.01)'
                      : isSwapTarget
                      ? 'translateY(-8px) scale(1.018)'
                      : 'none',
                    transition: 'opacity 0.15s ease, transform 0.16s ease',
                    borderRadius: 8,
                    filter: 'none',
                    zIndex: isSwapTarget ? 2 : 1,
                  }}
                >
                  <div style={{ paddingTop: 12 }}>
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
                padding: '3px 4px',
                borderRadius: 999,
                transform: showInsertAt(pluginChain.length) ? 'translateY(-2px) scale(1.08)' : 'none',
                background: showInsertAt(pluginChain.length) ? 'var(--rh-chain-insert-bg)' : 'transparent',
                boxShadow: 'none',
                transition: 'transform 120ms ease, background 120ms ease',
              }}
            >
              <CurvedArrow
                color={showInsertAt(pluginChain.length) ? token.colorPrimary : undefined}
                active={isMonitoring}
              />
            </div>

            <ChainEndpointCard
              variant="out"
              tooltipTitle={<span>Output: {outputDeviceName} <br />Virtual Output: {virtualOutputDeviceName}</span>}
              active={isMonitoring}
            />

              </div>
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
                  <HolderOutlined />
                  <span>{dragLabel}</span>
                </Space>
              </div>
            )}
          </>
        ) : (
          <Empty
            image={<AudioOutlined style={{ fontSize: 64, color: token.colorTextQuaternary }} />}
            description={
              <Space orientation="vertical" size={0}>
                <Text style={{ fontSize: 16, color: token.colorTextSecondary }}>No plugins loaded</Text>
                <Text style={{ fontSize: 13, color: token.colorTextTertiary }}>
                  Right-click or use the "Add Plugin" button to add plugins
                </Text>
                <Text style={{ fontSize: 12, color: token.colorTextQuaternary, marginTop: 4 }}>
                  Tip: drag the handle inside each card to reorder
                </Text>
              </Space>
            }
          />
        )}
      </Card>

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
