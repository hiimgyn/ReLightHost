import { Modal, Alert } from 'antd';
import { Sparkles, AlertTriangle, Cpu, Zap } from 'lucide-react';
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

const DEFAULTS = { 0: 24.0, 1: 0.2, 2: 1.0, 3: 0.0 };

export default function DeepFilterNetGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const sampleRate = useAudioStore(state => state.sampleRate);
  // Add-ons that cannot run at the current rate report status Error.
  const isSampleRateMismatch = usePluginStore(
    (s) => s.crashStatusByInstanceId[plugin.instance_id]?.type === 'Error',
  );
  const modalWidth = typeof window === 'undefined' ? 580 : 'clamp(540px, 54vw, 620px)';

  // Parameters:
  // 0: Max Attenuation (dB) [0..60] def: 24.0
  // 1: Post-filter Threshold Beta [0..1] def: 0.2
  // 2: Mix [0..1] def: 1.0
  // 3: Output Gain (dB) [-24..+12] def: 0.0

  const { values, set, resetAll: handleResetAll } = useParams(plugin, isOpen, DEFAULTS);
  const attenLim = values[0];
  const postFilter = values[1];
  const mix = values[2];
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
            ? 'linear-gradient(180deg, #130f24 0%, #090812 100%)'
            : 'linear-gradient(180deg, #ffffff 0%, #f8f6ff 100%)',
          border: isDark
            ? '1px solid rgba(168, 85, 247, 0.25)'
            : '1px solid rgba(168, 85, 247, 0.2)',
          boxShadow: isDark
            ? '0 24px 60px rgba(0, 0, 0, 0.85), inset 0 1px 0 rgba(255, 255, 255, 0.08)'
            : '0 20px 48px rgba(168, 85, 247, 0.08), 0 4px 12px rgba(0, 0, 0, 0.04)',
          borderRadius: 16,
          padding: '20px 22px 24px',
        },
      }}
    >
      <div className={`flex flex-col gap-4 ${isDark ? 'text-white' : 'text-slate-900'}`}>
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
                {t('deepFilter.sampleRateMismatch')} ({t('deepFilter.sampleRateRequired', { rate: sampleRate })})
              </span>
            }
            className="py-1 px-3 border border-amber-500/30 bg-amber-500/10 rounded-lg text-amber-200"
          />
        )}

        {/* Visual Stage: Neural Spectral Oscilloscope & AI Speech Orb */}
        <VisualStageContainer
          height={185}
          title={t('deepFilter.oscilloscopeTitle')}
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
              <WaveformDualCanvas instanceId={plugin.instance_id} active={isOpen} />
            </div>
            {/* Neural VAD Orb takes right section */}
            <div
              className={`w-28 h-full flex items-center justify-center border-l ${
                isDark ? 'border-white/5 bg-black/25' : 'border-slate-200 bg-slate-100/50'
              }`}
            >
              <NeuralVADOrb
                vad={vad}
                active={isOpen}
                size={74}
                label={t('deepFilter.vadConfidence') || 'SPEECH ENERGY'}
              />
            </div>
          </div>
        </VisualStageContainer>

        {/* Primary Controls Row: Rotary Studio Knobs */}
        <div
          className={`flex items-center justify-around py-3 px-2 rounded-xl border ${
            isDark ? 'bg-white/[0.025] border-white/5' : 'bg-slate-50 border-slate-200/80'
          }`}
        >
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
              set(0, v);
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
              set(1, v);
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
              set(2, v);
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
              set(3, v);
            }}
          />
        </div>

        {/* Neural Info Pill Footer */}
        <div
          className={`flex items-center justify-between text-[11px] px-2 ${
            isDark ? 'text-zinc-400/80' : 'text-slate-500'
          }`}
        >
          <span className="flex items-center gap-1.5">
            <Zap size={12} className={isDark ? 'text-purple-400' : 'text-purple-600'} />
            <span>DeepFilterNet3 ONNX • Tract Runtime</span>
          </span>
          <span className={`text-[10px] font-mono ${isDark ? 'text-zinc-400/60' : 'text-slate-400'}`}>
            Atten: -{attenLim.toFixed(0)} dB • Beta: {postFilter.toFixed(2)}
          </span>
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
