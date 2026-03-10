import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Divider, Badge, theme } from 'antd';
import { AudioOutlined, SoundOutlined } from '@ant-design/icons';
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

  const mixParam = plugin.parameters.find(p => p.id === 0);
  const [mix, setMix]     = useState<number>(mixParam?.value ?? 1.0);
  const [vad, setVad]     = useState<number>(0);
  const [history, setHistory] = useState<number[]>(Array(VAD_HISTORY).fill(0));
  const rafRef = useRef<number | null>(null);
  const mountedRef = useRef(false);

  // Sync mix from plugin params when the panel reopens
  useEffect(() => {
    const v = plugin.parameters.find(p => p.id === 0)?.value ?? 1.0;
    setMix(v);
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
    try {
      await tauri.setPluginParameter(plugin.instance_id, 0, value);
    } catch { /* plugin removed */ }
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
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 12 }}>
            <Space>
              <SoundOutlined style={{ color: token.colorPrimary }} />
              <Text strong>Noise Reduction Mix</Text>
            </Space>
            <Text type="secondary" style={{ fontVariantNumeric: 'tabular-nums' }}>
              {Math.round(mix * 100)}%
            </Text>
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

      </Space>
    </Modal>
  );
}
