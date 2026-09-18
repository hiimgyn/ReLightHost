import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { FunctionOutlined, UndoOutlined, SlidersOutlined } from '@ant-design/icons';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';

const { Text } = Typography;

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

// Parameter IDs match builtin/compressor.rs
const P_THRESHOLD = 0;
const P_RATIO     = 1;
const P_ATTACK    = 2;
const P_RELEASE   = 3;
const P_MAKEUP    = 4;
const P_KNEE      = 5;
const P_MIX       = 6;

function paramValue(plugin: PluginInstanceInfo, id: number, fallback: number) {
  return plugin.parameters.find(p => p.id === id)?.value ?? fallback;
}

// ── Extracted as module-level component so React never remounts the
// Slider on parent re-renders (which would break ongoing drag gestures).
interface ParamRowProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  leftLabel: string;
  rightLabel: string;
  defaultValue: number;
  primaryColor: string;
  tertiaryColor: string;
  onChange: (v: number) => void;
}

function ParamRow({
  label, value, min, max, step,
  format, leftLabel, rightLabel,
  defaultValue, primaryColor, tertiaryColor, onChange,
}: ParamRowProps) {
  return (
    <div
      className="minimal-surface"
      style={{
        padding: '8px 10px',
        borderRadius: 8,
        background: 'var(--rh-surface-soft-gradient)',
        border: '1px solid var(--rh-surface-soft-border)',
        transition: 'border-color 160ms ease, box-shadow 160ms ease',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
        <Text style={{ fontSize: 13 }}>{label}</Text>
        <Space size={6} align="center">
          {value !== defaultValue && (
            <Tooltip title="Reset to default">
              <UndoOutlined
                style={{ fontSize: 11, cursor: 'pointer', color: tertiaryColor }}
                onClick={() => onChange(defaultValue)}
              />
            </Tooltip>
          )}
          <Text type="secondary" style={{ fontSize: 13, fontVariantNumeric: 'tabular-nums' }}>
            {format(value)}
          </Text>
        </Space>
      </div>
      <Slider
        min={min} max={max} step={step} value={value}
        onChange={onChange}
        tooltip={{ formatter: (v) => format(v ?? min) }}
        trackStyle={{ background: primaryColor }}
        handleStyle={{ borderColor: primaryColor }}
      />
      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
        <Text type="secondary" style={{ fontSize: 11 }}>{leftLabel}</Text>
        <Text type="secondary" style={{ fontSize: 11 }}>{rightLabel}</Text>
      </div>
    </div>
  );
}

export default function CompressorGui({ plugin, isOpen, onClose }: Props) {
  const { token } = theme.useToken();
  const pc = token.colorPrimary;
  const tc = token.colorTextTertiary;
  const modalWidth = typeof window === 'undefined' ? 340 : 'clamp(320px, 32vw, 360px)';

  const [threshold, setThreshold] = useState(() => paramValue(plugin, P_THRESHOLD, -18));
  const [ratio,     setRatio]     = useState(() => paramValue(plugin, P_RATIO,       4));
  const [attack,    setAttack]    = useState(() => paramValue(plugin, P_ATTACK,     10));
  const [release,   setRelease]   = useState(() => paramValue(plugin, P_RELEASE,   100));
  const [makeup,    setMakeup]    = useState(() => paramValue(plugin, P_MAKEUP,      0));
  const [knee,      setKnee]      = useState(() => paramValue(plugin, P_KNEE,        3));
  const [mix,       setMix]       = useState(() => paramValue(plugin, P_MIX,         1));

  // Trailing throttle for IPC parameter calls (max 1 call per 40ms with immediate 1st tick)
  const pendingSendsRef = useRef<Map<number, number>>(new Map());
  const throttleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const send = useCallback((id: number, value: number) => {
    pendingSendsRef.current.set(id, value);
    if (!throttleTimerRef.current) {
      tauri.setPluginParameter(plugin.instance_id, id, value).catch(() => {});
      throttleTimerRef.current = setTimeout(() => {
        throttleTimerRef.current = null;
        pendingSendsRef.current.forEach((val, pId) => {
          tauri.setPluginParameter(plugin.instance_id, pId, val).catch(() => {});
        });
        pendingSendsRef.current.clear();
      }, 40);
    }
  }, [plugin.instance_id]);

  useEffect(() => {
    return () => {
      if (throttleTimerRef.current) {
        clearTimeout(throttleTimerRef.current);
      }
    };
  }, []);

  // Authoritative re-sync when opening the modal
  useEffect(() => {
    if (!isOpen) return;
    let isCancelled = false;

    (async () => {
      try {
        const params = await tauri.getPluginParameters(plugin.instance_id);
        if (isCancelled || !params || params.length === 0) return;
        const find = (id: number, fallback: number) =>
          params.find(p => p.id === id)?.value ?? paramValue(plugin, id, fallback);

        setThreshold(find(P_THRESHOLD, -18));
        setRatio(find(P_RATIO, 4));
        setAttack(find(P_ATTACK, 10));
        setRelease(find(P_RELEASE, 100));
        setMakeup(find(P_MAKEUP, 0));
        setKnee(find(P_KNEE, 3));
        setMix(find(P_MIX, 1));
      } catch {
        if (isCancelled) return;
        setThreshold(paramValue(plugin, P_THRESHOLD, -18));
        setRatio(paramValue(plugin, P_RATIO, 4));
        setAttack(paramValue(plugin, P_ATTACK, 10));
        setRelease(paramValue(plugin, P_RELEASE, 100));
        setMakeup(paramValue(plugin, P_MAKEUP, 0));
        setKnee(paramValue(plugin, P_KNEE, 3));
        setMix(paramValue(plugin, P_MIX, 1));
      }
    })();

    return () => {
      isCancelled = true;
    };
  }, [isOpen, plugin.instance_id]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <Modal
      title={
        <Space>
          <FunctionOutlined style={{ color: token.colorPrimary }} />
          <span>Compressor</span>
          <Badge color="cyan" text="Built-in" />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={modalWidth}
      style={{ top: 16, maxWidth: 360 }}
      styles={{ body: { maxHeight: 'calc(100vh - 160px)', overflowY: 'auto', overflowX: 'hidden', padding: '14px 16px 16px' } }}
    >
      <Space orientation="vertical" size={14} style={{ width: '100%' }}>

        {/* ── Signature Visual Dynamic Badge ──────────────────── */}
        <div
          className="minimal-surface"
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '10px 14px',
            borderRadius: 10,
            background: 'linear-gradient(135deg, rgba(99,103,255,0.12) 0%, rgba(132,148,255,0.06) 100%)',
            border: `1px solid ${token.colorPrimary}44`,
          }}
        >
          <Space size={8} align="center">
            <SlidersOutlined style={{ color: token.colorPrimary, fontSize: 16 }} />
            <div>
              <div style={{ fontSize: 11, fontWeight: 700, letterSpacing: 0.8, textTransform: 'uppercase', color: token.colorPrimary }}>
                Dynamics Profile
              </div>
              <Text type="secondary" style={{ fontSize: 11 }}>
                {ratio >= 10 ? 'Limiting / Brickwall' : ratio >= 4 ? 'Standard Compression' : ratio > 1 ? 'Gentle Leveling' : 'Linear (1:1)'}
              </Text>
            </div>
          </Space>
          <div style={{ textAlign: 'right' }}>
            <div style={{ fontSize: 13, fontWeight: 700, fontFamily: 'monospace', color: token.colorText }}>
              {threshold.toFixed(1)} dB
            </div>
            <div style={{ fontSize: 10, color: token.colorTextTertiary }}>
              Ratio {ratio.toFixed(1)}:1
            </div>
          </div>
        </div>

        {/* ── Dynamics ──────────────────────────────────────── */}
        <ParamRow
          label="Threshold" value={threshold} defaultValue={-18} min={-60} max={0} step={0.5}
          format={v => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`}
          leftLabel="-60 dB (always)" rightLabel="0 dB (never)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setThreshold(v); send(P_THRESHOLD, v); }}
        />

        <ParamRow
          label="Ratio" value={ratio} defaultValue={4} min={1} max={20} step={0.1}
          format={v => `${v.toFixed(1)} : 1`}
          leftLabel="1:1 (transparent)" rightLabel="20:1 (limiting)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setRatio(v); send(P_RATIO, v); }}
        />

        {/* ── Timing ────────────────────────────────────────── */}
        <ParamRow
          label="Attack" value={attack} defaultValue={10} min={0.1} max={200} step={0.1}
          format={v => v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`}
          leftLabel="0.1 ms (fast punch)" rightLabel="200 ms (slow transient)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setAttack(v); send(P_ATTACK, v); }}
        />

        <ParamRow
          label="Release" value={release} defaultValue={100} min={10} max={2000} step={1}
          format={v => v < 1000 ? `${Math.round(v)} ms` : `${(v / 1000).toFixed(2)} s`}
          leftLabel="10 ms (fast)" rightLabel="2000 ms (smooth)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setRelease(v); send(P_RELEASE, v); }}
        />

        {/* ── Output ────────────────────────────────────────── */}
        <ParamRow
          label="Makeup Gain" value={makeup} defaultValue={0} min={0} max={30} step={0.5}
          format={v => `+${v.toFixed(1)} dB`}
          leftLabel="0 dB" rightLabel="+30 dB"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setMakeup(v); send(P_MAKEUP, v); }}
        />

        <ParamRow
          label="Knee" value={knee} defaultValue={3} min={0} max={12} step={0.5}
          format={v => `${v.toFixed(1)} dB`}
          leftLabel="0 dB (hard)" rightLabel="12 dB (soft curve)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setKnee(v); send(P_KNEE, v); }}
        />

        <ParamRow
          label="Parallel Mix" value={mix} defaultValue={1} min={0} max={1} step={0.01}
          format={v => `${Math.round(v * 100)}%`}
          leftLabel="0% (dry only)" rightLabel="100% (wet)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setMix(v); send(P_MIX, v); }}
        />

      </Space>
    </Modal>
  );
}