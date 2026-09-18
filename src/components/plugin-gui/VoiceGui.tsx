import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { CustomerServiceOutlined, UndoOutlined } from '@ant-design/icons';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';

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
interface SectionProps { title: string; color: string; }
function SectionHeader({ title, color }: SectionProps) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
      <span style={{ fontSize: 11, letterSpacing: 1.5, color, textTransform: 'uppercase', fontWeight: 700 }}>
        {title}
      </span>
      <div
        style={{
          flex: 1,
          height: 1,
          background: `linear-gradient(90deg, ${color}88, transparent)`,
          boxShadow: `0 0 8px ${color}66`,
        }}
      />
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
    <div
      className="minimal-surface"
      style={{
        padding: '8px 10px',
        borderRadius: 8,
        background: 'var(--rh-surface-soft-gradient)',
        border: '1px solid var(--rh-surface-soft-border)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
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
  const modalWidth = typeof window === 'undefined' ? 540 : 'clamp(320px, 34vw, 560px)';

  const [low,     setLow]     = useState(() => paramValue(plugin, P_LOW,      0));
  const [mid,     setMid]     = useState(() => paramValue(plugin, P_MID,      0));
  const [high,    setHigh]    = useState(() => paramValue(plugin, P_HIGH,     0));
  const [drive,   setDrive]   = useState(() => paramValue(plugin, P_DRIVE,    0));
  const [width,   setWidth]   = useState(() => paramValue(plugin, P_WIDTH,    0));
  const [ceiling, setCeiling] = useState(() => paramValue(plugin, P_CEILING,  0));

  // Trailing throttle for IPC parameter updates
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

  useEffect(() => {
    if (!isOpen) return;
    let isCancelled = false;

    // Fetch authoritative parameter values from backend; fall back to props
    (async () => {
      try {
        const params = await tauri.getPluginParameters(plugin.instance_id);
        if (isCancelled || !params || params.length === 0) return;
        const find = (id: number, fallback: number) =>
          params.find(p => p.id === id)?.value ?? paramValue(plugin, id, fallback);

        setLow(find(P_LOW, 0));
        setMid(find(P_MID, 0));
        setHigh(find(P_HIGH, 0));
        setDrive(find(P_DRIVE, 0));
        setWidth(find(P_WIDTH, 0));
        setCeiling(find(P_CEILING, 0));
      } catch {
        if (isCancelled) return;
        setLow(paramValue(plugin, P_LOW, 0));
        setMid(paramValue(plugin, P_MID, 0));
        setHigh(paramValue(plugin, P_HIGH, 0));
        setDrive(paramValue(plugin, P_DRIVE, 0));
        setWidth(paramValue(plugin, P_WIDTH, 0));
        setCeiling(paramValue(plugin, P_CEILING, 0));
      }
    })();

    return () => {
      isCancelled = true;
    };
  }, [isOpen, plugin.instance_id]); // eslint-disable-line react-hooks/exhaustive-deps

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
      width={modalWidth}
      style={{ top: 16, maxWidth: 560 }}
      styles={{ body: { maxHeight: 'calc(100vh - 160px)', overflowY: 'auto', overflowX: 'hidden', padding: '14px 16px 16px' } }}
    >
      <Space orientation="vertical" size={14} style={{ width: '100%' }}>

        {/* ── EQ ───────────────────────────────────────────── */}
        <div>
          <SectionHeader title="3-Band EQ" color={EQ_COLOR} />
          <Space orientation="vertical" size={10} style={{ width: '100%' }}>
            <ParamRow
              label="Low (200 Hz)" value={low} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel="−12 dB (cut bass)" rightLabel="+12 dB (boost bass)"
              color={EQ_COLOR} tertiaryColor={tc}
              onChange={v => { setLow(v); send(P_LOW, v); }}
            />
            <ParamRow
              label="Mid (2 kHz)" value={mid} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel="−12 dB (thin)" rightLabel="+12 dB (body / presence)"
              color={EQ_COLOR} tertiaryColor={tc}
              onChange={v => { setMid(v); send(P_MID, v); }}
            />
            <ParamRow
              label="High (8 kHz)" value={high} defaultValue={0} min={-12} max={12} step={0.5}
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
            leftLabel="0% (clean)" rightLabel="100% (warm saturation)"
            color={SAT_COLOR} tertiaryColor={tc}
            onChange={v => { setDrive(v); send(P_DRIVE, v); }}
          />
        </div>

        {/* ── Doubler ──────────────────────────────────────── */}
        <div>
          <SectionHeader title="Stereo Doubler" color={DBL_COLOR} />
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
            leftLabel="−12 dB (heavy limit)" rightLabel="0 dB (unity / safety)"
            color={LIM_COLOR} tertiaryColor={tc}
            onChange={v => { setCeiling(v); send(P_CEILING, v); }}
          />
        </div>

      </Space>
    </Modal>
  );
}
