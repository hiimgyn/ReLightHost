import { useEffect, useState, useRef } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';

const { Text } = Typography;
import { 
  PlusOutlined, 
  ArrowRightOutlined,
  AudioOutlined,
  ExportOutlined,
  HolderOutlined,
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
  const [draggedIndex, setDraggedIndex]   = useState<number | null>(null);
  const [insertBefore, setInsertBefore]   = useState<number | null>(null);
  const dragCounter = useRef(0); // tracks enter/leave of nested elements

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
    // Show a clean ghost (empty image) â€” the card dims via opacity
    const ghost = document.createElement('div');
    ghost.style.cssText = 'width:1px;height:1px;position:absolute;top:-9999px';
    document.body.appendChild(ghost);
    e.dataTransfer.setDragImage(ghost, 0, 0);
    setTimeout(() => document.body.removeChild(ghost), 0);
  };

  const handleDragEnd = () => {
    setDraggedIndex(null);
    setInsertBefore(null);
    dragCounter.current = 0;
  };

  // â”€â”€ Drop zone: invisible strip between cards â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  const handleZoneDragEnter = (e: React.DragEvent, pos: number) => {
    e.preventDefault();
    dragCounter.current++;
    setInsertBefore(pos);
  };

  const handleZoneDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  };

  const handleZoneDragLeave = () => {
    dragCounter.current--;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setInsertBefore(null);
    }
  };

  const handleZoneDrop = async (e: React.DragEvent, pos: number) => {
    e.preventDefault();
    dragCounter.current = 0;
    setInsertBefore(null);

    if (draggedIndex === null) return;

    // Normalise: when moving forward, insertion point shifts by -1 after remove
    const from = draggedIndex;
    // `pos` is the index BEFORE which we insert in the ORIGINAL array.
    // After remove(from), if pos > from, pos effectively shifts left by 1.
    const to = pos > from ? pos - 1 : pos;

    if (from === to) { setDraggedIndex(null); return; }

    try {
      await tauri.reorderPluginChain(from, to);
      await fetchChain();
      message.success('Plugin order updated');
    } catch (error) {
      message.error('Failed to reorder plugins');
      console.error(error);
    } finally {
      setDraggedIndex(null);
    }
  };

  // â”€â”€ Helper: is a drop zone "active" for the current drag? â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
  // Suppress insertion line next to the dragged card itself (looks silly)
  const isActiveZone = (pos: number) => {
    if (insertBefore !== pos || draggedIndex === null) return false;
    // pos == draggedIndex means "insert before self" (no-op)
    // pos == draggedIndex+1 means "insert after self" (no-op)
    if (pos === draggedIndex || pos === draggedIndex + 1) return false;
    return true;
  };

  // â”€â”€ Drop zone strip component (rendered between every pair of items) â”€â”€â”€â”€â”€â”€â”€
  const DropZone = ({ pos }: { pos: number }) => (
    <div
      onDragEnter={(e) => handleZoneDragEnter(e, pos)}
      onDragOver={handleZoneDragOver}
      onDragLeave={handleZoneDragLeave}
      onDrop={(e) => handleZoneDrop(e, pos)}
      style={{
        width: isActiveZone(pos) ? 4 : 12,
        alignSelf: 'stretch',
        flexShrink: 0,
        borderRadius: 4,
        background: isActiveZone(pos) ? token.colorPrimary : 'transparent',
        boxShadow: isActiveZone(pos)
          ? `0 0 8px ${token.colorPrimary}88`
          : 'none',
        transition: 'all 0.1s ease',
        cursor: 'default',
      }}
    />
  );

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
        bodyStyle={{ height: '100%', padding: '24px', overflow: 'auto' }}
        onContextMenu={handleContextMenu}
      >
        {pluginChain.length > 0 ? (
          <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 0, paddingBottom: 16 }}>

            {/* IN node */}
            <Tooltip title="Audio Input">
              <Card
                style={{ width: 72, height: 110, flexShrink: 0 }}
                bodyStyle={{
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  padding: 8, height: '100%',
                  background: 'linear-gradient(135deg,#15803d,#166534)',
                  borderRadius: 8,
                }}
              >
                <div style={{ textAlign: 'center', color: '#fff' }}>
                  <AudioOutlined style={{ fontSize: 22, display: 'block', marginBottom: 4 }} />
                  <div style={{ fontWeight: 700, fontSize: 12 }}>IN</div>
                </div>
              </Card>
            </Tooltip>

            {/* â”€â”€ Plugin cards with drop zones between them â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ */}
            {pluginChain.map((plugin, index) => (
              <div key={plugin.instance_id} style={{ display: 'flex', alignItems: 'center' }}>

                {/* Drop zone BEFORE this card (+ arrow visually) */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
                  <ArrowRightOutlined style={{
                    fontSize: 15,
                    color: token.colorTextQuaternary,
                    flexShrink: 0,
                    margin: '0 2px',
                  }} />
                  <DropZone pos={index} />
                </div>

                {/* Card wrapper */}
                <div
                  style={{
                    position: 'relative',
                    width: 260,
                    flexShrink: 0,
                    opacity: draggedIndex === index ? 0.35 : 1,
                    transition: 'opacity 0.15s ease',
                    borderRadius: 8,
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
                      height: 10,
                      borderRadius: '8px 8px 0 0',
                      background: draggedIndex === index
                        ? token.colorPrimary
                        : token.colorFillSecondary,
                      cursor: 'grab',
                      zIndex: 5,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      transition: 'background 0.15s ease',
                    }}
                  >
                    <HolderOutlined style={{
                      fontSize: 10,
                      color: draggedIndex === index
                        ? '#fff'
                        : token.colorTextQuaternary,
                      pointerEvents: 'none',
                    }} />
                  </div>

                  {/* The card itself â€” not draggable, so buttons work normally */}
                  <div style={{ paddingTop: 10 }}>
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
            ))}

            {/* Drop zone AFTER last card */}
            <div style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
              <ArrowRightOutlined style={{
                fontSize: 15,
                color: token.colorTextQuaternary,
                flexShrink: 0,
                margin: '0 2px',
              }} />
              <DropZone pos={pluginChain.length} />
            </div>

            {/* OUT node */}
            <Tooltip title="Audio Output">
              <Card
                style={{ width: 72, height: 110, flexShrink: 0 }}
                bodyStyle={{
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  padding: 8, height: '100%',
                  background: 'linear-gradient(135deg,#1d4ed8,#1e3a8a)',
                  borderRadius: 8,
                }}
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
