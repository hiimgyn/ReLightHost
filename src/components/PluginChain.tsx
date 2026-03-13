import { useEffect, useRef, useState } from 'react';
import { lazy, Suspense } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';
import { listen } from '@tauri-apps/api/event';

const { Text } = Typography;
import { 
  PlusOutlined, 
  ArrowRightOutlined,
  AudioOutlined,
  ExportOutlined,
  HolderOutlined,
  LeftOutlined,
  RightOutlined,
  SwapOutlined,
} from '@ant-design/icons';
import { usePluginStore } from '../stores/pluginStore';
import PluginCard from './PluginCard';
const PresetManager = lazy(() => import('./PresetManager'));
const PluginLibrary = lazy(() => import('./PluginLibrary'));
import * as tauri from '../lib/tauri';
import type { PluginChainChangedEvent } from '../lib/types';

export default function PluginChain() {
  const { token } = theme.useToken();
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
  const [showPresetManager, setShowPresetManager] = useState(false);
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
        message.success('Plugin order updated');
      } catch (error) {
        message.error('Failed to reorder plugins');
        console.error(error);
      }
    };

    const onPointerUp = () => { void stopPointerDrag(); };
    const onPointerCancel = () => { void stopPointerDrag(); };

    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerCancel);
  };

  const handleMovePlugin = async (from: number, to: number) => {
    if (isMutating) return;
    if (to < 0 || to >= pluginChain.length) return;
    try {
      await reorderChain(from, to);
    } catch {
      message.error('Failed to reorder plugins');
    }
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
    <div className="h-full flex flex-col">
      {/* Toolbar */}
      <div
        style={{
          marginBottom: 16,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
          flexWrap: 'wrap',
        }}
      >
        <Space direction="vertical" size={0}>
          <Text strong style={{ fontSize: 22, color: token.colorText, margin: 0 }}>
            Signal Chain
          </Text>
        </Space>

        <Space size="middle">
          <Button 
            type="primary"
            icon={<PlusOutlined />}
            size="large"
            loading={addLocked}
            disabled={addLocked}
            onClick={() => setShowPluginLibrary(true)}
          >
            {isChainInitializing ? 'Preparing Chain...' : 'Add Plugin'}
          </Button>

          <Button 
            icon={<ExportOutlined />}
            size="large"
            onClick={() => setShowPresetManager(true)}
          >
            Presets
          </Button>
        </Space>
      </div>

      {/* Plugin Chain Area */}
      <Card
        style={{ flex: 1, background: token.colorBgContainer }}
        styles={{ body: { height: '100%', padding: '18px', overflow: 'hidden' } }}
        onContextMenu={handleContextMenu}
      >
        {pluginChain.length > 0 ? (
          <>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                marginBottom: 10,
                color: token.colorTextSecondary,
                fontSize: 12,
              }}
            >
              <SwapOutlined />
              <span>Drag the top bar to reorder plugins</span>
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
            <Tooltip title="Audio Input">
              <div
                style={{
                  position: 'relative',
                  width: 132,
                  flexShrink: 0,
                  borderRadius: 8,
                }}
              >
                <Card
                  style={{ width: '100%', height: 116, flexShrink: 0 }}
                  styles={{ body: {
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    padding: 8, height: '100%',
                    borderRadius: 8,
                    border: `1px solid ${token.colorSuccess}`,
                    background: `linear-gradient(135deg, ${token.colorBgContainer}, ${token.colorFillQuaternary})`,
                    boxShadow: `0 0px 6px ${token.colorSuccess}`,
                  } }}
                >
                  <div style={{ textAlign: 'center', color: token.colorSuccess }}>
                    <AudioOutlined style={{ fontSize: 18, display: 'block', marginBottom: 4 }} />
                    <div style={{ fontWeight: 700, fontSize: 11, color: token.colorText }}>IN</div>
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

                {/* Arrow separator */}
                <ArrowRightOutlined style={{
                  fontSize: 15,
                  color: isLeft ? token.colorPrimary : token.colorTextQuaternary,
                  flexShrink: 0,
                  margin: '0 4px',
                  transition: 'color 0.1s',
                }} />

                {/* Card wrapper — full drop target */}
                <div
                  data-plugin-card-index={index}
                  style={{
                    position: 'relative',
                    width: 252,
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
                  {isLeft && (
                    <div
                      style={{
                        position: 'absolute',
                        left: -7,
                        top: 10,
                        bottom: 10,
                        width: 4,
                        borderRadius: 99,
                        background: token.colorPrimary,
                        boxShadow: `0 0 10px ${token.colorPrimary}`,
                        zIndex: 7,
                      }}
                    />
                  )}
                  {isRight && (
                    <div
                      style={{
                        position: 'absolute',
                        right: -7,
                        top: 10,
                        bottom: 10,
                        width: 4,
                        borderRadius: 99,
                        background: token.colorPrimary,
                        boxShadow: `0 0 10px ${token.colorPrimary}`,
                        zIndex: 7,
                      }}
                    />
                  )}

                  {/* Drag handle (top strip) */}
                  <div
                    title="Drag to reorder"
                    onPointerDown={(e) => startPointerDrag(e, index)}
                    style={{
                      position: 'absolute',
                      top: 4,
                      left: 10,
                      right: 10,
                      height: 20,
                      borderRadius: 999,
                      background: draggedIndex === index
                        ? `linear-gradient(90deg, ${token.colorPrimary}, ${token.colorPrimaryHover})`
                        : `linear-gradient(90deg, ${token.colorFillSecondary}, ${token.colorFillTertiary})`,
                      border: `1px solid ${draggedIndex === index ? token.colorPrimaryBorder : token.colorBorderSecondary}`,
                      boxShadow: draggedIndex === index
                        ? `0 6px 16px ${token.colorPrimaryBg}`
                        : `0 2px 6px ${token.colorFillSecondary}`,
                      cursor: 'grab',
                      zIndex: 5,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '0 6px',
                      transition: 'background 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease',
                    }}
                  >
                    {/* Move-left button */}
                    <span
                      onPointerDown={e => e.stopPropagation()}
                      style={{ display: 'flex' }}
                    >
                      <Button
                        type="text"
                        size="small"
                        icon={<LeftOutlined style={{ fontSize: 9 }} />}
                        disabled={index === 0 || isMutating}
                        onClick={() => handleMovePlugin(index, index - 1)}
                        style={{
                          height: 16,
                          padding: '0 2px',
                          minWidth: 18,
                          borderRadius: 6,
                          color: draggedIndex === index ? '#fff' : token.colorTextSecondary,
                          background: draggedIndex === index ? 'rgba(255,255,255,0.14)' : 'transparent',
                        }}
                      />
                    </span>

                    {/* Centre grip */}
                    <span
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 4,
                        pointerEvents: 'none',
                        color: draggedIndex === index ? '#fff' : token.colorTextTertiary,
                        fontSize: 10,
                        fontWeight: 600,
                        letterSpacing: 0.3,
                        textTransform: 'uppercase',
                      }}
                    >
                      <HolderOutlined style={{ fontSize: 10 }} />
                    </span>

                    {/* Move-right button */}
                    <span
                      onPointerDown={e => e.stopPropagation()}
                      style={{ display: 'flex' }}
                    >
                      <Button
                        type="text"
                        size="small"
                        icon={<RightOutlined style={{ fontSize: 9 }} />}
                        disabled={index === pluginChain.length - 1 || isMutating}
                        onClick={() => handleMovePlugin(index, index + 1)}
                        style={{
                          height: 16,
                          padding: '0 2px',
                          minWidth: 18,
                          borderRadius: 6,
                          color: draggedIndex === index ? '#fff' : token.colorTextSecondary,
                          background: draggedIndex === index ? 'rgba(255,255,255,0.14)' : 'transparent',
                        }}
                      />
                    </span>
                  </div>

                  {/* The card itself not draggable, so buttons work normally */}
                  <div style={{ paddingTop: 24 }}>
                    <PluginCard
                      plugin={plugin}
                      crashStatus={crashStatusByInstanceId[plugin.instance_id]}
                      interactionLocked={isMutating}
                      onRemove={() => removeFromChain(plugin.instance_id)}
                      onToggleBypass={() => toggleBypass(plugin.instance_id)}
                      onCrashStatusChanged={fetchCrashStatuses}
                      onLaunch={async () => {
                        try {
                          await tauri.launchPlugin(plugin.instance_id);
                        } catch {
                          message.error('Failed to launch plugin');
                        }
                      }}
                    />
                  </div>
                </div>
              </div>
              );
            })}

            {/* Arrow after last card */}
            <ArrowRightOutlined style={{
              fontSize: 15,
              color: showInsertRight(pluginChain.length - 1) ? token.colorPrimary : token.colorTextQuaternary,
              flexShrink: 0,
              margin: '0 4px',
              transition: 'color 0.1s',
            }} />

            {/* OUT node */}
            <Tooltip title="Audio Output">
              <div
                style={{
                  position: 'relative',
                  width: 132,
                  flexShrink: 0,
                  borderRadius: 8,
                }}
              >
                <Card
                  style={{ width: '100%', height: 116, flexShrink: 0 }}
                  styles={{ body: {
                    display: 'flex', alignItems: 'center', justifyContent: 'center',
                    padding: 8, height: '100%',
                    borderRadius: 8,
                    border: `1px solid ${token.colorInfo}`,
                    background: `linear-gradient(135deg, ${token.colorBgContainer}, ${token.colorFillQuaternary})`,
                    boxShadow: `0 0px 6px ${token.colorInfo}`,
                  } }}
                >
                  <div style={{ textAlign: 'center', color: token.colorInfo }}>
                    <AudioOutlined style={{ fontSize: 18, display: 'block', marginBottom: 4 }} />
                    <div style={{ fontWeight: 700, fontSize: 11, color: token.colorText }}>OUT</div>
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
              <Space direction="vertical" size={0}>
                <Text style={{ fontSize: 16, color: token.colorTextSecondary }}>No plugins loaded</Text>
                <Text style={{ fontSize: 13, color: token.colorTextTertiary }}>
                  Right-click or use the "Add Plugin" button to add plugins
                </Text>
                <Text style={{ fontSize: 12, color: token.colorTextQuaternary, marginTop: 4 }}>
                  Tip: drag the top bar of each card to reorder
                </Text>
              </Space>
            }
          >
            <Button 
              type="primary" 
              icon={<PlusOutlined />}
              size="large"
              onClick={() => setShowPluginLibrary(true)}
              disabled={addLocked}
            >
              Add Your First Plugin
            </Button>
          </Empty>
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
      {showPresetManager && (
        <Suspense fallback={null}>
          <PresetManager 
            isOpen={showPresetManager} 
            onClose={() => setShowPresetManager(false)} 
          />
        </Suspense>
      )}
    </div>
  );
}
