import { useEffect, useState } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';

const { Text } = Typography;
import { 
  PlusOutlined, 
  ArrowRightOutlined,
  AudioOutlined,
  ExportOutlined,
  DragOutlined
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
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);

  useEffect(() => {
    // Fetch chain on mount
    fetchChain();
    // Poll chain every 2 seconds
    const interval = setInterval(fetchChain, 2000);
    return () => clearInterval(interval);
  }, [fetchChain]);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowPluginLibrary(true);
  };

  const handleDragStart = (e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', index.toString());
  };

  const handleDragOver = (e: React.DragEvent, index: number) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    setDragOverIndex(index);
  };

  const handleDragLeave = () => {
    setDragOverIndex(null);
  };

  const handleDrop = async (e: React.DragEvent, dropIndex: number) => {
    e.preventDefault();
    
    if (draggedIndex === null || draggedIndex === dropIndex) {
      setDraggedIndex(null);
      setDragOverIndex(null);
      return;
    }

    try {
      await tauri.reorderPluginChain(draggedIndex, dropIndex);
      await fetchChain();
      message.success('Plugin order updated');
    } catch (error) {
      message.error('Failed to reorder plugins');
      console.error(error);
    } finally {
      setDraggedIndex(null);
      setDragOverIndex(null);
    }
  };

  const handleDragEnd = () => {
    setDraggedIndex(null);
    setDragOverIndex(null);
  };

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

        {pluginChain.length > 1 && (
          <Tooltip title="Drag & drop plugins to reorder" placement="right">
            <DragOutlined style={{ fontSize: 16, color: token.colorTextQuaternary, marginLeft: 8 }} />
          </Tooltip>
        )}
      </Space>

      {/* Plugin Chain Area - Flex Layout with Context Menu */}
      <Card
        style={{ flex: 1, background: token.colorBgContainer }}
        bodyStyle={{ height: '100%', padding: '24px', overflow: 'auto' }}
        onContextMenu={handleContextMenu}
      >
        {pluginChain.length > 0 ? (
          <div className="flex flex-wrap items-start gap-4 pb-4">
            {/* Input */}
            <Tooltip title="Audio Input">
              <Card 
                className="flex-shrink-0 w-24 h-32 bg-gradient-to-br from-green-600 to-green-700 border-2 border-green-400"
                bodyStyle={{ 
                  display: 'flex', 
                  alignItems: 'center', 
                  justifyContent: 'center',
                  padding: '12px'
                }}
              >
                <div className="text-center text-white">
                  <AudioOutlined className="text-3xl mb-2" />
                  <div className="font-bold text-sm">IN</div>
                </div>
              </Card>
            </Tooltip>

            {/* Plugin Chain */}
            {pluginChain.map((plugin, index) => (
              <div 
                key={plugin.instance_id} 
                className="flex items-center gap-4"
                draggable
                onDragStart={(e) => handleDragStart(e, index)}
                onDragOver={(e) => handleDragOver(e, index)}
                onDragLeave={handleDragLeave}
                onDrop={(e) => handleDrop(e, index)}
                onDragEnd={handleDragEnd}
                style={{
                  opacity: draggedIndex === index ? 0.5 : 1,
                  transform: dragOverIndex === index && draggedIndex !== index ? 'scale(1.02)' : 'scale(1)',
                  transition: 'all 0.2s ease',
                  cursor: 'move',
                  position: 'relative',
                }}
              >
                <ArrowRightOutlined style={{ fontSize: 22, color: token.colorTextQuaternary, flexShrink: 0 }} />
                <div 
                  className="flex-shrink-0 w-72"
                  style={{
                    border: dragOverIndex === index && draggedIndex !== index 
                      ? `2px dashed ${token.colorPrimary}` 
                      : 'none',
                    borderRadius: '8px',
                    padding: dragOverIndex === index && draggedIndex !== index ? '2px' : '0',
                  }}
                >
                  {/* Drag Handle Indicator */}
                  <div 
                    style={{ 
                      position: 'absolute', 
                      left: '-28px', 
                      top: '50%', 
                      transform: 'translateY(-50%)',
                      color: token.colorTextQuaternary,
                      cursor: 'move',
                      opacity: draggedIndex === index ? 0.3 : 0.6,
                    }}
                  >
                    <DragOutlined style={{ fontSize: 16 }} />
                  </div>
                  
                  <PluginCard
                    plugin={plugin}
                    onRemove={() => removeFromChain(plugin.instance_id)}
                    onToggleBypass={() => toggleBypass(plugin.instance_id)}
                    onLaunch={async () => {
                      try {
                        await tauri.launchPlugin(plugin.instance_id);
                        message.info(`${plugin.name} launched`);
                      } catch {
                        message.error('Failed to launch plugin');
                      }
                    }}
                  />
                </div>
              </div>
            ))}

            {/* Arrow to Output */}
            <ArrowRightOutlined style={{ fontSize: 22, color: token.colorTextQuaternary, flexShrink: 0 }} />

            {/* Output */}
            <Tooltip title="Audio Output">
              <Card 
                className="flex-shrink-0 w-24 h-32 bg-gradient-to-br from-blue-600 to-blue-700 border-2 border-blue-400"
                bodyStyle={{ 
                  display: 'flex', 
                  alignItems: 'center', 
                  justifyContent: 'center',
                  padding: '12px'
                }}
              >
                <div className="text-center text-white">
                  <AudioOutlined className="text-3xl mb-2" />
                  <div className="font-bold text-sm">OUT</div>
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
                  💡 Tip: Drag & drop plugins to reorder them
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

      {/* Plugin Library Modal */}
      <PluginLibrary
        isOpen={showPluginLibrary}
        onClose={() => setShowPluginLibrary(false)}
      />

      {/* Preset Manager Modal */}
      <PresetManager 
        isOpen={showPresetManager} 
        onClose={() => setShowPresetManager(false)} 
      />

    </div>
  );
}
