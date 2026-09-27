import { useState, useEffect, useRef, useCallback } from 'react';
import { Modal } from 'antd';
import { Mic, Flame } from 'lucide-react';
import * as tauri from '../../lib/tauri';
import type { PluginInstanceInfo } from '../../lib/types';
import { useTranslation } from '../../i18n';
import { useThemeStore } from '../../stores/themeStore';
import {
  AudioKnob,
  VisualStageContainer,
  BodePlotCanvas,
  PluginHeader,
} from './common';

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

export default function VoiceGui({ plugin, isOpen, onClose }: Props) {
  const { t } = useTranslation();
  const modalWidth = typeof window === 'undefined' ? 560 : 'clamp(520px, 52vw, 580px)';

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
  }, [isOpen, plugin.instance_id]);

  const handleResetAll = () => {
    const defaults = [
      { id: P_LOW,     val: 0, set: setLow },
      { id: P_MID,     val: 0, set: setMid },
      { id: P_HIGH,    val: 0, set: setHigh },
      { id: P_DRIVE,   val: 0, set: setDrive },
      { id: P_WIDTH,   val: 0, set: setWidth },
      { id: P_CEILING, val: 0, set: setCeiling },
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
            ? 'linear-gradient(180deg, #131722 0%, #0b0d14 100%)'
            : 'linear-gradient(180deg, #ffffff 0%, #f5f7ff 100%)',
          border: isDark
            ? '1px solid rgba(255, 255, 255, 0.12)'
            : '1px solid rgba(0, 0, 0, 0.1)',
          boxShadow: isDark
            ? '0 24px 60px rgba(0, 0, 0, 0.75), inset 0 1px 0 rgba(255, 255, 255, 0.08)'
            : '0 20px 48px rgba(99, 102, 241, 0.08), 0 4px 12px rgba(0, 0, 0, 0.04)',
          borderRadius: 16,
          padding: '20px 22px 24px',
        },
      }}
    >
      <div className={`flex flex-col gap-4 ${isDark ? 'text-white' : 'text-slate-900'}`}>
        {/* Header */}
        <PluginHeader
          title={t('voice.title') || 'Voice Designer Pro'}
          subtitle="4-Stage Vocal Processing: 3-Band EQ • Tube Saturation • Stereo Doubler • Peak Limiter"
          badgeText="VOCAL CHANNEL"
          badgeColor="#6366f1"
          onResetAll={handleResetAll}
          icon={<Mic size={18} className="text-indigo-400" />}
        />

        {/* Visual Stage: Interactive 3-Band Bode Plot */}
        <VisualStageContainer
          height={185}
          title="Frequency Response (20 Hz — 20 kHz)"
          badge={
            <div className="flex items-center gap-2">
              <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-1.5 py-0.5 rounded border border-cyan-500/20">
                Low: {low >= 0 ? '+' : ''}{low.toFixed(1)}dB
              </span>
              <span className="text-[10px] font-mono text-amber-400 bg-amber-500/10 px-1.5 py-0.5 rounded border border-amber-500/20">
                Mid: {mid >= 0 ? '+' : ''}{mid.toFixed(1)}dB
              </span>
              <span className="text-[10px] font-mono text-purple-400 bg-purple-500/10 px-1.5 py-0.5 rounded border border-purple-500/20">
                Air: {high >= 0 ? '+' : ''}{high.toFixed(1)}dB
              </span>
            </div>
          }
        >
          <BodePlotCanvas
            lowGain={low}
            midGain={mid}
            highGain={high}
            onLowGainChange={(v) => {
              setLow(v);
              send(P_LOW, v);
            }}
            onMidGainChange={(v) => {
              setMid(v);
              send(P_MID, v);
            }}
            onHighGainChange={(v) => {
              setHigh(v);
              send(P_HIGH, v);
            }}
            accentColor="#6366f1"
          />
        </VisualStageContainer>

        {/* Section 1: 3-Band EQ Tone Controls */}
        <div
          className={`border rounded-xl p-3 ${
            isDark ? 'bg-white/[0.03] border-white/5' : 'bg-slate-50 border-slate-200/80'
          }`}
        >
          <div className="flex items-center gap-1.5 mb-2.5 px-1">
            <span
              className={`text-[11px] font-bold tracking-wider uppercase ${
                isDark ? 'text-indigo-400' : 'text-indigo-600'
              }`}
            >
              Tonal Balance (Interactive EQ)
            </span>
          </div>
          <div className="flex items-center justify-around">
            <AudioKnob
              label={t('voice.low') || 'Bass (200Hz)'}
              value={low}
              defaultValue={0}
              min={-12}
              max={12}
              step={0.5}
              bipolar
              unit="dB"
              size="md"
              color="#38bdf8"
              format={(v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)}`}
              onChange={(v) => {
                setLow(v);
                send(P_LOW, v);
              }}
            />

            <AudioKnob
              label={t('voice.mid') || 'Presence (2kHz)'}
              value={mid}
              defaultValue={0}
              min={-12}
              max={12}
              step={0.5}
              bipolar
              unit="dB"
              size="md"
              color="#f59e0b"
              format={(v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)}`}
              onChange={(v) => {
                setMid(v);
                send(P_MID, v);
              }}
            />

            <AudioKnob
              label={t('voice.high') || 'Air (8kHz)'}
              value={high}
              defaultValue={0}
              min={-12}
              max={12}
              step={0.5}
              bipolar
              unit="dB"
              size="md"
              color="#a855f7"
              format={(v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)}`}
              onChange={(v) => {
                setHigh(v);
                send(P_HIGH, v);
              }}
            />
          </div>
        </div>

        {/* Section 2: Character, Space & Limiter */}
        <div
          className={`border rounded-xl p-3 ${
            isDark ? 'bg-white/[0.02] border-white/5' : 'bg-slate-50/80 border-slate-200/80'
          }`}
        >
          <div className="flex items-center justify-between mb-2.5 px-1">
            <span
              className={`text-[11px] font-bold tracking-wider uppercase ${
                isDark ? 'text-white/60' : 'text-slate-600'
              }`}
            >
              Warmth, Width & Dynamics
            </span>
            {drive > 0.05 && (
              <span className="flex items-center gap-1 text-[10px] text-amber-400 font-mono">
                <Flame size={10} className="fill-amber-400 text-amber-400" />
                Tube Saturation Active
              </span>
            )}
          </div>
          <div className="flex items-center justify-around">
            <AudioKnob
              label={t('voice.drive') || 'Warmth'}
              value={drive}
              defaultValue={0}
              min={0}
              max={1}
              step={0.01}
              unit="%"
              size="md"
              color="#ef4444"
              format={(v) => Math.round(v * 100).toString()}
              onChange={(v) => {
                setDrive(v);
                send(P_DRIVE, v);
              }}
            />

            <AudioKnob
              label={t('voice.width') || 'Stereo Width'}
              value={width}
              defaultValue={0}
              min={0}
              max={1}
              step={0.01}
              unit="%"
              size="md"
              color="#10b981"
              format={(v) => Math.round(v * 100).toString()}
              onChange={(v) => {
                setWidth(v);
                send(P_WIDTH, v);
              }}
            />

            <AudioKnob
              label={t('voice.ceiling') || 'Limiter Ceiling'}
              value={ceiling}
              defaultValue={0}
              min={-12}
              max={0}
              step={0.5}
              unit="dB"
              size="md"
              color="#f43f5e"
              format={(v) => `${v >= 0 ? '' : ''}${v.toFixed(1)}`}
              onChange={(v) => {
                setCeiling(v);
                send(P_CEILING, v);
              }}
            />
          </div>
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
