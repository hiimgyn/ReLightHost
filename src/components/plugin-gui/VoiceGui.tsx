import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { Mic, RotateCcw } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';

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
  resetTooltip?: string;
  onChange: (v: number) => void;
}

function ParamRow({
  label, value, min, max, step,
  format, leftLabel, rightLabel,
  defaultValue, color, tertiaryColor, resetTooltip = "Reset to default", onChange,
}: ParamRowProps) {
  return (
    <div
      style={{
        padding: '10px 14px',
        borderRadius: 10,
        background: 'var(--rh-surface-card)',
        border: '1px solid var(--rh-border-subtle)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
        <Text style={{ fontSize: 13 }}>{label}</Text>
        <Space size={6} align="center">
          {value !== defaultValue && (
            <Tooltip title={resetTooltip}>
              <RotateCcw
                size={11}
                style={{ cursor: 'pointer', color: tertiaryColor }}
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
  const { t } = useTranslation();
  const tc = token.colorTextTertiary;
  const modalWidth = typeof window === 'undefined' ? 540 : 'clamp(480px, 50vw, 580px)';

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
          <Mic size={16} style={{ color: token.colorPrimary }} />
          <span>{t('voice.title')}</span>
          <Badge color="cyan" text={t('common.builtin')} />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={modalWidth}
      style={{ top: 24, maxWidth: 580 }}
      styles={{ body: { maxHeight: 'calc(100vh - 140px)', overflowY: 'auto', overflowX: 'hidden', padding: '16px 20px 22px' } }}
    >
      <Space direction="vertical" size={14} style={{ width: '100%' }}>

        {/* ── EQ ───────────────────────────────────────────── */}
        <div>
          <SectionHeader title={t('voice.eqSection')} color={EQ_COLOR} />
          <Space direction="vertical" size={10} style={{ width: '100%' }}>
            <ParamRow
              label={t('voice.low')} value={low} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel={t('voice.lowLeft')} rightLabel={t('voice.lowRight')}
              color={EQ_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
              onChange={v => { setLow(v); send(P_LOW, v); }}
            />
            <ParamRow
              label={t('voice.mid')} value={mid} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel={t('voice.midLeft')} rightLabel={t('voice.midRight')}
              color={EQ_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
              onChange={v => { setMid(v); send(P_MID, v); }}
            />
            <ParamRow
              label={t('voice.high')} value={high} defaultValue={0} min={-12} max={12} step={0.5}
              format={fmtDb}
              leftLabel={t('voice.highLeft')} rightLabel={t('voice.highRight')}
              color={EQ_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
              onChange={v => { setHigh(v); send(P_HIGH, v); }}
            />
          </Space>
        </div>

        {/* ── Saturation ───────────────────────────────────── */}
        <div>
          <SectionHeader title={t('voice.saturationSection')} color={SAT_COLOR} />
          <ParamRow
            label={t('voice.drive')} value={drive} defaultValue={0} min={0} max={1} step={0.01}
            format={fmtPct}
            leftLabel={t('voice.driveLeft')} rightLabel={t('voice.driveRight')}
            color={SAT_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
            onChange={v => { setDrive(v); send(P_DRIVE, v); }}
          />
        </div>

        {/* ── Doubler ──────────────────────────────────────── */}
        <div>
          <SectionHeader title={t('voice.doublerSection')} color={DBL_COLOR} />
          <ParamRow
            label={t('voice.width')} value={width} defaultValue={0} min={0} max={1} step={0.01}
            format={fmtPct}
            leftLabel={t('voice.widthLeft')} rightLabel={t('voice.widthRight')}
            color={DBL_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
            onChange={v => { setWidth(v); send(P_WIDTH, v); }}
          />
        </div>

        {/* ── Limiter ──────────────────────────────────────── */}
        <div>
          <SectionHeader title={t('voice.limiterSection')} color={LIM_COLOR} />
          <ParamRow
            label={t('voice.ceiling')} value={ceiling} defaultValue={0} min={-12} max={0} step={0.5}
            format={fmtDb}
            leftLabel={t('voice.ceilingLeft')} rightLabel={t('voice.ceilingRight')}
            color={LIM_COLOR} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
            onChange={v => { setCeiling(v); send(P_CEILING, v); }}
          />
        </div>

      </Space>
    </Modal>
  );
}
