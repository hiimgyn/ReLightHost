import { Card, Button, Space, Tag, Tooltip, theme, message, Input } from 'antd';
import type { InputRef } from 'antd';
import { 
  CloseOutlined, 
  PoweroffOutlined, 
  PlayCircleOutlined,
  LoadingOutlined,
  CheckCircleOutlined,
  WarningOutlined,
  ReloadOutlined,
  SettingOutlined,
  EditOutlined,
  CheckOutlined,
  CloseCircleOutlined,
} from '@ant-design/icons';
import { useState, useEffect, useRef } from 'react';
import type { PluginInstanceInfo, PluginStatus } from '../lib/types';
import * as tauri from '../lib/tauri';
import NoiseSuppressorGui from './NoiseSuppressorGui';
import CompressorGui from './CompressorGui';

interface PluginCardProps {
  plugin: PluginInstanceInfo;
  onRemove: () => void;
  onToggleBypass: () => void;
  onLaunch?: () => void;
}

function getFormatColor(format: string) {
  if (format === 'vst3') return 'purple';
  if (format === 'vst')  return 'blue';
  if (format === 'clap') return 'cyan';
  return 'green';
}

export default function PluginCard({ plugin, onRemove, onToggleBypass, onLaunch }: PluginCardProps) {
  const { token } = theme.useToken();
  const [crashStatus, setCrashStatus] = useState<PluginStatus>({ type: 'Ok' });
  const [checkingStatus, setCheckingStatus] = useState(false);
  const [showBuiltinGui, setShowBuiltinGui] = useState(false);

  // GUI launching state — reset once plugin.gui_open becomes true or after timeout
  const [isLaunching, setIsLaunching] = useState(false);
  const launchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Inline rename state
  const [isRenaming, setIsRenaming] = useState(false);
  const [editName, setEditName] = useState(plugin.name);
  const renameInputRef = useRef<InputRef | null>(null);

  // Sync edit name when plugin name changes externally
  useEffect(() => {
    if (!isRenaming) setEditName(plugin.name);
  }, [plugin.name, isRenaming]);

  // When gui_open becomes true, clear the launching spinner
  useEffect(() => {
    if (plugin.gui_open && isLaunching) {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  }, [plugin.gui_open, isLaunching]);

  useEffect(() => () => {
    if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
  }, []);

  useEffect(() => {
    if (isRenaming) {
      setTimeout(() => renameInputRef.current?.focus(), 0);
    }
  }, [isRenaming]);

  // Check crash status periodically
  useEffect(() => {
    const checkStatus = async () => {
      try {
        const status = await tauri.getPluginCrashStatus(plugin.instance_id);
        setCrashStatus(status);
      } catch { /* silent */ }
    };
    checkStatus();
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

  const handleLaunch = async () => {
    if (isLaunching) return;
    if (plugin.gui_open) return; // already open — do nothing
    setIsLaunching(true);
    // Safety fallback: clear spinner after 8 s if gui_open never becomes true
    launchTimerRef.current = setTimeout(() => setIsLaunching(false), 8000);
    try {
      await onLaunch?.();
    } catch {
      setIsLaunching(false);
      if (launchTimerRef.current) clearTimeout(launchTimerRef.current);
    }
  };

  const handleRenameConfirm = async () => {
    const trimmed = editName.trim();
    if (trimmed && trimmed !== plugin.name) {
      try {
        await tauri.renamePlugin(plugin.instance_id, trimmed);
      } catch (err) {
        message.error(`Rename failed: ${err}`);
        setEditName(plugin.name);
      }
    } else if (!trimmed) {
      setEditName(plugin.name);
    }
    setIsRenaming(false);
  };

  const handleRenameCancel = () => {
    setEditName(plugin.name);
    setIsRenaming(false);
  };

  const isCrashed = crashStatus.type !== 'Ok';

  // Determine launch button appearance
  const launchButtonProps = (() => {
    if (plugin.format === 'builtin') {
      return { icon: <SettingOutlined />, label: 'Settings', onClick: () => setShowBuiltinGui(true) };
    }
    if (plugin.gui_open) {
      return { icon: <CheckCircleOutlined />, label: 'Open', onClick: () => {} };
    }
    if (isLaunching) {
      return { icon: <LoadingOutlined />, label: 'Launching', onClick: () => {} };
    }
    return { icon: <PlayCircleOutlined />, label: 'Launch', onClick: handleLaunch };
  })();

  return (
    <>
    <Card
      size="small"
      className={`transition-all shadow-lg ${plugin.bypassed ? 'opacity-60' : ''}`}
      style={{
        borderWidth: 2,
        borderColor: isCrashed
          ? token.colorError
          : plugin.gui_open
          ? token.colorSuccess
          : plugin.bypassed
          ? token.colorBorderSecondary
          : token.colorPrimary,
        background: token.colorBgContainer,
      }}
      styles={{ body: { padding: '12px 14px' } }}
    >
      <Space direction="vertical" size={6} style={{ width: '100%' }}>

        {/* ── Name row ────────────────────────────────────────────── */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          {isRenaming ? (
            <Input
              ref={renameInputRef}
              size="small"
              value={editName}
              onChange={e => setEditName(e.target.value)}
              onPressEnter={handleRenameConfirm}
              onKeyDown={e => e.key === 'Escape' && handleRenameCancel()}
              style={{ flex: 1, fontWeight: 600, fontSize: 14 }}
              suffix={
                <Space size={2}>
                  <CheckOutlined
                    style={{ color: token.colorSuccess, cursor: 'pointer' }}
                    onClick={handleRenameConfirm}
                  />
                  <CloseCircleOutlined
                    style={{ color: token.colorTextSecondary, cursor: 'pointer' }}
                    onClick={handleRenameCancel}
                  />
                </Space>
              }
            />
          ) : (
            <>
              <span style={{
                fontWeight: 600,
                fontSize: 14,
                color: token.colorText,
                flex: 1,
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
              }}>
                {plugin.name}
              </span>
              <Tooltip title="Rename">
                <EditOutlined
                  style={{ fontSize: 12, color: token.colorTextQuaternary, cursor: 'pointer', flexShrink: 0 }}
                  onClick={() => { setEditName(plugin.name); setIsRenaming(true); }}
                />
              </Tooltip>
            </>
          )}
        </div>

        {/* ── Meta tags ───────────────────────────────────────────── */}
        <Space size={4} wrap style={{ lineHeight: 1 }}>
          <Tag color={getFormatColor(plugin.format)} style={{ fontSize: 10, margin: 0 }}>
            {plugin.format.toUpperCase()}
          </Tag>
          {plugin.manufacture && (
            <Tag color="default" style={{ fontSize: 10, margin: 0 }}>
              {plugin.manufacture}
            </Tag>
          )}
          {plugin.version && (
            <Tag color="default" style={{ fontSize: 10, margin: 0, opacity: 0.7 }}>
              v{plugin.version}
            </Tag>
          )}
          {plugin.category && plugin.category !== 'Unknown' && (
            <Tag color="geekblue" style={{ fontSize: 10, margin: 0 }}>
              {plugin.category}
            </Tag>
          )}
        </Space>

        {/* ── Crash status ────────────────────────────────────────── */}
        {isCrashed && (
          <Tag
            icon={<WarningOutlined />}
            color={crashStatus.type === 'Crashed' ? 'error' : 'warning'}
            style={{ fontSize: 10 }}
          >
            {crashStatus.type === 'Timeout' ? 'TIMEOUT' : `${crashStatus.type}: ${crashStatus.data ?? ''}`}
          </Tag>
        )}

        {/* ── Action Buttons ──────────────────────────────────────── */}
        <Space style={{ width: '100%' }} size="small">
          {isCrashed && (
            <Tooltip title="Reset Crash Protection">
              <Button
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
              type="primary"
              size="small"
              icon={<PoweroffOutlined />}
              onClick={onToggleBypass}
              style={{
                flex: 1,
                ...(plugin.bypassed
                  ? { background: 'transparent', borderColor: token.colorBorder, color: token.colorTextSecondary }
                  : { background: token.colorSuccess, borderColor: token.colorSuccess, color: '#fff' }),
              }}
              disabled={isCrashed}
            >
              {plugin.bypassed ? 'Bypassed' : 'Active'}
            </Button>
          </Tooltip>

          <Tooltip title={plugin.gui_open ? 'GUI already open' : launchButtonProps.label}>
            <Button
              type={plugin.gui_open ? 'default' : 'default'}
              size="small"
              icon={launchButtonProps.icon}
              onClick={launchButtonProps.onClick}
              disabled={isCrashed || isLaunching}
              style={{
                flex: 1,
                color: plugin.gui_open ? token.colorSuccess : undefined,
                borderColor: plugin.gui_open ? token.colorSuccess : undefined,
              }}
            >
              {launchButtonProps.label}
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

    {plugin.format === 'builtin' && plugin.plugin_id === 'builtin::noise_suppressor' && (
      <NoiseSuppressorGui
        plugin={plugin}
        isOpen={showBuiltinGui}
        onClose={() => setShowBuiltinGui(false)}
      />
    )}
    {plugin.format === 'builtin' && plugin.plugin_id === 'builtin::compressor' && (
      <CompressorGui
        plugin={plugin}
        isOpen={showBuiltinGui}
        onClose={() => setShowBuiltinGui(false)}
      />
    )}
  </>
  );
}
