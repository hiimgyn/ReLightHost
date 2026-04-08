import { useEffect, useRef, useState } from 'react';
import { lazy, Suspense } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';
import { listen } from '@tauri-apps/api/event';

const { Text } = Typography;
import { 
  PlusOutlined, 
  AudioOutlined,
  HolderOutlined,
  SwapOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import { Popconfirm } from 'antd';
import CurvedArrow from './CurvedArrow';
import { usePluginStore } from '../../stores/pluginStore';
import { useAudioStore } from '../../stores/audioStore';
import PluginCard from './PluginCard';
const PluginLibrary = lazy(() => import('../plugin/PluginLibrary'));
import * as tauri from '../../lib/tauri';
import type { PluginChainChangedEvent } from '../../lib/types';

export default function PluginChain() {
  const { token } = theme.useToken();
  const [messageApi, contextHolder] = message.useMessage();
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
  } = usePluginStore();
  const {
    devices,
    selectedInputDevice,
    selectedDevice,
    selectedVirtualOutputDevice,
  } = useAudioStore();
  const [showPluginLibrary, setShowPluginLibrary] = useState(false);
  const [isDeleteAllBusy, setIsDeleteAllBusy] = useState(false);
  const addLocked = isChainInitializing || isDeleteAllBusy;

  // draggedIndex: which card is being dragged
  // insertBefore: the index BEFORE which the dragged card will be inserted
  //               (0 = before first, pluginChain.length = after last)
  // draggedIndex: which card is being dragged
  // insertBefore: index BEFORE which the dragged card will be inserted
  //               (0 = before first,  pluginChain.length = after last)
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [insertBefore, setInsertBefore] = useState<number | null>(null);
  const [swapTargetIndex, setSwapTargetIndex] = useState<number | null>(null);
  const [dragPointer, setDragPointer] = useState<{ x: number; y: number } | null>(null);
  const [dragLabel, setDragLabel] = useState('');
  const draggingRef = useRef(false);
  const draggedIndexRef = useRef<number | null>(null);
  const insertBeforeRef = useRef<number | null>(null);
  const swapTargetIndexRef = useRef<number | null>(null);

  const getDeviceName = (deviceId: string | null) => {
    if (!deviceId) return 'None';
    return devices.find((device) => device.id === deviceId)?.name ?? deviceId;
  };

  const inputDeviceName = getDeviceName(selectedInputDevice);
  const outputDeviceName = getDeviceName(selectedDevice);
  const virtualOutputDeviceName = getDeviceName(selectedVirtualOutputDevice);

  useEffect(() => {
    insertBeforeRef.current = insertBefore;
  }, [insertBefore]);

  useEffect(() => {
    const syncAll = () => {
      fetchChain();
      fetchCrashStatuses();
    };

    syncAll();

    const crashInterval = setInterval(() => {
      if (document.visibilityState === 'visible') {
        fetchCrashStatuses();
      }
    }, 5000);

    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        syncAll();
      }
    };

    const unlistenPromise = listen<PluginChainChangedEvent>('plugin-chain-changed', () => {
      if (draggingRef.current) return;
      syncAll();
    });

    document.addEventListener('visibilitychange', onVisible);

    return () => {
      clearInterval(crashInterval);
      document.removeEventListener('visibilitychange', onVisible);
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [fetchChain, fetchCrashStatuses]);

  const handleContextMenu = (e: React.MouseEvent) => {
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

  // Pointer-based drag session (more reliable than HTML5 DnD inside Tauri WebView).
  const startPointerDrag = (e: React.PointerEvent, index: number) => {
    if (isChainInitializing || isDeleteAllBusy) return;
    if (e.button !== 0) return;
    e.preventDefault();

    draggingRef.current = true;
    draggedIndexRef.current = index;
    setDragLabel(pluginChain[index]?.name ?? 'Plugin');
    setDragPointer({ x: e.clientX, y: e.clientY });
    setDraggedIndex(index);
    setInsertBefore(null);
    setSwapTargetIndex(null);
    swapTargetIndexRef.current = null;

    const onPointerMove = (ev: PointerEvent) => {
      if (draggedIndexRef.current === null) return;
      setDragPointer({ x: ev.clientX, y: ev.clientY });

      const el = document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null;
      
      // Check if hovering over arrow element
      const arrowEl = el?.closest('[data-plugin-arrow]') as HTMLElement | null;
      if (arrowEl) {
        const arrowPosRaw = arrowEl.dataset.pluginArrowPos;
        if (arrowPosRaw != null) {
          const pos = Number(arrowPosRaw);
          if (Number.isFinite(pos) && insertBeforeRef.current !== pos) {
            insertBeforeRef.current = pos;
            setInsertBefore(pos);
          }
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const cardEl = el?.closest('[data-plugin-card-index]') as HTMLElement | null;
      if (!cardEl) {
        if (insertBeforeRef.current !== null) {
          insertBeforeRef.current = null;
          setInsertBefore(null);
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const indexRaw = cardEl.dataset.pluginCardIndex;
      if (indexRaw == null) return;
      const cardIndex = Number(indexRaw);
      if (!Number.isFinite(cardIndex)) return;

      const rect = cardEl.getBoundingClientRect();
      
      // Check if pointer is within reasonable Y range of the card (with tolerance for multi-row layouts)
      const tolerance = 60;
      const isWithinVerticalBounds = 
        ev.clientY >= rect.top - tolerance && 
        ev.clientY <= rect.bottom + tolerance;
      
      if (!isWithinVerticalBounds) {
        if (insertBeforeRef.current !== null) {
          insertBeforeRef.current = null;
          setInsertBefore(null);
        }
        if (swapTargetIndexRef.current !== null) {
          swapTargetIndexRef.current = null;
          setSwapTargetIndex(null);
        }
        return;
      }

      const from = draggedIndexRef.current;
      if (from === null) return;

      // Hovering a card means swap target. Hovering an arrow means insert target.
      if (insertBeforeRef.current !== null) {
        insertBeforeRef.current = null;
        setInsertBefore(null);
      }
      const nextSwapTarget = cardIndex === from ? null : cardIndex;
      if (swapTargetIndexRef.current !== nextSwapTarget) {
        swapTargetIndexRef.current = nextSwapTarget;
        setSwapTargetIndex(nextSwapTarget);
      }
    };

    const stopPointerDrag = async () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);

      const from = draggedIndexRef.current;
      const pos = insertBeforeRef.current;
      const swapTo = swapTargetIndexRef.current;

      draggingRef.current = false;
      draggedIndexRef.current = null;
      insertBeforeRef.current = null;
      swapTargetIndexRef.current = null;
      setDragPointer(null);
      setDragLabel('');
      setDraggedIndex(null);
      setInsertBefore(null);
      setSwapTargetIndex(null);

      if (from === null) return;

      try {
        if (swapTo !== null && swapTo !== from) {
          await swapChain(from, swapTo);
          messageApi.success('Plugins swapped');
          return;
        }

        if (pos === null) return;
        const to = pos > from ? pos - 1 : pos;
        if (from === to) return;

        await reorderChain(from, to);
        messageApi.success('Plugin order updated');
      } catch (error) {
        messageApi.error('Failed to reorder plugins');
        console.error(error);
      }
    };

    const onPointerUp = () => { void stopPointerDrag(); };
    const onPointerCancel = () => { void stopPointerDrag(); };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
  };

  //  Visual indicator helpers 
  const showInsertAt = (pos: number) =>
    draggedIndex !== null &&
    insertBefore === pos &&
    insertBefore !== draggedIndex &&
    insertBefore !== draggedIndex + 1;

  return (
    <div className="h-full flex flex-col min-h-0 signal-chain-container">
      {contextHolder}
      {/* Toolbar */}
      <div
        className="glass-panel"
        style={{
          marginBottom: 0,
          padding: '14px 18px',
          borderRadius: `${token.borderRadiusLG * 1.25}px ${token.borderRadiusLG * 1.25}px 0 0`,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 16,
          flexWrap: 'wrap',
          background: token.colorBgElevated.includes('rgb') 
            ? (token.colorBgElevated.includes('255')
              ? 'linear-gradient(135deg, rgba(255,255,255,0.22) 0%, rgba(255,255,255,0.16) 100%)'
              : 'linear-gradient(135deg, rgba(58,64,96,0.24) 0%, rgba(45,50,78,0.18) 100%)')
            : token.colorBgElevated,
          border: `1px solid ${token.colorBorderSecondary.includes('255') ? 'rgba(255,255,255,0.34)' : 'rgba(210,216,255,0.24)'}`,
          borderBottom: 'none',
          backdropFilter: 'blur(20px) saturate(1.1)',
          WebkitBackdropFilter: 'blur(20px) saturate(1.1)',
          boxShadow: token.colorBorderSecondary.includes('255')
            ? '0 8px 24px rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.05)'
            : '0 8px 20px rgba(15,23,42,0.06), inset 0 1px 0 rgba(255,255,255,0.16)',
        }}
      >
        <Space orientation="vertical" size={2}>
          <Text
            strong
            style={{
              fontSize: 20,
              fontWeight: 700,
              letterSpacing: '-0.02em',
              color: token.colorText,
              margin: 0,
              display: 'block',
            }}
          >
            Signal chain
          </Text>
          <Text style={{ fontSize: 12, color: token.colorTextTertiary, margin: 0 }}>
            Drag cards to reorder · right-click empty area to add
          </Text>
        </Space>

        <Space size="middle" wrap>
          <Tooltip title={isChainInitializing ? 'Preparing…' : 'Add plugin'}>
            <Button
              type="primary"
              icon={<PlusOutlined />}
              size="middle"
              className="btn-pill"
              loading={addLocked}
              disabled={addLocked}
              onClick={() => setShowPluginLibrary(true)}
              aria-label={isChainInitializing ? 'Preparing plugin library' : 'Add plugin'}
            >
              Add Plugin
            </Button>
          </Tooltip>

          <Popconfirm
            title="Remove all plugins from the chain?"
            onConfirm={handleDeleteAll}
            okText="Remove"
            cancelText="Cancel"
          >
            <Button
              size="middle"
              type="default"
              icon={<DeleteOutlined />}
              loading={isDeleteAllBusy}
              disabled={pluginChain.length === 0 || isDeleteAllBusy || isChainInitializing}
              className="btn-pill btn-tonal"
              style={{
                borderColor: 'rgba(99,103,255,0.2)',
                color: token.colorTextSecondary,
              }}
            >
            </Button>
          </Popconfirm>
        </Space>
      </div>

      {/* Plugin Chain Area */}
      <Card
        className="glass-card"
        style={{
          flex: 1,
          minHeight: 0,
          background: token.colorBgElevated.includes('rgb')
            ? (token.colorBgElevated.includes('255')
              ? 'linear-gradient(135deg, rgba(255,255,255,0.22) 0%, rgba(255,255,255,0.16) 100%)'
              : 'linear-gradient(135deg, rgba(58,64,96,0.24) 0%, rgba(45,50,78,0.18) 100%)')
            : token.colorBgElevated,
          borderRadius: `0 0 ${token.borderRadiusLG * 1.25}px ${token.borderRadiusLG * 1.25}px`,
          border: `1px solid ${token.colorBorderSecondary.includes('255') ? 'rgba(255,255,255,0.34)' : 'rgba(210,216,255,0.24)'}`,
          borderTop: 'none',
          backdropFilter: 'blur(20px) saturate(1.1)',
          WebkitBackdropFilter: 'blur(20px) saturate(1.1)',
          boxShadow: token.colorBorderSecondary.includes('255')
            ? '0 8px 32px rgba(0,0,0,0.25), inset 0 1px 0 rgba(255,255,255,0.05)'
            : '0 8px 24px rgba(15,23,42,0.08), inset 0 1px 0 rgba(255,255,255,0.16)',
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
                border: `1px dashed ${token.colorBorderSecondary.includes('255') ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.15)'}`,
                borderRadius: 10,
                padding: 12,
                overflowX: 'hidden',
                overflowY: 'auto',
                background: token.colorBgElevated.includes('rgb')
                  ? (token.colorBgElevated.includes('255')
                    ? 'linear-gradient(135deg, rgba(255,255,255,0.18) 0%, rgba(255,255,255,0.14) 100%)'
                    : 'linear-gradient(135deg, rgba(58,64,96,0.22) 0%, rgba(45,50,78,0.16) 100%)')
                  : 'transparent',
                backdropFilter: 'blur(12px)',
                WebkitBackdropFilter: 'blur(12px)',
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

            {/* IN node */}
            <Tooltip
              title={
                <Space orientation="vertical" size={0} style={{ minWidth: 190 }}>
                  <Text strong style={{ color: token.colorText }}>Input Node</Text>
                  <Text style={{ color: token.colorTextSecondary }}>{inputDeviceName}</Text>
                </Space>
              }
            >
              <div
                style={{
                  position: 'relative',
                  width: 148,
                  flexShrink: 0,
                  borderRadius: 18,
                }}
              >
                <Card
                  className="glass-card"
                  style={{ width: '100%', height: 145, flexShrink: 0, overflow: 'hidden' }}
                  styles={{ body: {
                    position: 'relative',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'stretch',
                    justifyContent: 'space-between',
                    padding: 12,
                    height: '100%',
                  
                    background: `linear-gradient(160deg, ${token.colorSuccessBg} 0%, ${token.colorBgContainer} 55%, ${token.colorFillQuaternary} 100%)`,
                    border: `1px solid ${token.colorSuccessBorder}`,
                    boxShadow: `0 14px 32px rgba(2,6,23,0.42), inset 0 1px 0 rgba(255,255,255,0.06)`,
                  } }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                      <span
                        style={{
                          width: 8,
                          height: 8,
                          borderRadius: 999,
                          background: token.colorSuccess,
                          boxShadow: `0 0 12px ${token.colorSuccess}`,
                        }}
                      />
                      <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.8, color: token.colorTextSecondary }}>
                        INPUT
                      </span>
                    </div>
                    <span
                      style={{
                        padding: '3px 8px',
                        borderRadius: 999,
                        background: token.colorSuccessBgHover,
                        color: token.colorSuccess,
                        fontSize: 10,
                        fontWeight: 700,
                        letterSpacing: 0.4,
                      }}
                    >
                      IN
                    </span>
                  </div>

                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flex: 1 }}>
                    <div
                      style={{
                        width: 64,
                        height: 64,
                        borderRadius: '50%',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        background: `radial-gradient(circle at 35% 35%, ${token.colorSuccessHover} 0%, ${token.colorSuccessBgHover} 45%, ${token.colorBgContainer} 100%)`,
                        border: `1px solid ${token.colorSuccessBorder}`,
                        boxShadow: `0 10px 24px ${token.colorSuccessBg}`,
                      }}
                    >
                      <AudioOutlined style={{ fontSize: 22, color: token.colorSuccess }} />
                    </div>
                  </div>

                </Card>
              </div>
            </Tooltip>

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
                    background: showInsertAt(index)
                      ? (token.colorBgElevated.includes('255')
                        ? 'linear-gradient(135deg, rgba(99,103,255,0.26), rgba(132,148,255,0.22))'
                        : 'linear-gradient(135deg, rgba(99,103,255,0.34), rgba(132,148,255,0.28))')
                      : 'transparent',
                    boxShadow: showInsertAt(index)
                      ? `0 0 0 1px ${token.colorPrimaryBorder}, 0 6px 16px ${token.colorPrimaryBg}`
                      : 'none',
                    transition: 'transform 120ms ease, box-shadow 120ms ease, background 120ms ease',
                  }}
                >
                  <CurvedArrow color={showInsertAt(index) ? token.colorPrimary : token.colorTextQuaternary} />
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
                    transition: 'opacity 0.15s ease, transform 0.16s ease, filter 0.16s ease',
                    borderRadius: 8,
                    filter: isSwapTarget ? 'brightness(1.04) saturate(1.05)' : 'none',
                    zIndex: isSwapTarget ? 2 : 1,
                  }}
                >
                  <div style={{ paddingTop: 12 }}>
                    <PluginCard
                      plugin={plugin}
                      crashStatus={crashStatusByInstanceId[plugin.instance_id]}
                      interactionLocked={isChainInitializing || isDeleteAllBusy || draggedIndex !== null}
                      onRemove={async () => { await removeFromChain(plugin.instance_id); }}
                      onToggleBypass={async () => { await toggleBypass(plugin.instance_id); }}
                      onCrashStatusChanged={fetchCrashStatuses}
                      onDragHandlePointerDown={(e) => startPointerDrag(e, index)}
                      isDragging={draggedIndex === index}
                      onLaunch={async () => {
                        try {
                          await tauri.launchPlugin(plugin.instance_id);
                        } catch {
                          messageApi.error('Failed to launch plugin');
                        }
                      }}
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
                background: showInsertAt(pluginChain.length)
                  ? (token.colorBgElevated.includes('255')
                    ? 'linear-gradient(135deg, rgba(99,103,255,0.26), rgba(132,148,255,0.22))'
                    : 'linear-gradient(135deg, rgba(99,103,255,0.34), rgba(132,148,255,0.28))')
                  : 'transparent',
                boxShadow: showInsertAt(pluginChain.length)
                  ? `0 0 0 1px ${token.colorPrimaryBorder}, 0 6px 16px ${token.colorPrimaryBg}`
                  : 'none',
                transition: 'transform 120ms ease, box-shadow 120ms ease, background 120ms ease',
              }}
            >
              <CurvedArrow color={showInsertAt(pluginChain.length) ? token.colorPrimary : token.colorTextQuaternary} />
            </div>

            {/* OUT node */}
            <Tooltip
              title={
                <Space orientation="vertical" size={0} style={{ minWidth: 210 }}>
                  <Text strong style={{ color: token.colorText }}>Output Node</Text>
                  <Text style={{ color: token.colorTextSecondary }}>{outputDeviceName}</Text>
                  <Text style={{ color: token.colorTextSecondary }}>Virtual output: {virtualOutputDeviceName}</Text>
                </Space>
              }
            >
              <div
                style={{
                  position: 'relative',
                  width: 148,
                  flexShrink: 0,
                  borderRadius: 18,
                }}
              >
                <Card
                  className="glass-card"
                  style={{ width: '100%', height: 145, flexShrink: 0, overflow: 'hidden' }}
                  styles={{ body: {
                    position: 'relative',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'stretch',
                    justifyContent: 'space-between',
                    padding: 12,
                    height: '100%',
                   
                    background: `linear-gradient(160deg, ${token.colorInfoBg} 0%, ${token.colorBgContainer} 55%, ${token.colorFillQuaternary} 100%)`,
                    border: `1px solid ${token.colorInfoBorder}`,
                    boxShadow: `0 14px 32px rgba(2,6,23,0.42), inset 0 1px 0 rgba(255,255,255,0.06)`,
                  } }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                    <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                      <span
                        style={{
                          width: 8,
                          height: 8,
                          borderRadius: 999,
                          background: token.colorInfo,
                          boxShadow: `0 0 12px ${token.colorInfo}`,
                        }}
                      />
                      <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.8, color: token.colorTextSecondary }}>
                        OUTPUT
                      </span>
                    </div>
                    <span
                      style={{
                        padding: '3px 8px',
                        borderRadius: 999,
                        background: token.colorInfoBgHover,
                        color: token.colorInfo,
                        fontSize: 10,
                        fontWeight: 700,
                        letterSpacing: 0.4,
                      }}
                    >
                      OUT
                    </span>
                  </div>

                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flex: 1 }}>
                    <div
                      style={{
                        width: 64,
                        height: 64,
                        borderRadius: '50%',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        background: `radial-gradient(circle at 35% 35%, ${token.colorInfoHover} 0%, ${token.colorInfoBgHover} 45%, ${token.colorBgContainer} 100%)`,
                        border: `1px solid ${token.colorInfoBorder}`,
                        boxShadow: `0 10px 24px ${token.colorInfoBg}`,
                      }}
                    >
                      <AudioOutlined style={{ fontSize: 22, color: token.colorInfo }} />
                    </div>
                  </div>
                </Card>
              </div>
            </Tooltip>

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
