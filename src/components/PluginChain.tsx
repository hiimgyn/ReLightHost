import { useEffect, useState } from 'react';
import { Card, Button, Empty, Space, Tooltip, message, theme, Typography } from 'antd';

const { Text } = Typography;
import { 
  PlusOutlined, 
  ArrowRightOutlined,
  AudioOutlined,
  ExportOutlined 
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
            {pluginChain.map((plugin) => (
              <div key={plugin.instance_id} className="flex items-center gap-4">
                <ArrowRightOutlined style={{ fontSize: 22, color: token.colorTextQuaternary, flexShrink: 0 }} />
                <div className="flex-shrink-0 w-72">
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
                <Text style={{ fontSize: 13, color: token.colorTextTertiary }}>Right-click or use the "Add Plugin" button to add plugins</Text>
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
