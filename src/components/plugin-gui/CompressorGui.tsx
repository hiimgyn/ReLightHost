import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal } from 'antd';
import { Activity } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';
import { useThemeStore } from '../../stores/themeStore';
import {
  AudioKnob,
  VisualStageContainer,
  TransferCurveCanvas,
  PluginHeader,
} from './common';

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

export default function CompressorGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const modalWidth = typeof window === 'undefined' ? 560 : 'clamp(520px, 52vw, 580px)';

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
  }, [isOpen, plugin.instance_id]);

  const handleResetAll = () => {
    const defaults = [
      { id: P_THRESHOLD, val: -18, set: setThreshold },
      { id: P_RATIO,     val: 4,   set: setRatio },
      { id: P_ATTACK,    val: 10,  set: setAttack },
      { id: P_RELEASE,   val: 100, set: setRelease },
      { id: P_MAKEUP,    val: 0,   set: setMakeup },
      { id: P_KNEE,      val: 3,   set: setKnee },
      { id: P_MIX,       val: 1,   set: setMix },
    ];
    defaults.forEach(({ id, val, set }) => {
      set(val);
      send(id, val);
    });
  };

  const dynamicsProfileText = ratio >= 10
    ? t('compressor.limiting')
    : ratio >= 4
    ? t('compressor.standard')
    : ratio > 1
    ? t('compressor.gentle')
    : t('compressor.linear');

  const isDark = useThemeStore((s) => s.theme === 'dark');

  return (
    <Modal
      open={isOpen}
      onCancel={onClose}
      footer={null}
      width={modalWidth}
      centered
      closable={false}
      styles={{
        body: {
          background: isDark
            ? 'linear-gradient(180deg, #141822 0%, #0c0e14 100%)'
            : 'linear-gradient(180deg, #ffffff 0%, #f8fafc 100%)',
          border: isDark
            ? '1px solid rgba(255, 255, 255, 0.12)'
            : '1px solid rgba(0, 0, 0, 0.1)',
          boxShadow: isDark
            ? '0 24px 60px rgba(0, 0, 0, 0.75), inset 0 1px 0 rgba(255, 255, 255, 0.08)'
            : '0 20px 48px rgba(0, 0, 0, 0.12), 0 4px 12px rgba(0, 0, 0, 0.04)',
          borderRadius: 16,
          padding: '20px 22px 24px',
        },
      }}
    >
      <div className={`flex flex-col gap-4 ${isDark ? 'text-white' : 'text-slate-900'}`}>
        {/* Header */}
        <PluginHeader
          title={t('compressor.title') || 'Studio Compressor'}
          subtitle={`${dynamicsProfileText} • Quadratic Soft Knee`}
          badgeText="DYNAMIC DSP"
          badgeColor="#f59e0b"
          onResetAll={handleResetAll}
          icon={<Activity size={18} className="text-amber-400" />}
        />

        {/* Visual Stage: Interactive Transfer Curve */}
        <VisualStageContainer
          height={190}
          title="Transfer Characteristic & Gain Reduction"
          badge={
            <span className="text-[10px] font-mono text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded border border-amber-500/20">
              {threshold.toFixed(1)} dB / {ratio.toFixed(1)}:1
            </span>
          }
        >
          <TransferCurveCanvas
            threshold={threshold}
            ratio={ratio}
            knee={knee}
            makeup={makeup}
            liveInputDb={threshold - 6} // responsive indicator
            liveGainReductionDb={Math.max(0, (threshold - -12) * (1 - 1 / ratio))}
            onThresholdChange={(v) => {
              setThreshold(v);
              send(P_THRESHOLD, v);
            }}
            accentColor="#f59e0b"
          />
        </VisualStageContainer>

        {/* Primary Controls Row: Precision Audio Knobs */}
        <div
          className={`flex items-center justify-around py-3 px-2 rounded-xl border ${
            isDark ? 'bg-white/[0.03] border-white/5' : 'bg-slate-50 border-slate-200/80'
          }`}
        >
          <AudioKnob
            label={t('compressor.threshold') || 'Threshold'}
            value={threshold}
            defaultValue={-18}
            min={-60}
            max={0}
            step={0.5}
            unit="dB"
            size="lg"
            color="#f59e0b"
            format={(v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)}`}
            onChange={(v) => {
              setThreshold(v);
              send(P_THRESHOLD, v);
            }}
          />

          <AudioKnob
            label={t('compressor.ratio') || 'Ratio'}
            value={ratio}
            defaultValue={4}
            min={1}
            max={20}
            step={0.1}
            unit=":1"
            size="lg"
            color="#f59e0b"
            format={(v) => v.toFixed(1)}
            onChange={(v) => {
              setRatio(v);
              send(P_RATIO, v);
            }}
          />

          <AudioKnob
            label={t('compressor.attack') || 'Attack'}
            value={attack}
            defaultValue={10}
            min={0.1}
            max={200}
            step={0.5}
            unit="ms"
            size="md"
            color="#38bdf8"
            format={(v) => (v < 10 ? v.toFixed(1) : Math.round(v).toString())}
            onChange={(v) => {
              setAttack(v);
              send(P_ATTACK, v);
            }}
          />

          <AudioKnob
            label={t('compressor.release') || 'Release'}
            value={release}
            defaultValue={100}
            min={10}
            max={2000}
            step={5}
            unit={release < 1000 ? 'ms' : 's'}
            size="md"
            color="#38bdf8"
            format={(v) => (v < 1000 ? Math.round(v).toString() : (v / 1000).toFixed(2))}
            onChange={(v) => {
              setRelease(v);
              send(P_RELEASE, v);
            }}
          />
        </div>

        {/* Secondary Polish Controls Strip */}
        <div
          className={`flex items-center justify-around py-2.5 px-4 rounded-xl border ${
            isDark ? 'bg-white/[0.02] border-white/5' : 'bg-slate-50/80 border-slate-200/80'
          }`}
        >
          <AudioKnob
            label={t('compressor.knee') || 'Knee'}
            value={knee}
            defaultValue={3}
            min={0}
            max={12}
            step={0.5}
            unit="dB"
            size="sm"
            color="#a855f7"
            format={(v) => v.toFixed(1)}
            onChange={(v) => {
              setKnee(v);
              send(P_KNEE, v);
            }}
          />

          <AudioKnob
            label={t('compressor.makeupGain') || 'Makeup Gain'}
            value={makeup}
            defaultValue={0}
            min={0}
            max={30}
            step={0.5}
            unit="dB"
            size="sm"
            color="#10b981"
            format={(v) => `+${v.toFixed(1)}`}
            onChange={(v) => {
              setMakeup(v);
              send(P_MAKEUP, v);
            }}
          />

          <AudioKnob
            label={t('compressor.parallelMix') || 'Wet / Dry'}
            value={mix}
            defaultValue={1}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="sm"
            color="#6366f1"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setMix(v);
              send(P_MIX, v);
            }}
          />
        </div>

        {/* Close Button Footer */}
        <div className="flex justify-end pt-1">
          <button
            onClick={onClose}
            className={`px-4 py-1.5 text-xs font-semibold rounded-lg transition-colors cursor-pointer border ${
              isDark
                ? 'text-white/80 hover:text-white bg-white/5 hover:bg-white/10 border-white/10'
                : 'text-slate-700 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 border-slate-300'
            }`}
          >
            {t('common.close') || 'Close'}
          </button>
        </div>
      </div>
    </Modal>
  );
}