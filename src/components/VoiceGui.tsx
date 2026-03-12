import { useState, useEffect } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { CustomerServiceOutlined, UndoOutlined } from '@ant-design/icons';
import * as tauri from '../lib/tauri';
import type { PluginInstanceInfo } from '../lib/types';

const { Text } = Typography;

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

// Parameter IDs — must match builtin/voice.rs
const P_LOW     = 0;
const P_MID     = 1;
const P_HIGH    = 2;
const P_DRIVE   = 3;
const P_WIDTH   = 4;
const P_CEILING = 5;

function paramValue(plugin: PluginInstanceInfo, id: number, fallback: number) {
  return plugin.parameters.find(p => p.id === id)?.value ?? fallback;
}

// ── Section header ───────────────────────────────────────────────────────────
// Module-level component — stable reference prevents Slider remounting on drag.

interface SectionProps { title: string; color: string; }
function SectionHeader({ title, color }: SectionProps) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
      <span style={{ fontSize: 10, letterSpacing: 2, color, textTransform: 'uppercase', fontWeight: 700 }}>
        {title}
      </span>
      <div style={{ flex: 1, height: 1, background: `${color}30` }} />
    </div>
  );
}

// ── Generic param row ────────────────────────────────────────────────────────

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
  color: string;
  tertiaryColor: string;
  onChange: (v: number) => void;
}

function ParamRow({
  label, value, min, max, step,
  format, leftLabel, rightLabel,
  defaultValue, color, tertiaryColor, onChange,
}: ParamRowProps) {
  return (
    <div>
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
        trackStyle={{ background: color }}
        handleStyle={{ borderColor: color }}
      />
      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
        <Text type="secondary" style={{ fontSize: 11 }}>{leftLabel}</Text>
        <Text type="secondary" style={{ fontSize: 11 }}>{rightLabel}</Text>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

export default function VoiceGui({ plugin, isOpen, onClose }: Props) {
  const { token } = theme.useToken();
  const tc = token.colorTextTertiary;

  const [low,     setLow]     = useState(() => paramValue(plugin, P_LOW,      0));
  const [mid,     setMid]     = useState(() => paramValue(plugin, P_MID,      0));
  const [high,    setHigh]    = useState(() => paramValue(plugin, P_HIGH,     0));
  const [drive,   setDrive]   = useState(() => paramValue(plugin, P_DRIVE,    0));
  const [width,   setWidth]   = useState(() => paramValue(plugin, P_WIDTH,    0));
  const [ceiling, setCeiling] = useState(() => paramValue(plugin, P_CEILING,  0));

  useEffect(() => {
    if (!isOpen) return;
    setLow(    paramValue(plugin, P_LOW,     0));
    setMid(    paramValue(plugin, P_MID,     0));
    setHigh(   paramValue(plugin, P_HIGH,    0));
    setDrive(  paramValue(plugin, P_DRIVE,   0));
    setWidth(  paramValue(plugin, P_WIDTH,   0));
    setCeiling(paramValue(plugin, P_CEILING, 0));
  }, [plugin.parameters, isOpen]); // eslint-disable-line

  const send = (id: number, value: number) =>
    tauri.setPluginParameter(plugin.instance_id, id, value).catch(() => {});

  const fmtDb  = (v: number) => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`;
  const fmtPct = (v: number) => `${Math.round(v * 100)}%`;

  // Section accent colors
  const EQ_COLOR  = '#22aacc';
  const SAT_COLOR = '#cc8822';
  const DBL_COLOR = '#22cc77';
  const LIM_COLOR = '#cc3355';

  return (
    <Modal
      title={
        <Space>
          <CustomerServiceOutlined style={{ color: token.colorPrimary }} />
          <span>Voice Designer</span>
          <Badge color="cyan" text="Built-in" />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={460}
      styles={{ body: { padding: '16px 24px 24px' } }}
    >
      <Space direction="vertical" size={20} style={{ width: '100%' }}>

        {/* ── EQ ───────────────────────────────────────────── */}
        <div>
          <SectionHeader title="EQ" color={EQ_COLOR} />
          <Space direction="vertical" size={12} style={{ width: '100%' }}>
            <ParamRow
              label="Low" value={low} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel="−12 dB (cut bass)" rightLabel="+12 dB (boost bass)"
              color={EQ_COLOR} tertiaryColor={tc}
              onChange={v => { setLow(v); send(P_LOW, v); }}
            />
            <ParamRow
              label="Mid" value={mid} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel="−12 dB (thin)" rightLabel="+12 dB (body)"
              color={EQ_COLOR} tertiaryColor={tc}
              onChange={v => { setMid(v); send(P_MID, v); }}
            />
            <ParamRow
              label="High" value={high} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel="−12 dB (dark)" rightLabel="+12 dB (bright / air)"
              color={EQ_COLOR} tertiaryColor={tc}
              onChange={v => { setHigh(v); send(P_HIGH, v); }}
            />
          </Space>
        </div>

        {/* ── Saturation ───────────────────────────────────── */}
        <div>
          <SectionHeader title="Saturation" color={SAT_COLOR} />
          <ParamRow
            label="Drive" value={drive} defaultValue={0} min={0} max={1} step={0.01}
            format={fmtPct}
            leftLabel="0% (clean)" rightLabel="100% (saturated)"
            color={SAT_COLOR} tertiaryColor={tc}
            onChange={v => { setDrive(v); send(P_DRIVE, v); }}
          />
        </div>

        {/* ── Doubler ──────────────────────────────────────── */}
        <div>
          <SectionHeader title="Doubler" color={DBL_COLOR} />
          <ParamRow
            label="Width" value={width} defaultValue={0} min={0} max={1} step={0.01}
            format={fmtPct}
            leftLabel="0% (mono)" rightLabel="100% (wide stereo)"
            color={DBL_COLOR} tertiaryColor={tc}
            onChange={v => { setWidth(v); send(P_WIDTH, v); }}
          />
        </div>

        {/* ── Limiter ──────────────────────────────────────── */}
        <div>
          <SectionHeader title="Limiter" color={LIM_COLOR} />
          <ParamRow
            label="Ceiling" value={ceiling} defaultValue={0} min={-12} max={0} step={0.5}
            format={fmtDb}
            leftLabel="−12 dB (heavy limit)" rightLabel="0 dB (unity)"
            color={LIM_COLOR} tertiaryColor={tc}
            onChange={v => { setCeiling(v); send(P_CEILING, v); }}
          />
        </div>

      </Space>
    </Modal>
  );
}
