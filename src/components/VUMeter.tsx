import { useEffect, useRef, useState } from 'react';
import * as tauri from '../lib/tauri';
import type { VUData } from '../lib/types';

function toDb(v: number) { return v <= 0.0031623 ? -50 : Math.min(0, Math.max(20 * Math.log10(v), -50)); }
function toFrac(db: number) { return Math.min(1, Math.max(0, (db + 50) / 50)); }

const BAR_GRAD = 'linear-gradient(to right, #9b72cf 0%, #b08ee0 45%, #e478a8 75%, #ff4d4f 100%)';
const BAR_GRAD_CLIP = 'linear-gradient(to right, #f97316 0%, #ef4444 60%, #ff4d4f 100%)';

function HBar({ peak, rms, peak_hold, clip }: {
  peak: number; rms: number; peak_hold: number; clip: boolean;
}) {
  const peakPct = toFrac(toDb(peak)) * 100;
  const rmsPct  = toFrac(toDb(rms))  * 100;
  const holdPct = toFrac(toDb(peak_hold)) * 100;

  return (
    <div style={{
      position: 'relative', flex: 1, height: 5, borderRadius: 3,
      background: 'rgba(255,255,255,0.05)',
      border: '1px solid rgba(255,255,255,0.07)',
    }}>
      {/* RMS ghost */}
      <div style={{
        position: 'absolute', top: 0, left: 0, bottom: 0,
        width: rmsPct + '%', borderRadius: 3,
        background: 'rgba(155,114,207,0.2)',
      }} />
      {/* Peak fill */}
      <div style={{
        position: 'absolute', top: 0, left: 0, bottom: 0,
        width: peakPct + '%', borderRadius: 3,
        background: clip ? BAR_GRAD_CLIP : BAR_GRAD,
        transition: 'width 35ms linear',
        boxShadow: peakPct > 5 ? (clip ? '0 0 6px rgba(255,77,79,.5)' : '0 0 6px rgba(155,114,207,.5)') : undefined,
      }} />
      {/* Peak-hold tick */}
      {peak_hold > 0.001 && holdPct < 99 && (
        <div style={{
          position: 'absolute', top: -1, bottom: -1,
          left: 'calc(' + holdPct + '% - 1px)',
          width: 2, borderRadius: 1,
          background: 'rgba(255,255,255,0.8)',
          boxShadow: '0 0 4px rgba(255,255,255,0.5)',
        }} />
      )}
    </div>
  );
}

export function VUMeter({ updateInterval = 50, isDark = true }: { updateInterval?: number; isDark?: boolean }) {
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
    const poll = async () => {
      if (document.visibilityState !== 'visible') return;
      if (inFlight.current) return;
      inFlight.current = true;
      try {
        const d = await tauri.getVUData();
        setVu(d);
        if (d.left.peak  > 0.989) { setClipL(true); if (timerL.current) clearTimeout(timerL.current); timerL.current = setTimeout(() => setClipL(false), 1500); }
        if (d.right.peak > 0.989) { setClipR(true); if (timerR.current) clearTimeout(timerR.current); timerR.current = setTimeout(() => setClipR(false), 1500); }
      } catch { /* not started */ }
      finally {
        inFlight.current = false;
      }
    };

    poll();
    const id = setInterval(poll, updateInterval);
    const onVisible = () => {
      if (document.visibilityState === 'visible') {
        poll();
      }
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      clearInterval(id);
      document.removeEventListener('visibilitychange', onVisible);
      if (timerL.current) clearTimeout(timerL.current);
      if (timerR.current) clearTimeout(timerR.current);
    };
  }, [updateInterval]);

  const labelCss: React.CSSProperties = {
    fontSize: 8, fontWeight: 700, letterSpacing: 0.8, width: 8, flexShrink: 0,
    color: isDark ? 'rgba(176,142,224,0.8)' : 'rgba(109,40,217,0.7)', textTransform: 'uppercase',
  };


  return (
    <div style={{
      display: 'flex', flexDirection: 'row', alignItems: 'center', gap: 6,
      padding: '3px 10px',
      width: '100%',
      maxWidth: 'clamp(240px, 42vw, 760px)',
      minWidth: 0,
    }}>
      {/* L channel */}
      <span style={labelCss}>L</span>
      <HBar peak={vu.left.peak} rms={vu.left.rms} peak_hold={vu.left.peak_hold} clip={clipL} />

      {/* divider */}
      <div style={{ width: 1, height: 14, background: 'rgba(255,255,255,0.1)', flexShrink: 0 }} />

      {/* R channel */}
      <span style={labelCss}>R</span>
      <HBar peak={vu.right.peak} rms={vu.right.rms} peak_hold={vu.right.peak_hold} clip={clipR} />
    </div>
  );
}
