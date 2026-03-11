import { useEffect, useState } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';

const { Text } = Typography;
import { 
  PlusOutlined, 
  ArrowRightOutlined,
  AudioOutlined,
  ExportOutlined,
  HolderOutlined,
  LeftOutlined,
  RightOutlined,
} from '@ant-design/icons';
import { usePluginStore } from '../stores/pluginStore';
import PluginCard from './PluginCard';
import PresetManager from './PresetManager';
import PluginLibrary from './PluginLibrary';
import * as tauri from '../lib/tauri';

export default function PluginChain() {
  const { token } = theme.useToken();
  const { pluginChain, removeFromChain, toggleBypass, fetchChain } = usePluginStore();
  const [showPresetManager, setShowPresetManager] = useState(false);
  const [showPluginLibrary, setShowPluginLibrary] = useState(false);

  // draggedIndex: which card is being dragged
  // insertBefore: the index BEFORE which the dragged card will be inserted
  //               (0 = before first, pluginChain.length = after last)
  // draggedIndex: which card is being dragged
  // insertBefore: index BEFORE which the dragged card will be inserted
  //               (0 = before first,  pluginChain.length = after last)
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [insertBefore, setInsertBefore] = useState<number | null>(null);

  useEffect(() => {
    fetchChain();
    const interval = setInterval(fetchChain, 2000);
    return () => clearInterval(interval);
  }, [fetchChain]);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowPluginLibrary(true);
  };

  // â”€â”€ Drag handle: only the handle strip triggers drag â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', index.toString());
  };

  const handleDragEnd = () => {
    setDraggedIndex(null);
    setInsertBefore(null);
  };

  const handleMovePlugin = async (from: number, to: number) => {
    if (to < 0 || to >= pluginChain.length) return;
    try {
      await tauri.reorderPluginChain(from, to);
      await fetchChain();
    } catch {
      message.error('Failed to reorder plugins');
    }
  };

  // ── Drop targets: each full card is a drop target ─────────────────────────
  // Midpoint of the hovered card determines insertion side:
  //   left half  → insert BEFORE this card  (insertBefore = index)
  //   right half → insert AFTER  this card  (insertBefore = index + 1)
  const handleCardDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'move';
    if (draggedIndex === null) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const pos  = e.clientX < rect.left + rect.width / 2 ? index : index + 1;
    if (insertBefore !== pos) setInsertBefore(pos);
  };

  const handleCardDrop = async (e: React.DragEvent, index: number) => {
    e.preventDefault();
    e.stopPropagation();
    if (draggedIndex === null) { setInsertBefore(null); return; }
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const pos  = e.clientX < rect.left + rect.width / 2 ? index : index + 1;
    const from = draggedIndex;
    const to   = pos > from ? pos - 1 : pos;
    setDraggedIndex(null);
    setInsertBefore(null);
    if (from === to) return;
    try {
      await tauri.reorderPluginChain(from, to);
      await fetchChain();
      message.success('Plugin order updated');
    } catch (error) {
      message.error('Failed to reorder plugins');
      console.error(error);
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
      <Space style={{ marginBottom: 24 }} size="middle">
        <Text strong style={{ fontSize: 22, color: token.colorText, margin: 0 }}>Signal Chain</Text>
        
        <Button 
          type="primary"
          icon={<PlusOutlined />}
          size="large"
          onClick={() => setShowPluginLibrary(true)}
        >
          Add Plugin
        </Button>

        <Button 
          icon={<ExportOutlined />}
          size="large"
          onClick={() => setShowPresetManager(true)}
        >
          Presets
        </Button>
      </Space>

      {/* Plugin Chain Area */}
      <Card
        style={{ flex: 1, background: token.colorBgContainer }}
        styles={{ body: { height: '100%', padding: '24px', overflow: 'auto' } }}
        onContextMenu={handleContextMenu}
      >
        {pluginChain.length > 0 ? (
          <div
            style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 0, paddingBottom: 16 }}
            onDragLeave={(e) => {
              if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node))
                setInsertBefore(null);
            }}
          >

            {/* IN node */}
            <Tooltip title="Audio Input">
              <Card
                style={{ width: 72, height: 110, flexShrink: 0 }}
                styles={{ body: {
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  padding: 8, height: '100%',
                  background: 'linear-gradient(135deg,#15803d,#166534)',
                  borderRadius: 8,
                } }}
              >
                <div style={{ textAlign: 'center', color: '#fff' }}>
                  <AudioOutlined style={{ fontSize: 22, display: 'block', marginBottom: 4 }} />
                  <div style={{ fontWeight: 700, fontSize: 12 }}>IN</div>
                </div>
              </Card>
            </Tooltip>

            {/* â”€â”€ Plugin cards with drop zones between them â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */}
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
                  onDragOver={(e) => handleCardDragOver(e, index)}
                  onDrop={(e) => handleCardDrop(e, index)}
                  style={{
                    position: 'relative',
                    width: 260,
                    flexShrink: 0,
                    opacity: draggedIndex === index ? 0.35 : 1,
                    transition: 'opacity 0.15s ease, box-shadow 0.1s ease',
                    borderRadius: 8,
                    boxShadow: isLeft
                      ? `inset 4px 0 0 ${token.colorPrimary}, 0 0 0 1px ${token.colorPrimary}55`
                      : isRight
                        ? `inset -4px 0 0 ${token.colorPrimary}, 0 0 0 1px ${token.colorPrimary}55`
                        : 'none',
                  }}
                >
                  {/* â”€â”€ Drag handle (top strip) â”€â”€ */}
                  <div
                    draggable
                    onDragStart={(e) => handleDragStart(e, index)}
                    onDragEnd={handleDragEnd}
                    title="Drag to reorder"
                    style={{
                      position: 'absolute',
                      top: 0, left: 0, right: 0,
                      height: 24,
                      borderRadius: '8px 8px 0 0',
                      background: draggedIndex === index
                        ? token.colorPrimary
                        : token.colorFillSecondary,
                      cursor: 'grab',
                      zIndex: 5,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '0 4px',
                      transition: 'background 0.15s ease',
                    }}
                  >
                    {/* Move-left button */}
                    <span
                      draggable={false}
                      onDragStart={e => e.stopPropagation()}
                      style={{ display: 'flex' }}
                    >
                      <Button
                        type="text"
                        size="small"
                        icon={<LeftOutlined style={{ fontSize: 9 }} />}
                        disabled={index === 0}
                        onClick={() => handleMovePlugin(index, index - 1)}
                        style={{
                          height: 18, padding: '0 4px', minWidth: 22,
                          color: draggedIndex === index ? '#fff' : token.colorTextSecondary,
                        }}
                      />
                    </span>

                    {/* Centre grip */}
                    <HolderOutlined style={{
                      fontSize: 11,
                      color: draggedIndex === index ? '#fff' : token.colorTextQuaternary,
                      pointerEvents: 'none',
                    }} />

                    {/* Move-right button */}
                    <span
                      draggable={false}
                      onDragStart={e => e.stopPropagation()}
                      style={{ display: 'flex' }}
                    >
                      <Button
                        type="text"
                        size="small"
                        icon={<RightOutlined style={{ fontSize: 9 }} />}
                        disabled={index === pluginChain.length - 1}
                        onClick={() => handleMovePlugin(index, index + 1)}
                        style={{
                          height: 18, padding: '0 4px', minWidth: 22,
                          color: draggedIndex === index ? '#fff' : token.colorTextSecondary,
                        }}
                      />
                    </span>
                  </div>

                  {/* The card itself â€” not draggable, so buttons work normally */}
                  <div style={{ paddingTop: 24 }}>
                    <PluginCard
                      plugin={plugin}
                      onRemove={() => removeFromChain(plugin.instance_id)}
                      onToggleBypass={() => toggleBypass(plugin.instance_id)}
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
              <Card
                style={{ width: 72, height: 110, flexShrink: 0 }}
                styles={{ body: {
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  padding: 8, height: '100%',
                  background: 'linear-gradient(135deg,#1d4ed8,#1e3a8a)',
                  borderRadius: 8,
                } }}
              >
                <div style={{ textAlign: 'center', color: '#fff' }}>
                  <AudioOutlined style={{ fontSize: 22, display: 'block', marginBottom: 4 }} />
                  <div style={{ fontWeight: 700, fontSize: 12 }}>OUT</div>
                </div>
              </Card>
            </Tooltip>

          </div>
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
                  ðŸ’¡ Drag the top handle of a card to reorder
                </Text>
              </Space>
            }
          >
            <Button 
              type="primary" 
              icon={<PlusOutlined />}
              size="large"
              onClick={() => setShowPluginLibrary(true)}
            >
              Add Your First Plugin
            </Button>
          </Empty>
        )}
      </Card>

      <PluginLibrary
        isOpen={showPluginLibrary}
        onClose={() => setShowPluginLibrary(false)}
      />
      <PresetManager 
        isOpen={showPresetManager} 
        onClose={() => setShowPresetManager(false)} 
      />
    </div>
  );
}
