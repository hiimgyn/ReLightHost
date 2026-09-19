import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Badge, Tooltip, theme } from 'antd';
import { Activity, RotateCcw, Sliders } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';

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
  resetTooltip?: string;
  onChange: (v: number) => void;
}

function ParamRow({
  label, value, min, max, step,
  format, leftLabel, rightLabel,
  defaultValue, primaryColor, tertiaryColor, resetTooltip = "Reset to default", onChange,
}: ParamRowProps) {
  return (
    <div
      style={{
        padding: '10px 14px',
        borderRadius: 10,
        background: 'var(--rh-surface-card)',
        border: '1px solid var(--rh-border-subtle)',
        transition: 'border-color 160ms ease, box-shadow 160ms ease',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
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
  const { t } = useTranslation();
  const pc = token.colorPrimary;
  const tc = token.colorTextTertiary;
  const modalWidth = typeof window === 'undefined' ? 520 : 'clamp(460px, 48vw, 540px)';

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
          <Activity size={16} style={{ color: token.colorPrimary }} />
          <span>{t('compressor.title')}</span>
          <Badge color="cyan" text={t('common.builtin')} />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={modalWidth}
      style={{ top: 24, maxWidth: 540 }}
      styles={{ body: { maxHeight: 'calc(100vh - 140px)', overflowY: 'auto', overflowX: 'hidden', padding: '16px 20px 22px' } }}
    >
      <Space direction="vertical" size={14} style={{ width: '100%' }}>

        {/* ── Signature Visual Dynamic Badge ──────────────────── */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '12px 16px',
            borderRadius: 10,
            background: 'var(--rh-surface-elevated)',
            border: `1px solid ${token.colorPrimary}44`,
          }}
        >
          <Space size={8} align="center">
            <Sliders size={16} style={{ color: token.colorPrimary }} />
            <div>
              <div style={{ fontSize: 11, fontWeight: 700, letterSpacing: 0.8, textTransform: 'uppercase', color: token.colorPrimary }}>
                {t('compressor.dynamicsProfile')}
              </div>
              <Text type="secondary" style={{ fontSize: 11 }}>
                {ratio >= 10 ? t('compressor.limiting') : ratio >= 4 ? t('compressor.standard') : ratio > 1 ? t('compressor.gentle') : t('compressor.linear')}
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
          label={t('compressor.threshold')} value={threshold} defaultValue={-18} min={-60} max={0} step={0.5}
          format={v => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`}
          leftLabel={t('compressor.thresholdLeft')} rightLabel={t('compressor.thresholdRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setThreshold(v); send(P_THRESHOLD, v); }}
        />

        <ParamRow
          label={t('compressor.ratio')} value={ratio} defaultValue={4} min={1} max={20} step={0.1}
          format={v => `${v.toFixed(1)} : 1`}
          leftLabel={t('compressor.ratioLeft')} rightLabel={t('compressor.ratioRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setRatio(v); send(P_RATIO, v); }}
        />

        {/* ── Timing ────────────────────────────────────────── */}
        <ParamRow
          label={t('compressor.attack')} value={attack} defaultValue={10} min={0.1} max={200} step={0.1}
          format={v => v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`}
          leftLabel={t('compressor.attackLeft')} rightLabel={t('compressor.attackRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setAttack(v); send(P_ATTACK, v); }}
        />

        <ParamRow
          label={t('compressor.release')} value={release} defaultValue={100} min={10} max={2000} step={1}
          format={v => v < 1000 ? `${Math.round(v)} ms` : `${(v / 1000).toFixed(2)} s`}
          leftLabel={t('compressor.releaseLeft')} rightLabel={t('compressor.releaseRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setRelease(v); send(P_RELEASE, v); }}
        />

        {/* ── Output ────────────────────────────────────────── */}
        <ParamRow
          label={t('compressor.makeupGain')} value={makeup} defaultValue={0} min={0} max={30} step={0.5}
          format={v => `+${v.toFixed(1)} dB`}
          leftLabel="0 dB" rightLabel="+30 dB"
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setMakeup(v); send(P_MAKEUP, v); }}
        />

        <ParamRow
          label={t('compressor.knee')} value={knee} defaultValue={3} min={0} max={12} step={0.5}
          format={v => `${v.toFixed(1)} dB`}
          leftLabel={t('compressor.kneeLeft')} rightLabel={t('compressor.kneeRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setKnee(v); send(P_KNEE, v); }}
        />

        <ParamRow
          label={t('compressor.parallelMix')} value={mix} defaultValue={1} min={0} max={1} step={0.01}
          format={v => `${Math.round(v * 100)}%`}
          leftLabel={t('compressor.parallelMixLeft')} rightLabel={t('compressor.parallelMixRight')}
          primaryColor={pc} tertiaryColor={tc} resetTooltip={t('common.resetToDefault')}
          onChange={v => { setMix(v); send(P_MIX, v); }}
        />

      </Space>
    </Modal>
  );
}