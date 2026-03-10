import { useState, useEffect } from 'react';
import { Modal, Input, Button, List, Space, Typography, Empty, Divider, message, Popconfirm } from 'antd';
import { 
  SaveOutlined, 
  DeleteOutlined, 
  DownloadOutlined, 
  FolderOpenOutlined 
} from '@ant-design/icons';
import { usePresetStore } from '../stores/presetStore';

const { Text } = Typography;

interface PresetManagerProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function PresetManager({ isOpen, onClose }: PresetManagerProps) {
  const { presets, fetchPresets, savePreset, loadPreset, deletePreset } = usePresetStore();
  const [presetName, setPresetName] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [selectedPreset, setSelectedPreset] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      fetchPresets();
    }
  }, [isOpen, fetchPresets]);

  const handleSave = async () => {
    if (!presetName.trim()) {
      message.warning('Please enter a preset name');
      return;
    }

    setIsSaving(true);
    try {
      await savePreset(presetName);
      setPresetName('');
      message.success(`Preset "${presetName}" saved successfully!`);
    } catch (error) {
      message.error('Failed to save preset');
    } finally {
      setIsSaving(false);
    }
  };

  const handleLoad = async (name: string) => {
    try {
      await loadPreset(name);
      message.success(`Preset "${name}" loaded successfully!`);
      onClose();
    } catch (error) {
      message.error('Failed to load preset');
    }
  };

  const handleDelete = async (name: string) => {
    try {
      await deletePreset(name);
      message.success(`Preset "${name}" deleted`);
    } catch (error) {
      message.error('Failed to delete preset');
    }
  };

  return (
    <Modal
      title={
        <Space>
          <FolderOpenOutlined style={{ color: '#1677ff' }} />
          <span>Preset Manager</span>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={700}
      footer={[
        <Button key="close" onClick={onClose}>
          Close
        </Button>,
      ]}
    >
      <Divider />
      
      {/* Save New Preset */}
      <Space direction="vertical" style={{ width: '100%', marginBottom: 24 }}>
        <Text strong style={{ fontSize: 16 }}>Save Current Chain</Text>
        <Space.Compact style={{ width: '100%' }}>
          <Input
            size="large"
            placeholder="Enter preset name..."
            value={presetName}
            onChange={(e) => setPresetName(e.target.value)}
            onPressEnter={handleSave}
          />
          <Button
            type="primary"
            size="large"
            icon={<SaveOutlined />}
            onClick={handleSave}
            disabled={!presetName.trim()}
            loading={isSaving}
          >
            Save
          </Button>
        </Space.Compact>
      </Space>

      <Divider />

      {/* Preset List */}
      <div>
        <Text strong style={{ fontSize: 16 }}>Saved Presets ({presets.length})</Text>
        
        {presets.length > 0 ? (
          <List
            style={{ marginTop: 16 }}
            dataSource={presets}
            renderItem={(preset) => (
              <List.Item
                style={{
                  padding: '12px 16px',
                  marginBottom: 8,
                  background: selectedPreset === preset ? 'rgba(22, 119, 255, 0.1)' : 'rgba(255, 255, 255, 0.02)',
                  borderRadius: 8,
                  border: selectedPreset === preset ? '2px solid #1677ff' : '1px solid rgba(255, 255, 255, 0.06)',
                  cursor: 'pointer',
                  transition: 'all 0.2s'
                }}
                onClick={() => setSelectedPreset(preset)}
                actions={[
                  <Button
                    key="load"
                    type="primary"
                    size="small"
                    icon={<DownloadOutlined />}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleLoad(preset);
                    }}
                  >
                    Load
                  </Button>,
                  <Popconfirm
                    key="delete"
                    title="Delete Preset"
                    description={`Are you sure you want to delete "${preset}"?`}
                    onConfirm={(e) => {
                      e?.stopPropagation();
                      handleDelete(preset);
                    }}
                    okText="Delete"
                    cancelText="Cancel"
                    okButtonProps={{ danger: true }}
                  >
                    <Button
                      danger
                      size="small"
                      icon={<DeleteOutlined />}
                      onClick={(e) => e.stopPropagation()}
                    >
                      Delete
                    </Button>
                  </Popconfirm>,
                ]}
              >
                <List.Item.Meta
                  title={<Text strong>{preset}</Text>}
                />
              </List.Item>
            )}
          />
        ) : (
          <Empty
            image={<SaveOutlined style={{ fontSize: 64, color: '#666' }} />}
            description={
              <Space direction="vertical" size={0}>
                <Text type="secondary">No presets saved yet</Text>
                <Text type="secondary" style={{ fontSize: 12 }}>
                  Save your first preset above
                </Text>
              </Space>
            }
            style={{ marginTop: 32 }}
          />
        )}
      </div>
    </Modal>
  );
}
