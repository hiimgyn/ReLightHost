import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Alert } from 'antd';
import { Mic, AlertTriangle } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import { useAudioStore } from '../../stores/audioStore';
import type { PluginInstanceInfo } from '../../lib/types';
import { useWindowVisibility } from '../../lib/windowVisibility';
import { useTranslation } from '../../i18n';
import { useThemeStore } from '../../stores/themeStore';
import {
  AudioKnob,
  VisualStageContainer,
  WaveformDualCanvas,
  NeuralVADOrb,
  PluginHeader,
} from './common';

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

function paramValue(plugin: PluginInstanceInfo, id: number, fallback: number) {
  return plugin.parameters.find(p => p.id === id)?.value ?? fallback;
}

export default function NoiseSuppressorGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const sampleRate = useAudioStore(state => state.sampleRate);
  const isSampleRateMismatch = Math.abs(sampleRate - 48000) > 1;
  const modalWidth = typeof window === 'undefined' ? 560 : 'clamp(520px, 52vw, 580px)';

  const [mix,        setMix]        = useState(() => paramValue(plugin, 0, 1.0));
  const [vadGate,    setVadGate]    = useState(() => paramValue(plugin, 1, 0.0));
  const [gateAtten,  setGateAtten]  = useState(() => paramValue(plugin, 2, 0.0));
  const [outputGain, setOutputGain] = useState(() => paramValue(plugin, 3, 0.0));
  const [vad,        setVad]        = useState<number>(0);
  const isWindowVisible = useWindowVisibility();

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
  }, [isOpen, plugin.instance_id]);

  // Real-time Voice Activity Detection (VAD) visualizer polling
  const pollVad = useCallback(() => {
    if (!mountedRef.current) return;
    tauri.getNoiseSuppressorVad(plugin.instance_id)
      .then((val) => {
        if (!mountedRef.current) return;
        if (typeof val === 'number') {
          const clamped = Math.max(0, Math.min(1, val));
          setVad(clamped);
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
    if (isOpen && isWindowVisible) {
      mountedRef.current = true;
      pollVad();
    } else {
      mountedRef.current = false;
      if (rafRef.current !== null) {
        clearTimeout(rafRef.current);
        rafRef.current = null;
      }
    }
    return () => {
      mountedRef.current = false;
      if (rafRef.current !== null) {
        clearTimeout(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [isOpen, isWindowVisible, pollVad]);

  const handleResetAll = () => {
    const defaults = [
      { id: 0, val: 1.0, set: setMix },
      { id: 1, val: 0.0, set: setVadGate },
      { id: 2, val: 0.0, set: setGateAtten },
      { id: 3, val: 0.0, set: setOutputGain },
    ];
    defaults.forEach(({ id, val, set }) => {
      set(val);
      send(id, val);
    });
  };

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
            ? 'linear-gradient(180deg, #111722 0%, #0a0d14 100%)'
            : 'linear-gradient(180deg, #ffffff 0%, #f0fdfa 100%)',
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
          title={t('noise.title') || 'Noise Suppressor Pro'}
          subtitle="Real-Time Speech Enhancement & Neural VAD Gate"
          badgeText="NEURAL DSP"
          badgeColor="#00f0ff"
          onResetAll={handleResetAll}
          icon={<Mic size={18} className="text-cyan-400" />}
        />

        {/* 48kHz Warning if mismatched */}
        {isSampleRateMismatch && (
          <Alert
            type="warning"
            showIcon
            icon={<AlertTriangle size={15} />}
            message={
              <span className="text-xs">
                {t('noise.sampleRateMismatch')} (48 kHz required, current: {sampleRate} Hz)
              </span>
            }
            className="py-1 px-3 border border-amber-500/30 bg-amber-500/10 rounded-lg text-amber-200"
          />
        )}

        {/* Visual Stage: Dual Waveform Oscilloscope & AI Speech Orb */}
        <VisualStageContainer
          height={185}
          title="Dual-Layer Oscilloscope (Dry Noise vs Clean Voice)"
          badge={
            <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-1.5 py-0.5 rounded border border-cyan-500/20">
              VAD: {Math.round(vad * 100)}%
            </span>
          }
        >
          <div className="relative w-full h-full flex">
            {/* Waveform takes 75% width */}
            <div className="flex-1 h-full">
              <WaveformDualCanvas instanceId={plugin.instance_id} />
            </div>
            {/* Neural VAD Orb takes right section */}
            <div
              className={`w-28 h-full flex items-center justify-center border-l ${
                isDark ? 'border-white/5 bg-black/20' : 'border-slate-200 bg-slate-100/50'
              }`}
            >
              <NeuralVADOrb vad={vad} size={74} label="VAD CONFIDENCE" />
            </div>
          </div>
        </VisualStageContainer>

        {/* Primary Controls Row: Precision Audio Knobs */}
        <div
          className={`flex items-center justify-around py-3 px-2 rounded-xl border ${
            isDark ? 'bg-white/[0.03] border-white/5' : 'bg-slate-50 border-slate-200/80'
          }`}
        >
          <AudioKnob
            label={t('noise.mixTitle') || 'Wet / Dry Mix'}
            value={mix}
            defaultValue={1.0}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="lg"
            color="#00f0ff"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setMix(v);
              send(0, v);
            }}
          />

          <AudioKnob
            label={t('noise.vadGateThreshold') || 'VAD Gate'}
            value={vadGate}
            defaultValue={0.0}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="md"
            color="#38bdf8"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setVadGate(v);
              send(1, v);
            }}
          />

          <AudioKnob
            label={t('noise.gateAtten') || 'Attenuation'}
            value={gateAtten}
            defaultValue={0.0}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="md"
            color="#f59e0b"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setGateAtten(v);
              send(2, v);
            }}
          />

          <AudioKnob
            label={t('noise.outputGain') || 'Output Gain'}
            value={outputGain}
            defaultValue={0.0}
            min={-24}
            max={12}
            step={0.5}
            bipolar
            unit="dB"
            size="md"
            color="#10b981"
            format={(v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)}`}
            onChange={(v) => {
              setOutputGain(v);
              send(3, v);
            }}
          />
        </div>

        {/* Footer */}
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
