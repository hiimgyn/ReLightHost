import { useEffect, useRef, useState } from 'react';
import * as tauri from '../../lib/tauri';
import type { VUData } from '../../lib/types';
import { useAudioStore } from '../../stores/audioStore';
import { useVisibleInterval } from '../../lib/useVisibleInterval';

const MIN_DB = -72;
const MAX_DB = 6;

function toDb(v: number) {
  if (v <= 0.000001) return MIN_DB;
  return Math.min(MAX_DB, Math.max(20 * Math.log10(v), MIN_DB));
}

function toFrac(db: number) {
  return Math.min(1, Math.max(0, (db - MIN_DB) / (MAX_DB - MIN_DB)));
}

const BAR_GRAD = 'linear-gradient(to right, #6367FF 0%, #8494FF 45%, #C9BEFF 75%, #FFDBFD 100%)';
const BAR_GRAD_CLIP = 'linear-gradient(to right, #f97316 0%, #ef4444 60%, #ff4d4f 100%)';

function HBar({ peak, rms, peak_hold, clip, isDark }: {
  peak: number; rms: number; peak_hold: number; clip: boolean; isDark: boolean;
}) {
  const peakPct = toFrac(toDb(peak)) * 100;
  const rmsPct  = toFrac(toDb(rms))  * 100;
  const holdPct = toFrac(toDb(peak_hold)) * 100;

  return (
    <div style={{
      position: 'relative', flex: 1, height: 6, borderRadius: 3,
      background: isDark ? 'rgba(0, 0, 0, 0.45)' : 'rgba(0, 0, 0, 0.08)',
      border: `1px solid ${isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(0, 0, 0, 0.08)'}`,
    }}>
      {/* RMS ghost */}
      <div style={{
        position: 'absolute', top: 0, left: 0, bottom: 0,
        width: rmsPct + '%', borderRadius: 3,
        background: isDark ? 'rgba(99,102,241,0.32)' : 'rgba(99,102,241,0.22)',
      }} />
      {/* Peak fill */}
      <div style={{
        position: 'absolute', top: 0, left: 0, bottom: 0,
        width: peakPct + '%', borderRadius: 3,
        background: clip ? BAR_GRAD_CLIP : BAR_GRAD,
        transition: 'width 80ms linear',
      }} />
      {/* Peak-hold tick */}
      {peak_hold > 0.001 && holdPct < 99 && (
        <div style={{
          position: 'absolute', top: -1, bottom: -1,
          left: 'calc(' + holdPct + '% - 1px)',
          width: 2, borderRadius: 1,
          background: isDark ? 'rgba(255,255,255,0.9)' : 'rgba(15,23,42,0.9)',
        }} />
      )}
    </div>
  );
}

export function VUMeter({ updateInterval = 160, isDark = true }: { updateInterval?: number; isDark?: boolean }) {
  const isMonitoring = useAudioStore((state) => state.status.is_monitoring);
  const [vu, setVu] = useState<VUData>({
    left:  { peak: 0, peak_hold: 0, rms: 0 },
    right: { peak: 0, peak_hold: 0, rms: 0 },
  });
  const [clipL, setClipL] = useState(false);
  const [clipR, setClipR] = useState(false);
  const timerL = useRef<ReturnType<typeof setTimeout> | null>(null);
  const timerR = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef(false);

  useEffect(() => {
    if (!isMonitoring) {
      if (timerL.current) clearTimeout(timerL.current);
      if (timerR.current) clearTimeout(timerR.current);
      timerL.current = null;
      timerR.current = null;
      inFlight.current = false;
      setClipL(false);
      setClipR(false);
      setVu({
        left: { peak: 0, peak_hold: 0, rms: 0 },
        right: { peak: 0, peak_hold: 0, rms: 0 },
      });
    }
  }, [updateInterval, isMonitoring]);

  useVisibleInterval(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    void (async () => {
      try {
        const d = await tauri.getVUData();
        setVu(d);
        if (d.left.peak > 0.989) {
          setClipL(true);
          if (timerL.current) clearTimeout(timerL.current);
          timerL.current = setTimeout(() => setClipL(false), 1500);
        }
        if (d.right.peak > 0.989) {
          setClipR(true);
          if (timerR.current) clearTimeout(timerR.current);
          timerR.current = setTimeout(() => setClipR(false), 1500);
        }
      } catch { /* not started */ } finally {
        inFlight.current = false;
      }
    })();
  }, updateInterval, isMonitoring, [updateInterval, isMonitoring]);

  const labelCss: React.CSSProperties = {
    fontSize: 8, fontWeight: 700, letterSpacing: 0.8, width: 8, flexShrink: 0,
    color: isDark ? 'rgba(201,190,255,0.86)' : 'rgba(99,103,255,0.74)', textTransform: 'uppercase',
  };


  return (
    <div style={{
      display: 'flex', flexDirection: 'row', alignItems: 'center', gap: 8,
      padding: '4px 12px',
      width: '100%',
      maxWidth: 'clamp(240px, 42vw, 760px)',
      minWidth: 0,
      borderRadius: 8,
      background: 'var(--rh-surface-card)',
      border: '1px solid var(--rh-border-subtle)',
    }}>
      {/* L channel */}
      <span style={labelCss}>L</span>
      <HBar peak={vu.left.peak} rms={vu.left.rms} peak_hold={vu.left.peak_hold} clip={clipL} isDark={isDark} />

      {/* divider */}
      <div style={{
        width: 1,
        height: 14,
        background: isDark ? 'rgba(255,255,255,0.1)' : 'rgba(15,23,42,0.1)',
        flexShrink: 0,
      }} />

      {/* R channel */}
      <span style={labelCss}>R</span>
      <HBar peak={vu.right.peak} rms={vu.right.rms} peak_hold={vu.right.peak_hold} clip={clipR} isDark={isDark} />
    </div>
  );
}
