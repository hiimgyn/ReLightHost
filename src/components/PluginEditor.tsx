import { useState } from 'react';
import { Modal, Slider, InputNumber, Collapse, Space, Button, Descriptions, Tag, Empty, Divider } from 'antd';
import { 
  ReloadOutlined, 
  SettingOutlined,
  ThunderboltOutlined,
  InfoCircleOutlined 
} from '@ant-design/icons';
import type { PluginInstanceInfo, PluginParameter } from '../lib/types';

const { Panel } = Collapse;

interface PluginEditorProps {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
  onParameterChange?: (paramId: number, value: number) => void;
}

export default function PluginEditor({ plugin, isOpen, onClose, onParameterChange }: PluginEditorProps) {
  const [activeKeys, setActiveKeys] = useState<string[]>(['parameters']);

  const handleParameterChange = (param: PluginParameter, value: number | null) => {
    if (value !== null && onParameterChange) {
      onParameterChange(param.id, value);
    }
  };

  const handleReset = (param: PluginParameter) => {
    if (onParameterChange) {
      onParameterChange(param.id, param.default);
    }
  };

  return (
    <Modal
      title={
        <Space>
          <SettingOutlined style={{ color: '#1677ff' }} />
          <span>{plugin.name}</span>
          <Tag color={plugin.bypassed ? 'warning' : 'success'}>
            {plugin.bypassed ? 'Bypassed' : 'Active'}
          </Tag>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={800}
      footer={[
        <Button key="close" type="primary" onClick={onClose}>
          Close
        </Button>,
      ]}
      style={{ top: 40 }}
    >
      <Divider />
      
      {plugin.parameters.length > 0 ? (
        <Collapse 
          activeKey={activeKeys}
          onChange={(keys) => setActiveKeys(keys as string[])}
          defaultActiveKey={['parameters']}
          style={{ marginBottom: 24 }}
        >
          <Panel 
            header={
              <Space>
                <ThunderboltOutlined />
                <span>Parameters ({plugin.parameters.length})</span>
              </Space>
            } 
            key="parameters"
          >
            <Space direction="vertical" style={{ width: '100%' }} size="large">
              {plugin.parameters.map((param) => (
                <div key={param.id} style={{ 
                  padding: '16px',
                  background: 'rgba(255, 255, 255, 0.02)',
                  borderRadius: 8,
                  border: '1px solid rgba(255, 255, 255, 0.06)'
                }}>
                  <div style={{ 
                    display: 'flex', 
                    justifyContent: 'space-between', 
                    alignItems: 'center',
                    marginBottom: 12 
                  }}>
                    <Space>
                      <span style={{ fontWeight: 600, fontSize: 14 }}>{param.name}</span>
                      {param.value !== param.default && (
                        <Tag color="blue">Modified</Tag>
                      )}
                    </Space>
                    <Button
                      type="text"
                      size="small"
                      icon={<ReloadOutlined />}
                      onClick={() => handleReset(param)}
                      title="Reset to default"
                    >
                      Reset
                    </Button>
                  </div>

                  <Space style={{ width: '100%' }} size="middle">
                    <Slider
                      min={param.min}
                      max={param.max}
                      step={(param.max - param.min) / 1000}
                      value={param.value}
                      onChange={(value) => handleParameterChange(param, value)}
                      style={{ flex: 1, minWidth: 400 }}
                      tooltip={{ 
                        formatter: (val) => val?.toFixed(3),
                        placement: 'top'
                      }}
                    />
                    <InputNumber
                      min={param.min}
                      max={param.max}
                      step={(param.max - param.min) / 1000}
                      value={param.value}
                      onChange={(value) => handleParameterChange(param, value)}
                      style={{ width: 120 }}
                      precision={3}
                    />
                  </Space>

                  <div style={{ 
                    display: 'flex', 
                    justifyContent: 'space-between',
                    marginTop: 8,
                    fontSize: 12,
                    color: '#999'
                  }}>
                    <span>Min: {param.min}</span>
                    <span>Default: {param.default.toFixed(3)}</span>
                    <span>Max: {param.max}</span>
                  </div>
                </div>
              ))}
            </Space>
          </Panel>

          <Panel 
            header={
              <Space>
                <InfoCircleOutlined />
                <span>Plugin Information</span>
              </Space>
            } 
            key="info"
          >
            <Descriptions bordered column={2} size="small">
              <Descriptions.Item label="Name" span={2}>
                <strong>{plugin.name}</strong>
              </Descriptions.Item>
              <Descriptions.Item label="Vendor">{plugin.manufacture || '—'}</Descriptions.Item>
              <Descriptions.Item label="Version">{plugin.version || '—'}</Descriptions.Item>
              <Descriptions.Item label="Format">
                <Tag color={plugin.format === 'clap' ? 'blue' : plugin.format === 'vst3' ? 'purple' : 'green'}>
                  {plugin.format?.toUpperCase() ?? '—'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Category">{plugin.category || '—'}</Descriptions.Item>
              <Descriptions.Item label="Status">
                <Tag color={plugin.bypassed ? 'warning' : 'success'}>
                  {plugin.bypassed ? 'Bypassed' : 'Active'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Parameters">{plugin.parameters.length}</Descriptions.Item>
              <Descriptions.Item label="Path" span={2}>
                <code style={{ fontSize: 10, wordBreak: 'break-all' }}>{plugin.path || '—'}</code>
              </Descriptions.Item>
              <Descriptions.Item label="Instance ID" span={2}>
                <code style={{ fontSize: 10 }}>{plugin.instance_id}</code>
              </Descriptions.Item>
            </Descriptions>
          </Panel>
        </Collapse>
      ) : (
        <Empty
          image={<SettingOutlined style={{ fontSize: 64, color: '#666' }} />}
          description={
            <Space direction="vertical" size={0}>
              <span>No parameters available</span>
              <span style={{ fontSize: 12, color: '#999' }}>
                This plugin doesn't expose any parameters
              </span>
            </Space>
          }
        />
      )}
    </Modal>
  );
}
