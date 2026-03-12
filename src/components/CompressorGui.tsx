import { useState, useEffect } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { FunctionOutlined, UndoOutlined } from '@ant-design/icons';
import * as tauri from '../lib/tauri';
import type { PluginInstanceInfo } from '../lib/types';

const { Text } = Typography;

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

// Parameter IDs match builtin/compressor.rs
const P_THRESHOLD  = 0;
const P_RATIO      = 1;
const P_ATTACK     = 2;
const P_RELEASE    = 3;
const P_MAKEUP     = 4;
const P_KNEE       = 5;
const P_MIX        = 6;

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
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
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

  const [threshold,  setThreshold]  = useState(() => paramValue(plugin, P_THRESHOLD, -18));
  const [ratio,      setRatio]      = useState(() => paramValue(plugin, P_RATIO,       4));
  const [attack,     setAttack]     = useState(() => paramValue(plugin, P_ATTACK,     10));
  const [release,    setRelease]    = useState(() => paramValue(plugin, P_RELEASE,   100));
  const [makeup,     setMakeup]     = useState(() => paramValue(plugin, P_MAKEUP,      0));
  const [knee,       setKnee]       = useState(() => paramValue(plugin, P_KNEE,        3));
  const [mix,        setMix]        = useState(() => paramValue(plugin, P_MIX,         1));

  // Re-sync state when the modal reopens (e.g. preset loaded)
  useEffect(() => {
    if (!isOpen) return;
    setThreshold(paramValue(plugin, P_THRESHOLD, -18));
    setRatio(    paramValue(plugin, P_RATIO,       4));
    setAttack(   paramValue(plugin, P_ATTACK,     10));
    setRelease(  paramValue(plugin, P_RELEASE,   100));
    setMakeup(   paramValue(plugin, P_MAKEUP,      0));
    setKnee(     paramValue(plugin, P_KNEE,        3));
    setMix(      paramValue(plugin, P_MIX,         1));
  }, [plugin.parameters, isOpen]); // eslint-disable-line

  const send = async (id: number, value: number) => {
    try { await tauri.setPluginParameter(plugin.instance_id, id, value); } catch { /* removed */ }
  };

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
      width={440}
      styles={{ body: { padding: '20px 24px 24px' } }}
    >
      <Space direction="vertical" size="large" style={{ width: '100%' }}>

        {/* ── Dynamics ──────────────────────────────────────── */}
        <ParamRow
          label="Threshold" value={threshold} defaultValue={-18} min={-60} max={0} step={0.5}
          format={v => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`}
          leftLabel="-60 dB (always compress)" rightLabel="0 dB (never compress)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setThreshold(v); send(P_THRESHOLD, v); }}
        />

        <ParamRow
          label="Ratio" value={ratio} defaultValue={4} min={1} max={20} step={0.1}
          format={v => `${v.toFixed(1)} : 1`}
          leftLabel="1:1 (no compression)" rightLabel="20:1 (limiting)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setRatio(v); send(P_RATIO, v); }}
        />

        {/* ── Timing ────────────────────────────────────────── */}
        <ParamRow
          label="Attack" value={attack} defaultValue={10} min={0.1} max={200} step={0.1}
          format={v => v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`}
          leftLabel="0.1 ms (fast)" rightLabel="200 ms (slow)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setAttack(v); send(P_ATTACK, v); }}
        />

        <ParamRow
          label="Release" value={release} defaultValue={100} min={10} max={2000} step={1}
          format={v => v < 1000 ? `${Math.round(v)} ms` : `${(v / 1000).toFixed(2)} s`}
          leftLabel="10 ms (fast)" rightLabel="2000 ms (slow)"
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
          leftLabel="0 dB (hard)" rightLabel="12 dB (very soft)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setKnee(v); send(P_KNEE, v); }}
        />

        <ParamRow
          label="Parallel Mix" value={mix} defaultValue={1} min={0} max={1} step={0.01}
          format={v => `${Math.round(v * 100)}%`}
          leftLabel="0% (dry only)" rightLabel="100% (fully compressed)"
          primaryColor={pc} tertiaryColor={tc}
          onChange={v => { setMix(v); send(P_MIX, v); }}
        />

      </Space>
    </Modal>
  );
}