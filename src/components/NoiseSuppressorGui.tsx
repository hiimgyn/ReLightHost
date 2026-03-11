import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Divider, Badge, Collapse, Tooltip, theme } from 'antd';
import { AudioOutlined, SoundOutlined, SettingOutlined, UndoOutlined } from '@ant-design/icons';
import * as tauri from '../lib/tauri';
import type { PluginInstanceInfo } from '../lib/types';

const { Text } = Typography;

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

// How many VAD history bars to display in the meter strip
const VAD_HISTORY = 40;

export default function NoiseSuppressorGui({ plugin, isOpen, onClose }: Props) {
  const { token } = theme.useToken();

  const mixParam          = plugin.parameters.find(p => p.id === 0);
  const vadGateParam      = plugin.parameters.find(p => p.id === 1);
  const gateAttenParam    = plugin.parameters.find(p => p.id === 2);
  const outputGainParam   = plugin.parameters.find(p => p.id === 3);

  const [mix, setMix]               = useState<number>(mixParam?.value       ?? 1.0);
  const [vadGate, setVadGate]       = useState<number>(vadGateParam?.value   ?? 0.0);
  const [gateAtten, setGateAtten]   = useState<number>(gateAttenParam?.value ?? 0.0);
  const [outputGain, setOutputGain] = useState<number>(outputGainParam?.value ?? 0.0);
  const [vad, setVad]     = useState<number>(0);
  const [history, setHistory] = useState<number[]>(Array(VAD_HISTORY).fill(0));
  const rafRef = useRef<number | null>(null);
  const mountedRef = useRef(false);

  // Sync params from plugin when the panel reopens
  useEffect(() => {
    setMix(plugin.parameters.find(p => p.id === 0)?.value       ?? 1.0);
    setVadGate(plugin.parameters.find(p => p.id === 1)?.value   ?? 0.0);
    setGateAtten(plugin.parameters.find(p => p.id === 2)?.value ?? 0.0);
    setOutputGain(plugin.parameters.find(p => p.id === 3)?.value ?? 0.0);
  }, [plugin.parameters, isOpen]);

  // Poll VAD at ~20 fps while open
  const pollVad = useCallback(async () => {
    if (!mountedRef.current) return;
    try {
      const v = await tauri.getNoiseSuppressorVad(plugin.instance_id);
      setVad(v);
      setHistory(prev => [...prev.slice(1), v]);
    } catch { /* instance removed mid-flight */ }
    if (mountedRef.current) {
      rafRef.current = window.setTimeout(pollVad, 50);
    }
  }, [plugin.instance_id]);

  useEffect(() => {
    if (isOpen) {
      mountedRef.current = true;
      pollVad();
    }
    return () => {
      mountedRef.current = false;
      if (rafRef.current !== null) clearTimeout(rafRef.current);
    };
  }, [isOpen, pollVad]);

  const handleMixChange = async (value: number) => {
    setMix(value);
    try { await tauri.setPluginParameter(plugin.instance_id, 0, value); } catch { /* removed */ }
  };

  const handleVadGateChange = async (value: number) => {
    setVadGate(value);
    try { await tauri.setPluginParameter(plugin.instance_id, 1, value); } catch { /* removed */ }
  };

  const handleGateAttenChange = async (value: number) => {
    setGateAtten(value);
    try { await tauri.setPluginParameter(plugin.instance_id, 2, value); } catch { /* removed */ }
  };

  const handleOutputGainChange = async (value: number) => {
    setOutputGain(value);
    try { await tauri.setPluginParameter(plugin.instance_id, 3, value); } catch { /* removed */ }
  };

  // VAD colour thresholds
  const vadColor   = vad > 0.65 ? token.colorSuccess : vad > 0.35 ? token.colorWarning : token.colorTextQuaternary;
  const vadLabel   = vad > 0.65 ? 'Voice detected' : vad > 0.35 ? 'Uncertain' : 'Background noise';
  const vadPercent = Math.round(vad * 100);

  return (
    <Modal
      title={
        <Space>
          <AudioOutlined style={{ color: token.colorPrimary }} />
          <span>Noise Suppressor (RNNoise)</span>
          <Badge color="cyan" text="Built-in" />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={420}
      styles={{ body: { padding: '20px 24px 24px' } }}
    >
      <Space direction="vertical" size="large" style={{ width: '100%' }}>

        {/* ── Voice Activity Meter ───────────────────────────── */}
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 8 }}>
            <Text type="secondary" style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1 }}>
              Voice Activity
            </Text>
            <Text style={{ fontSize: 12, color: vadColor, fontVariantNumeric: 'tabular-nums' }}>
              {vadPercent}% — {vadLabel}
            </Text>
          </div>

          {/* History strip */}
          <div
            style={{
              display: 'flex',
              alignItems: 'flex-end',
              gap: 2,
              height: 48,
              padding: '4px 6px',
              background: token.colorFillQuaternary,
              borderRadius: token.borderRadiusSM,
              overflow: 'hidden',
            }}
          >
            {history.map((v, i) => {
              const barColor = v > 0.65 ? token.colorSuccess : v > 0.35 ? token.colorWarning : token.colorFillSecondary;
              return (
                <div
                  key={i}
                  style={{
                    flex: 1,
                    height: `${Math.max(4, v * 100)}%`,
                    background: barColor,
                    borderRadius: 2,
                    transition: 'height 0.05s ease-out',
                    opacity: 0.6 + (i / VAD_HISTORY) * 0.4,
                  }}
                />
              );
            })}
          </div>

          {/* Big live badge */}
          <div style={{ display: 'flex', justifyContent: 'center', marginTop: 10 }}>
            <div
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 8,
                padding: '6px 16px',
                background: token.colorFillTertiary,
                borderRadius: token.borderRadiusLG,
                border: `1px solid ${vadColor}44`,
              }}
            >
              <div
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: '50%',
                  background: vadColor,
                  boxShadow: vad > 0.35 ? `0 0 8px ${vadColor}` : 'none',
                  transition: 'background 0.1s, box-shadow 0.1s',
                }}
              />
              <Text style={{ color: vadColor, fontSize: 13, fontWeight: 500 }}>
                {vadLabel}
              </Text>
            </div>
          </div>
        </div>

        <Divider style={{ margin: '0' }} />

        {/* ── Mix Control ────────────────────────────────────── */}
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
            <Space>
              <SoundOutlined style={{ color: token.colorPrimary }} />
              <Text strong>Noise Reduction Mix</Text>
            </Space>
            <Space size={6} align="center">
              {mix !== 1.0 && (
                <Tooltip title="Reset to default">
                  <UndoOutlined
                    style={{ fontSize: 11, cursor: 'pointer', color: token.colorTextTertiary }}
                    onClick={() => handleMixChange(1.0)}
                  />
                </Tooltip>
              )}
              <Text type="secondary" style={{ fontVariantNumeric: 'tabular-nums' }}>
                {Math.round(mix * 100)}%
              </Text>
            </Space>
          </div>

          <Slider
            min={0}
            max={1}
            step={0.01}
            value={mix}
            onChange={handleMixChange}
            tooltip={{ formatter: (v) => `${Math.round((v ?? 0) * 100)}%` }}
            trackStyle={{ background: token.colorPrimary }}
            handleStyle={{ borderColor: token.colorPrimary }}
          />

          <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 4 }}>
            <Text type="secondary" style={{ fontSize: 11 }}>Dry (pass-through)</Text>
            <Text type="secondary" style={{ fontSize: 11 }}>Full noise reduction</Text>
          </div>
        </div>

        {/* ── Info footer ────────────────────────────────────── */}
        <div
          style={{
            padding: '8px 12px',
            background: token.colorFillQuaternary,
            borderRadius: token.borderRadiusSM,
            lineHeight: 1.6,
          }}
        >
          <Text type="secondary" style={{ fontSize: 11 }}>
            Powered by <strong>RNNoise</strong> — a recurrent neural network trained for real-time
            speech enhancement. No external files required.
          </Text>
        </div>

        {/* ── Advanced ───────────────────────────────────────── */}
        <Collapse
          ghost
          items={[{
            key: 'advanced',
            label: (
              <Space size={6}>
                <SettingOutlined style={{ color: token.colorTextSecondary }} />
                <Text style={{ fontSize: 13, color: token.colorTextSecondary }}>Advanced</Text>
              </Space>
            ),
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%', paddingTop: 4 }}>

                {/* VAD Gate Threshold */}
                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                    <Text style={{ fontSize: 12 }}>VAD Gate Threshold</Text>
                    <Space size={6} align="center">
                      {vadGate !== 0.0 && (
                        <Tooltip title="Reset to default">
                          <UndoOutlined
                            style={{ fontSize: 11, cursor: 'pointer', color: token.colorTextTertiary }}
                            onClick={() => handleVadGateChange(0.0)}
                          />
                        </Tooltip>
                      )}
                      <Text type="secondary" style={{ fontSize: 12, fontVariantNumeric: 'tabular-nums' }}>
                        {vadGate === 0 ? 'Off' : `${Math.round(vadGate * 100)}%`}
                      </Text>
                    </Space>
                  </div>
                  <Slider min={0} max={1} step={0.01} value={vadGate} onChange={handleVadGateChange}
                    tooltip={{ formatter: (v) => v === 0 ? 'Off' : `${Math.round((v ?? 0) * 100)}%` }}
                    trackStyle={{ background: token.colorPrimary }} handleStyle={{ borderColor: token.colorPrimary }} />
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
                    <Text type="secondary" style={{ fontSize: 11 }}>Off (no gating)</Text>
                    <Text type="secondary" style={{ fontSize: 11 }}>Gate all non-speech</Text>
                  </div>
                </div>

                {/* Gate Attenuation */}
                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                    <Text style={{ fontSize: 12 }}>Gate Attenuation</Text>
                    <Space size={6} align="center">
                      {gateAtten !== 0.0 && (
                        <Tooltip title="Reset to default">
                          <UndoOutlined
                            style={{ fontSize: 11, cursor: 'pointer', color: token.colorTextTertiary }}
                            onClick={() => handleGateAttenChange(0.0)}
                          />
                        </Tooltip>
                      )}
                      <Text type="secondary" style={{ fontSize: 12, fontVariantNumeric: 'tabular-nums' }}>
                        {Math.round(gateAtten * 100)}%
                      </Text>
                    </Space>
                  </div>
                  <Slider min={0} max={1} step={0.01} value={gateAtten} onChange={handleGateAttenChange}
                    tooltip={{ formatter: (v) => `${Math.round((v ?? 0) * 100)}%` }}
                    trackStyle={{ background: token.colorPrimary }} handleStyle={{ borderColor: token.colorPrimary }} />
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
                    <Text type="secondary" style={{ fontSize: 11 }}>No reduction</Text>
                    <Text type="secondary" style={{ fontSize: 11 }}>Full silence</Text>
                  </div>
                </div>

                {/* Output Gain */}
                <div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                    <Text style={{ fontSize: 12 }}>Output Gain</Text>
                    <Space size={6} align="center">
                      {outputGain !== 0.0 && (
                        <Tooltip title="Reset to default">
                          <UndoOutlined
                            style={{ fontSize: 11, cursor: 'pointer', color: token.colorTextTertiary }}
                            onClick={() => handleOutputGainChange(0.0)}
                          />
                        </Tooltip>
                      )}
                      <Text type="secondary" style={{ fontSize: 12, fontVariantNumeric: 'tabular-nums' }}>
                      {outputGain >= 0 ? '+' : ''}{outputGain.toFixed(1)} dB
                      </Text>
                    </Space>
                  </div>
                  <Slider min={-24} max={12} step={0.5} value={outputGain} onChange={handleOutputGainChange}
                    tooltip={{ formatter: (v) => `${(v ?? 0) >= 0 ? '+' : ''}${(v ?? 0).toFixed(1)} dB` }}
                    trackStyle={{ background: token.colorPrimary }} handleStyle={{ borderColor: token.colorPrimary }} />
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
                    <Text type="secondary" style={{ fontSize: 11 }}>-24 dB</Text>
                    <Text type="secondary" style={{ fontSize: 11 }}>+12 dB</Text>
                  </div>
                </div>

              </Space>
            ),
          }]}
        />

      </Space>
    </Modal>
  );
}
