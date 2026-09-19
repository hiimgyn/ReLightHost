import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Slider, Typography, Space, Divider, Badge, Collapse, Tooltip, Alert, theme } from 'antd';
import { Mic, Volume2, Sliders, RotateCcw, AlertTriangle } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import { useAudioStore } from '../../stores/audioStore';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';

const { Text } = Typography;

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

// How many VAD history bars to display in the meter strip
const VAD_HISTORY = 40;

interface NoiseParamRowProps {
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

// Module-level extracted component to keep Slider mounting stable during 10fps VAD history updates
function NoiseParamRow({
  label, value, min, max, step,
  format, leftLabel, rightLabel,
  defaultValue, primaryColor, tertiaryColor, resetTooltip = "Reset to default", onChange,
}: NoiseParamRowProps) {
  return (
    <div
      style={{
        padding: '10px 14px',
        borderRadius: 10,
        background: 'var(--rh-surface-card)',
        border: '1px solid var(--rh-border-subtle)',
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

function paramValue(plugin: PluginInstanceInfo, id: number, fallback: number) {
  return plugin.parameters.find(p => p.id === id)?.value ?? fallback;
}

export default function NoiseSuppressorGui({ plugin, isOpen, onClose }: Props) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const sampleRate = useAudioStore(state => state.sampleRate);
  const isSampleRateMismatch = Math.abs(sampleRate - 48000) > 1;

  const modalWidth = typeof window === 'undefined' ? 520 : 'clamp(460px, 52vw, 540px)';

  const [mix,        setMix]        = useState(() => paramValue(plugin, 0, 1.0));
  const [vadGate,    setVadGate]    = useState(() => paramValue(plugin, 1, 0.0));
  const [gateAtten,  setGateAtten]  = useState(() => paramValue(plugin, 2, 0.0));
  const [outputGain, setOutputGain] = useState(() => paramValue(plugin, 3, 0.0));
  const [vad,        setVad]        = useState<number>(0);
  const [history,    setHistory]    = useState<number[]>(Array(VAD_HISTORY).fill(0));
  const rafRef = useRef<number | null>(null);
  const mountedRef = useRef(false);

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

  // Authoritative sync from backend when panel opens
  useEffect(() => {
    if (!isOpen) return;
    let isCancelled = false;

    (async () => {
      try {
        const params = await tauri.getPluginParameters(plugin.instance_id);
        if (isCancelled || !params || params.length === 0) return;
        const find = (id: number, fallback: number) =>
          params.find(p => p.id === id)?.value ?? paramValue(plugin, id, fallback);

        setMix(find(0, 1.0));
        setVadGate(find(1, 0.0));
        setGateAtten(find(2, 0.0));
        setOutputGain(find(3, 0.0));
      } catch {
        if (isCancelled) return;
        setMix(paramValue(plugin, 0, 1.0));
        setVadGate(paramValue(plugin, 1, 0.0));
        setGateAtten(paramValue(plugin, 2, 0.0));
        setOutputGain(paramValue(plugin, 3, 0.0));
      }
    })();

    return () => {
      isCancelled = true;
    };
  }, [isOpen, plugin.instance_id]); // eslint-disable-line react-hooks/exhaustive-deps

  // Real-time Voice Activity Detection (VAD) visualizer polling
  const pollVad = useCallback(() => {
    if (!mountedRef.current) return;
    tauri.getPluginParameters(plugin.instance_id)
      .then((params) => {
        if (!mountedRef.current) return;
        const vadParam = params?.find(p => p.id === 4);
        if (vadParam !== undefined) {
          const clamped = Math.max(0, Math.min(1, vadParam.value));
          setVad(clamped);
          setHistory(prev => {
            const next = [...prev.slice(1), clamped];
            return next;
          });
        }
      })
      .catch(() => {})
      .finally(() => {
        if (mountedRef.current) {
          rafRef.current = window.setTimeout(pollVad, 100);
        }
      });
  }, [plugin.instance_id]);

  useEffect(() => {
    if (!isOpen) return;
    mountedRef.current = true;
    pollVad();
    return () => {
      mountedRef.current = false;
      if (rafRef.current) {
        clearTimeout(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [isOpen, pollVad]);

  // VAD colour thresholds
  const isVoiceDetected = vad > 0.65;
  const vadColor   = isVoiceDetected ? token.colorSuccess : vad > 0.35 ? token.colorWarning : token.colorTextQuaternary;
  const vadLabel   = isVoiceDetected ? t('noise.voiceDetected') : vad > 0.35 ? t('noise.uncertain') : t('noise.backgroundNoise');
  const vadPercent = Math.round(vad * 100);

  return (
    <Modal
      title={
        <Space size={10}>
          <Mic size={16} style={{ color: token.colorPrimary }} />
          <span style={{ fontWeight: 700 }}>{t('noise.title')}</span>
          <Badge color="cyan" text={t('common.builtin')} />
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={modalWidth}
      style={{ top: 28, maxWidth: 540 }}
      styles={{ body: { maxHeight: 'calc(100vh - 160px)', overflowY: 'auto', overflowX: 'hidden', padding: '16px 22px 22px' } }}
    >
      <Space direction="vertical" size={14} style={{ width: '100%' }}>

        {/* ── Sample Rate Warning if not 48 kHz ───────────────── */}
        {isSampleRateMismatch && (
          <Alert
            type="warning"
            showIcon
            icon={<AlertTriangle size={15} />}
            message={t('noise.sampleRateMismatch')}
            description={
              <span style={{ fontSize: 12 }}>
                {t('noise.sampleRateMismatchDesc', { rate: sampleRate })}
              </span>
            }
          />
        )}

        {/* ── Voice Activity Meter ───────────────────────────── */}
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 8 }}>
            <Text type="secondary" style={{ fontSize: 11, fontWeight: 700, letterSpacing: 1, textTransform: 'uppercase' }}>
              {t('noise.voiceActivity')}
            </Text>
            <Text style={{ fontSize: 12, color: vadColor, fontVariantNumeric: 'tabular-nums', fontWeight: 600 }}>
              {vadPercent}% — {vadLabel}
            </Text>
          </div>

          {/* History strip */}
          <div
            className="minimal-surface"
            style={{
              display: 'flex',
              alignItems: 'flex-end',
              gap: 2,
              height: 44,
              padding: '3px 5px',
              background: 'var(--rh-surface-soft-gradient)',
              border: '1px solid var(--rh-surface-soft-border)',
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
                    opacity: 0.5 + (i / VAD_HISTORY) * 0.5,
                  }}
                />
              );
            })}
          </div>

          {/* Live badge with acoustic pulse when speaking */}
          <div style={{ display: 'flex', justifyContent: 'center', marginTop: 10 }}>
            <div
              className="minimal-surface"
              style={{
                position: 'relative',
                display: 'inline-flex',
                alignItems: 'center',
                gap: 8,
                padding: '6px 14px',
                background: 'var(--rh-surface-soft-gradient)',
                borderRadius: token.borderRadiusLG,
                border: `1px solid ${vadColor}88`,
                boxShadow: isVoiceDetected ? `0 0 16px ${token.colorSuccess}44` : 'none',
                transition: 'all 200ms ease',
              }}
            >
              <div
                style={{
                  position: 'relative',
                  width: 10,
                  height: 10,
                  borderRadius: '50%',
                  background: vadColor,
                  boxShadow: vad > 0.35 ? `0 0 8px ${vadColor}` : 'none',
                  transition: 'background 0.1s, box-shadow 0.1s',
                }}
              >
                {isVoiceDetected && (
                  <span
                    style={{
                      position: 'absolute',
                      inset: -4,
                      borderRadius: '50%',
                      border: `1px solid ${token.colorSuccess}`,
                      animation: 'rh-ring-wave 1.6s cubic-bezier(0.25, 0.8, 0.25, 1) infinite',
                    }}
                  />
                )}
              </div>
              <Text style={{ color: vadColor, fontSize: 13, fontWeight: 600 }}>
                {vadLabel}
              </Text>
            </div>
          </div>
        </div>

        <Divider style={{ margin: '4px 0' }} />

        {/* ── Mix Control ────────────────────────────────────── */}
        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <Space>
              <Volume2 size={15} style={{ color: token.colorPrimary }} />
              <Text strong style={{ fontSize: 13 }}>{t('noise.mixTitle')}</Text>
            </Space>
            <Space size={6} align="center">
              {mix !== 1.0 && (
                <Tooltip title={t('common.resetToDefault')}>
                  <RotateCcw
                    size={11}
                    style={{ cursor: 'pointer', color: token.colorTextTertiary }}
                    onClick={() => { setMix(1.0); send(0, 1.0); }}
                  />
                </Tooltip>
              )}
              <Text type="secondary" style={{ fontVariantNumeric: 'tabular-nums', fontSize: 13 }}>
                {Math.round(mix * 100)}%
              </Text>
            </Space>
          </div>

          <Slider
            min={0}
            max={1}
            step={0.01}
            value={mix}
            onChange={(val) => { setMix(val); send(0, val); }}
            tooltip={{ formatter: (v) => `${Math.round((v ?? 0) * 100)}%` }}
            trackStyle={{ background: token.colorPrimary }}
            handleStyle={{ borderColor: token.colorPrimary }}
          />

          <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 2 }}>
            <Text type="secondary" style={{ fontSize: 11 }}>{t('noise.mixDry')}</Text>
            <Text type="secondary" style={{ fontSize: 11 }}>{t('noise.mixFull')}</Text>
          </div>
        </div>

        {/* ── Advanced Controls ──────────────────────────────── */}
        <Collapse
          ghost
          items={[{
            key: 'advanced',
            label: (
              <Space size={6}>
                <Sliders size={14} style={{ color: token.colorTextSecondary }} />
                <Text style={{ fontSize: 13, color: token.colorTextSecondary, fontWeight: 500 }}>{t('noise.advancedSettings')}</Text>
              </Space>
            ),
            children: (
              <Space direction="vertical" size="middle" style={{ width: '100%', paddingTop: 4 }}>

                {/* VAD Gate Threshold */}
                <NoiseParamRow
                  label={t('noise.vadGateThreshold')}
                  value={vadGate}
                  defaultValue={0.0}
                  min={0}
                  max={1}
                  step={0.01}
                  format={v => v === 0 ? t('noise.off') : `${Math.round(v * 100)}%`}
                  leftLabel={t('noise.gateOff')}
                  rightLabel={t('noise.gateAll')}
                  primaryColor={token.colorPrimary}
                  tertiaryColor={token.colorTextTertiary}
                  resetTooltip={t('common.resetToDefault')}
                  onChange={(v) => { setVadGate(v); send(1, v); }}
                />

                {/* Gate Attenuation */}
                <NoiseParamRow
                  label={t('noise.gateAtten')}
                  value={gateAtten}
                  defaultValue={0.0}
                  min={0}
                  max={1}
                  step={0.01}
                  format={v => `${Math.round(v * 100)}%`}
                  leftLabel={t('noise.attenNone')}
                  rightLabel={t('noise.attenFull')}
                  primaryColor={token.colorPrimary}
                  tertiaryColor={token.colorTextTertiary}
                  resetTooltip={t('common.resetToDefault')}
                  onChange={(v) => { setGateAtten(v); send(2, v); }}
                />

                {/* Output Gain */}
                <NoiseParamRow
                  label={t('noise.outputGain')}
                  value={outputGain}
                  defaultValue={0.0}
                  min={-24}
                  max={12}
                  step={0.5}
                  format={v => `${v >= 0 ? '+' : ''}${v.toFixed(1)} dB`}
                  leftLabel="-24 dB"
                  rightLabel="+12 dB"
                  primaryColor={token.colorPrimary}
                  tertiaryColor={token.colorTextTertiary}
                  resetTooltip={t('common.resetToDefault')}
                  onChange={(v) => { setOutputGain(v); send(3, v); }}
                />

              </Space>
            ),
          }]}
        />

        {/* ── Info footer ────────────────────────────────────── */}
        <div
          className="minimal-surface"
          style={{
            padding: '8px 12px',
            background: 'var(--rh-surface-soft-gradient)',
            border: '1px solid var(--rh-surface-soft-border)',
            borderRadius: token.borderRadiusSM,
            lineHeight: 1.5,
          }}
        >
          <Text type="secondary" style={{ fontSize: 11 }}>
            {t('noise.footerDesc')}
          </Text>
        </div>

      </Space>
    </Modal>
  );
}
