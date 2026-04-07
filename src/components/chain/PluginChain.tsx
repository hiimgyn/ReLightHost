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
    fetchChain,
    fetchCrashStatuses,
    isChainInitializing,
    isMutating,
  } = usePluginStore();
  const {
    devices,
    selectedInputDevice,
    selectedDevice,
    selectedVirtualOutputDevice,
  } = useAudioStore();
  const [showPluginLibrary, setShowPluginLibrary] = useState(false);
  const addLocked = isMutating || isChainInitializing;

  // draggedIndex: which card is being dragged
  // insertBefore: the index BEFORE which the dragged card will be inserted
  //               (0 = before first, pluginChain.length = after last)
  // draggedIndex: which card is being dragged
  // insertBefore: index BEFORE which the dragged card will be inserted
  //               (0 = before first,  pluginChain.length = after last)
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [insertBefore, setInsertBefore] = useState<number | null>(null);
  const [dragPointer, setDragPointer] = useState<{ x: number; y: number } | null>(null);
  const [dragLabel, setDragLabel] = useState('');
  const draggingRef = useRef(false);
  const draggedIndexRef = useRef<number | null>(null);
  const insertBeforeRef = useRef<number | null>(null);

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
    if (isMutating) return;
    console.debug('PluginChain: Delete All clicked', { pluginCount: pluginChain.length });
    try {
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
    }
  };

  // Pointer-based drag session (more reliable than HTML5 DnD inside Tauri WebView).
  const startPointerDrag = (e: React.PointerEvent, index: number) => {
    if (isMutating) return;
    if (e.button !== 0) return;
    e.preventDefault();

    draggingRef.current = true;
    draggedIndexRef.current = index;
    setDragLabel(pluginChain[index]?.name ?? 'Plugin');
    setDragPointer({ x: e.clientX, y: e.clientY });
    setDraggedIndex(index);
    setInsertBefore(null);

    const onPointerMove = (ev: PointerEvent) => {
      if (draggedIndexRef.current === null) return;
      setDragPointer({ x: ev.clientX, y: ev.clientY });

      const el = document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null;
      const cardEl = el?.closest('[data-plugin-card-index]') as HTMLElement | null;
      if (!cardEl) return;

      const indexRaw = cardEl.dataset.pluginCardIndex;
      if (indexRaw == null) return;
      const cardIndex = Number(indexRaw);
      if (!Number.isFinite(cardIndex)) return;

      const rect = cardEl.getBoundingClientRect();
      const pos = ev.clientX < rect.left + rect.width / 2 ? cardIndex : cardIndex + 1;
      if (insertBeforeRef.current !== pos) {
        insertBeforeRef.current = pos;
        setInsertBefore(pos);
      }
    };

    const stopPointerDrag = async () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', onPointerUp);
      window.removeEventListener('pointercancel', onPointerCancel);

      const from = draggedIndexRef.current;
      const pos = insertBeforeRef.current;

      draggingRef.current = false;
      draggedIndexRef.current = null;
      insertBeforeRef.current = null;
      setDragPointer(null);
      setDragLabel('');
      setDraggedIndex(null);
      setInsertBefore(null);

      if (from === null || pos === null) return;
      const to = pos > from ? pos - 1 : pos;
      if (from === to) return;

      try {
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
  // Suppress indicator when insertion is a no-op (card stays in same place).
  const showInsertLeft  = (index: number) =>
    draggedIndex !== null &&
    insertBefore === index &&
    insertBefore !== draggedIndex &&
    insertBefore !== draggedIndex + 1;

  const showInsertRight = (index: number) =>
    draggedIndex !== null &&
    insertBefore === index + 1 &&
    insertBefore !== draggedIndex &&
    insertBefore !== draggedIndex + 1;

  return (
    <div className="h-full flex flex-col min-h-0 signal-chain-container">
      {contextHolder}
      {/* Toolbar */}
      <div
        className="glass-panel"
        style={{
          marginBottom: 16,
          padding: '16px 20px',
          borderRadius: token.borderRadiusLG * 1.25,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 16,
          flexWrap: 'wrap',
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
              size="large"
              loading={addLocked}
              disabled={addLocked}
              onClick={() => setShowPluginLibrary(true)}
              aria-label={isChainInitializing ? 'Preparing plugin library' : 'Add plugin'}
            />
          </Tooltip>

          <Popconfirm
            title="Remove all plugins from the chain?"
            onConfirm={handleDeleteAll}
            okText="Remove"
            cancelText="Cancel"
          >
            <Button
              size="small"
              danger
              icon={<DeleteOutlined />}
              disabled={pluginChain.length === 0 || isMutating}
              className="btn-icon"
            >
            </Button>
          </Popconfirm>
        </Space>
      </div>

      {/* Plugin Chain Area */}
      <Card
        style={{
          flex: 1,
          minHeight: 0,
          background: token.colorBgContainer,
          borderRadius: token.borderRadiusLG * 1.25,
          border: `1px solid ${token.colorBorderSecondary}`,
          boxShadow: token.boxShadowTertiary,
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
                border: `1px dashed ${token.colorBorderSecondary}`,
                borderRadius: 10,
                padding: 12,
                overflowX: 'hidden',
                overflowY: 'auto',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  flexWrap: 'wrap',
                  alignItems: 'center',
                  alignContent: 'flex-start',
                  columnGap: 0,
                  rowGap: 30,
                  minHeight: 138,
                }}
              >

            {/* IN node */}
            <Tooltip
              title={
                <Space direction="vertical" size={0} style={{ minWidth: 160 }}>
                  <Text strong style={{ color: token.colorText }}>Input</Text>
                  <Text style={{ color: token.colorTextSecondary }}>Device: {inputDeviceName}</Text>
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
                  style={{ width: '100%', height: 145, flexShrink: 0, overflow: 'hidden' }}
                  styles={{ body: {
                    position: 'relative',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'stretch',
                    justifyContent: 'space-between',
                    padding: 12,
                    height: '100%',
                    borderRadius: 18,
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
              const isLeft  = showInsertLeft(index);
              const isRight = showInsertRight(index);
              return (
              <div key={plugin.instance_id} style={{ display: 'flex', alignItems: 'center' }}>

                {/* Arrow separator (curved/dashed) */}
                <div style={{ display: 'flex', alignItems: 'center', margin: '0 6px', flexShrink: 0 }}>
                  <CurvedArrow color={isLeft ? token.colorPrimary : token.colorTextQuaternary} />
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
                    transform: draggedIndex === index ? 'translateY(-2px) scale(1.01)' : 'none',
                    transition: 'opacity 0.15s ease, box-shadow 0.14s ease, transform 0.14s ease',
                    borderRadius: 8,
                    boxShadow: isLeft
                      ? `0 0 0 1px ${token.colorPrimary}55`
                      : isRight
                        ? `0 0 0 1px ${token.colorPrimary}55`
                        : 'none',
                  }}
                >
                  <div style={{ paddingTop: 12 }}>
                    <PluginCard
                      plugin={plugin}
                      crashStatus={crashStatusByInstanceId[plugin.instance_id]}
                      interactionLocked={isMutating}
                      onRemove={() => removeFromChain(plugin.instance_id)}
                      onToggleBypass={() => toggleBypass(plugin.instance_id)}
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
            <div style={{ display: 'flex', alignItems: 'center', margin: '0 6px', flexShrink: 0 }}>
              <CurvedArrow color={showInsertRight(pluginChain.length - 1) ? token.colorPrimary : token.colorTextQuaternary} />
            </div>

            {/* OUT node */}
            <Tooltip
              title={
                <Space direction="vertical" size={0} style={{ minWidth: 180 }}>
                  <Text strong style={{ color: token.colorText }}>Output</Text>
                  <Text style={{ color: token.colorTextSecondary }}>Device: {outputDeviceName}</Text>
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
                  style={{ width: '100%', height: 145, flexShrink: 0, overflow: 'hidden' }}
                  styles={{ body: {
                    position: 'relative',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'stretch',
                    justifyContent: 'space-between',
                    padding: 12,
                    height: '100%',
                    borderRadius: 18,
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
                style={{
                  position: 'fixed',
                  left: dragPointer.x + 14,
                  top: dragPointer.y + 14,
                  pointerEvents: 'none',
                  zIndex: 2000,
                  padding: '8px 10px',
                  borderRadius: 10,
                  border: `1px solid ${token.colorPrimaryBorder}`,
                  background: `linear-gradient(180deg, ${token.colorBgElevated}, ${token.colorFillSecondary})`,
                  boxShadow: `0 12px 28px ${token.colorFillSecondary}`,
                  color: token.colorText,
                  fontSize: 12,
                  fontWeight: 600,
                  maxWidth: 220,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
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
