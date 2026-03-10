import { Card, Button, Space, Tag, Tooltip, theme, message } from 'antd';
import { 
  CloseOutlined, 
  PoweroffOutlined, 
  PlayCircleOutlined,
  ThunderboltOutlined,
  WarningOutlined,
  ReloadOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { useState, useEffect } from 'react';
import type { PluginInstanceInfo, PluginStatus } from '../lib/types';
import * as tauri from '../lib/tauri';
import NoiseSuppressorGui from './NoiseSuppressorGui';

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  onRemove: () => void;
  onToggleBypass: () => void;
  onLaunch?: () => void;
}

export default function PluginCard({ plugin, onRemove, onToggleBypass, onLaunch }: PluginCardProps) {
  const { token } = theme.useToken();
  const [crashStatus, setCrashStatus] = useState<PluginStatus>({ type: 'Ok' });
  const [checkingStatus, setCheckingStatus] = useState(false);
  const [showBuiltinGui, setShowBuiltinGui] = useState(false);
  
  // Check crash status periodically
  useEffect(() => {
    const checkStatus = async () => {
      try {
        const status = await tauri.getPluginCrashStatus(plugin.instance_id);
        setCrashStatus(status);
      } catch (err) {
        console.error('Failed to check crash status:', err);
      }
    };
    
    // Check immediately
    checkStatus();
    
    // Then check every 2 seconds
    const interval = setInterval(checkStatus, 2000);
    return () => clearInterval(interval);
  }, [plugin.instance_id]);
  
  const handleResetCrash = async () => {
    try {
      setCheckingStatus(true);
      await tauri.resetPluginCrashProtection(plugin.instance_id);
      setCrashStatus({ type: 'Ok' });
      message.success('Plugin crash protection reset');
    } catch (err) {
      message.error(`Failed to reset: ${err}`);
    } finally {
      setCheckingStatus(false);
    }
  };
  
  const getCrashStatusTag = () => {
    if (crashStatus.type === 'Crashed') {
      return (
        <Tag icon={<WarningOutlined />} color="error">
          CRASHED: {crashStatus.data}
        </Tag>
      );
    } else if (crashStatus.type === 'Error') {
      return (
        <Tag icon={<WarningOutlined />} color="warning">
          ERROR: {crashStatus.data}
        </Tag>
      );
    } else if (crashStatus.type === 'Timeout') {
      return (
        <Tag icon={<WarningOutlined />} color="warning">
          TIMEOUT
        </Tag>
      );
    }
    return null;
  };
  
  const isCrashed = crashStatus.type !== 'Ok';
  
  return (
    <>
    <Card
      size="small"
      className={`
        transition-all shadow-lg
        ${plugin.bypassed ? 'opacity-60' : ''}
        ${isCrashed ? 'border-red-500' : ''}
      `}
      style={{
        borderWidth: 2,
        borderColor: isCrashed ? token.colorError : (plugin.bypassed ? token.colorBorderSecondary : token.colorPrimary),
        background: token.colorBgContainer,
      }}
      bodyStyle={{ padding: '16px' }}
    >
      {/* Header */}
      <Space direction="vertical" size="small" style={{ width: '100%' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <h3 style={{ 
              fontWeight: 600, 
              color: token.colorText, 
              marginBottom: 4,
              fontSize: 15,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis'
            }}>
              {plugin.name}
            </h3>
            <code style={{ 
              fontSize: 10, 
              color: token.colorTextSecondary,
              display: 'block'
            }}>
              {plugin.instance_id.slice(0, 12)}...
            </code>
          </div>
        </div>
        
        {/* Crash Status */}
        {getCrashStatusTag()}

        {/* Parameters Info */}
        {plugin.parameters.length > 0 && (
          <Tag icon={<ThunderboltOutlined />} color="blue" style={{ fontSize: 11 }}>
            {plugin.parameters.length} parameter{plugin.parameters.length !== 1 ? 's' : ''}
          </Tag>
        )}

        {/* Action Buttons */}
        <Space style={{ width: '100%' }} size="small">
          {isCrashed && (
            <Tooltip title="Reset Crash Protection">
              <Button
                type="default"
                size="small"
                icon={<ReloadOutlined />}
                onClick={handleResetCrash}
                loading={checkingStatus}
                style={{ flex: 1 }}
              >
                Reset
              </Button>
            </Tooltip>
          )}
          
          <Tooltip title={plugin.bypassed ? 'Enable Plugin' : 'Bypass Plugin'}>
            <Button
              type={plugin.bypassed ? 'default' : 'primary'}
              danger={!plugin.bypassed}
              size="small"
              icon={<PoweroffOutlined />}
              onClick={onToggleBypass}
              style={{ flex: 1 }}
              disabled={isCrashed}
            >
              {plugin.bypassed ? 'Bypassed' : 'Active'}
            </Button>
          </Tooltip>

          <Tooltip title="Launch Plugin">
            <Button
              type="default"
              size="small"
              icon={plugin.format === 'builtin' ? <SettingOutlined /> : <PlayCircleOutlined />}
              onClick={plugin.format === 'builtin' ? () => setShowBuiltinGui(true) : onLaunch}
              disabled={isCrashed}
            >
              {plugin.format === 'builtin' ? 'Settings' : 'Launch'}
            </Button>
          </Tooltip>

          <Tooltip title="Remove from Chain">
            <Button
              type="primary"
              danger
              size="small"
              icon={<CloseOutlined />}
              onClick={onRemove}
            />
          </Tooltip>
        </Space>
      </Space>
    </Card>

    {/* Built-in plugin GUI (rendered outside the Card to avoid z-index issues) */}
    {plugin.format === 'builtin' && (
      <NoiseSuppressorGui
        plugin={plugin}
        isOpen={showBuiltinGui}
        onClose={() => setShowBuiltinGui(false)}
      />
    )}
  </>
  );
}
