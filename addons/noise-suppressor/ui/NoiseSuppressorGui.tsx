import { Modal, Alert } from 'antd';
import { Mic, AlertTriangle } from 'lucide-react';
import { useAudioStore } from '../../ui-common/host';
import type { PluginInstanceInfo } from '../../ui-common/host';
import { usePluginStore } from '../../ui-common/host';
import { useTranslation } from '../../ui-common/host';
import { useThemeStore } from '../../ui-common/host';
import {
  AudioKnob,
  VisualStageContainer,
  WaveformDualCanvas,
  NeuralVADOrb,
  PluginHeader,
  useParams,
  useMeters,
} from '../../ui-common';

interface Props {
  plugin: PluginInstanceInfo;
  isOpen: boolean;
  onClose: () => void;
}

const DEFAULTS = { 0: 1.0, 1: 0.0, 2: 0.0, 3: 0.0 };

export default function NoiseSuppressorGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const sampleRate = useAudioStore(state => state.sampleRate);
  // Error text set by the host (instance.rs); only the rate one mentions 48000 Hz.
  const errorText = usePluginStore((s) => {
    const status = s.crashStatusByInstanceId[plugin.instance_id];
    return status?.type === 'Error' ? status.data : null;
  });
  const isSampleRateMismatch = errorText?.includes('needs 48000 Hz') ?? false;
  const modalWidth = typeof window === 'undefined' ? 560 : 'clamp(520px, 52vw, 580px)';


  const { values, set, resetAll: handleResetAll } = useParams(plugin, isOpen, DEFAULTS);
  const mix = values[0];
  const vadGate = values[1];
  const gateAtten = values[2];
  const outputGain = values[3];
  const meters = useMeters(plugin.instance_id, isOpen);
  const vad = Math.max(0, Math.min(1, meters[0] ?? 0));

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
          subtitle={t('noise.subtitle')}
          badgeText="NEURAL DSP"
          badgeColor="#00f0ff"
          onResetAll={handleResetAll}
          icon={<Mic size={18} className="text-cyan-400" />}
        />

        {/* 48 kHz warning, or the host's error text for any other error */}
        {errorText && (
          <Alert
            type="warning"
            showIcon
            icon={<AlertTriangle size={15} />}
            message={
              <span className="text-xs">
                {isSampleRateMismatch
                  ? `${t('noise.sampleRateMismatch')} (${t('noise.sampleRateRequired', { rate: sampleRate })})`
                  : errorText}
              </span>
            }
            className="py-1 px-3 border border-amber-500/30 bg-amber-500/10 rounded-lg text-amber-200"
          />
        )}

        {/* Visual Stage: Dual Waveform Oscilloscope & AI Speech Orb */}
        <VisualStageContainer
          height={185}
          title={t('noise.levelsTitle')}
          badge={
            <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-1.5 py-0.5 rounded border border-cyan-500/20">
              VAD: {Math.round(vad * 100)}%
            </span>
          }
        >
          <div className="relative w-full h-full flex">
            {/* Waveform takes 75% width */}
            <div className="flex-1 h-full">
              <WaveformDualCanvas instanceId={plugin.instance_id} active={isOpen} />
            </div>
            {/* Neural VAD Orb takes right section */}
            <div
              className={`w-28 h-full flex items-center justify-center border-l ${
                isDark ? 'border-white/5 bg-black/20' : 'border-slate-200 bg-slate-100/50'
              }`}
            >
              <NeuralVADOrb vad={vad} active={isOpen} size={74} label={t('noise.vadConfidence')} />
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
              set(0, v);
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
              set(1, v);
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
              set(2, v);
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
              set(3, v);
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
