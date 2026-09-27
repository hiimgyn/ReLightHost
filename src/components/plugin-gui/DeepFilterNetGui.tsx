import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal, Alert } from 'antd';
import { Sparkles, AlertTriangle, Cpu, Zap } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import { useAudioStore } from '../../stores/audioStore';
import type { PluginInstanceInfo } from '../../lib/types';
import { useWindowVisibility } from '../../lib/windowVisibility';
import { useTranslation } from '../../i18n';
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

export default function DeepFilterNetGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const sampleRate = useAudioStore(state => state.sampleRate);
  const isSampleRateMismatch = Math.abs(sampleRate - 48000) > 1;
  const modalWidth = typeof window === 'undefined' ? 580 : 'clamp(540px, 54vw, 620px)';

  // Parameters:
  // 0: Max Attenuation (dB) [0..60] def: 24.0
  // 1: Post-filter Threshold Beta [0..1] def: 0.2
  // 2: Mix [0..1] def: 1.0
  // 3: Output Gain (dB) [-24..+12] def: 0.0
  const [attenLim,    setAttenLim]    = useState(() => paramValue(plugin, 0, 24.0));
  const [postFilter,  setPostFilter]  = useState(() => paramValue(plugin, 1, 0.2));
  const [mix,         setMix]         = useState(() => paramValue(plugin, 2, 1.0));
  const [outputGain,  setOutputGain]  = useState(() => paramValue(plugin, 3, 0.0));
  const [vad,         setVad]         = useState<number>(0);
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

  // Sync parameter values from backend on open
  useEffect(() => {
    if (!isOpen) return;
    let isCancelled = false;

    (async () => {
      try {
        const params = await tauri.getPluginParameters(plugin.instance_id);
        if (isCancelled || !params || params.length === 0) return;
        const find = (id: number, fallback: number) =>
          params.find(p => p.id === id)?.value ?? paramValue(plugin, id, fallback);

        setAttenLim(find(0, 24.0));
        setPostFilter(find(1, 0.2));
        setMix(find(2, 1.0));
        setOutputGain(find(3, 0.0));
      } catch {
        if (isCancelled) return;
        setAttenLim(paramValue(plugin, 0, 24.0));
        setPostFilter(paramValue(plugin, 1, 0.2));
        setMix(paramValue(plugin, 2, 1.0));
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
          rafRef.current = window.setTimeout(pollVad, 80);
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
      { id: 0, val: 24.0, set: setAttenLim },
      { id: 1, val: 0.2,  set: setPostFilter },
      { id: 2, val: 1.0,  set: setMix },
      { id: 3, val: 0.0,  set: setOutputGain },
    ];
    defaults.forEach(({ id, val, set }) => {
      set(val);
      send(id, val);
    });
  };

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
          background: 'linear-gradient(180deg, #130f24 0%, #090812 100%)',
          border: '1px solid rgba(168, 85, 247, 0.25)',
          boxShadow: '0 24px 60px rgba(0, 0, 0, 0.85), inset 0 1px 0 rgba(255, 255, 255, 0.08)',
          borderRadius: 16,
          padding: '20px 22px 24px',
        },
      }}
    >
      <div className="flex flex-col gap-4 text-white">
        {/* Header */}
        <PluginHeader
          title={t('deepFilter.title') || 'DeepFilterNet 3 Pro'}
          subtitle={t('deepFilter.subtitle') || 'Deep Complex-Valued Spectrogram Filtering'}
          badgeText={t('deepFilter.badgeText') || 'DEEP LEARNING SOTA'}
          badgeColor="#a855f7"
          onResetAll={handleResetAll}
          icon={<Sparkles size={18} className="text-purple-400" />}
        />

        {/* 48kHz Warning if mismatched */}
        {isSampleRateMismatch && (
          <Alert
            type="warning"
            showIcon
            icon={<AlertTriangle size={15} />}
            message={
              <span className="text-xs">
                {t('deepFilter.sampleRateMismatch')} (48 kHz required, current: {sampleRate} Hz)
              </span>
            }
            className="py-1 px-3 border border-amber-500/30 bg-amber-500/10 rounded-lg text-amber-200"
          />
        )}

        {/* Visual Stage: Neural Spectral Oscilloscope & AI Speech Orb */}
        <VisualStageContainer
          height={185}
          title={t('deepFilter.oscilloscopeTitle') || 'Neural Dual Oscilloscope (Raw Noise vs Clean Voice)'}
          badge={
            <div className="flex items-center gap-2">
              <span className="flex items-center gap-1 text-[10px] font-mono text-purple-400 bg-purple-500/10 px-1.5 py-0.5 rounded border border-purple-500/20">
                <Cpu size={10} /> 10ms HOP
              </span>
              <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-1.5 py-0.5 rounded border border-cyan-500/20">
                VAD: {Math.round(vad * 100)}%
              </span>
            </div>
          }
        >
          <div className="relative w-full h-full flex">
            {/* Waveform takes 75% width */}
            <div className="flex-1 h-full">
              <WaveformDualCanvas
                vad={vad}
                reductionDb={attenLim * mix}
                active={mix > 0.05}
              />
            </div>
            {/* Neural VAD Orb takes right section */}
            <div className="w-28 h-full flex items-center justify-center border-l border-white/5 bg-black/25">
              <NeuralVADOrb
                vad={vad}
                size={74}
                label={t('deepFilter.vadConfidence') || 'SPEECH ENERGY'}
              />
            </div>
          </div>
        </VisualStageContainer>

        {/* Primary Controls Row: Rotary Studio Knobs */}
        <div className="flex items-center justify-around py-3 px-2 bg-white/[0.025] border border-white/5 rounded-xl">
          <AudioKnob
            label={t('deepFilter.maxAttenuation') || 'Max Attenuation'}
            value={attenLim}
            defaultValue={24.0}
            min={0}
            max={60}
            step={0.5}
            unit="dB"
            size="lg"
            color="#a855f7"
            format={(v) => `-${Math.round(v)}`}
            onChange={(v) => {
              setAttenLim(v);
              send(0, v);
            }}
          />

          <AudioKnob
            label={t('deepFilter.postFilterBeta') || 'Post-Filter Beta'}
            value={postFilter}
            defaultValue={0.2}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="md"
            color="#06b6d4"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setPostFilter(v);
              send(1, v);
            }}
          />

          <AudioKnob
            label={t('deepFilter.mix') || 'Wet / Dry Mix'}
            value={mix}
            defaultValue={1.0}
            min={0}
            max={1}
            step={0.01}
            unit="%"
            size="md"
            color="#10b981"
            format={(v) => Math.round(v * 100).toString()}
            onChange={(v) => {
              setMix(v);
              send(2, v);
            }}
          />

          <AudioKnob
            label={t('deepFilter.outputGain') || 'Output Trim'}
            value={outputGain}
            defaultValue={0.0}
            min={-24}
            max={12}
            step={0.1}
            unit="dB"
            size="md"
            color="#f59e0b"
            format={(v) => `${v > 0 ? '+' : ''}${v.toFixed(1)}`}
            onChange={(v) => {
              setOutputGain(v);
              send(3, v);
            }}
          />
        </div>

        {/* Neural Info Pill Footer */}
        <div className="flex items-center justify-between text-[11px] text-zinc-400/80 px-2">
          <span className="flex items-center gap-1.5">
            <Zap size={12} className="text-purple-400" />
            <span>DeepFilterNet3 ONNX • Tract Runtime</span>
          </span>
          <span className="text-[10px] font-mono text-zinc-400/60">
            Atten: -{attenLim.toFixed(0)} dB • Beta: {postFilter.toFixed(2)}
          </span>
        </div>
      </div>
    </Modal>
  );
}
