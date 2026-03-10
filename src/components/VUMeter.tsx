import { useEffect, useState, useRef } from 'react';
import * as tauri from '../lib/tauri';
import './VUMeter.css';

interface VUChannel {
  peak: number;      // 0.0 - 1.0
  peak_hold: number; // 0.0 - 1.0
  rms: number;       // 0.0 - 1.0
}

interface VUData {
  left: VUChannel;
  right: VUChannel;
}

interface VUMeterProps {
  updateInterval?: number;
  compact?: boolean;
}

/** Convert linear amplitude → dB, clamped to [-60, +∞) */
function toDb(linear: number): number {
  if (linear <= 0.00001) return -Infinity;
  return Math.max(20 * Math.log10(linear), -60);
}

/** Normalize dB [-60…0] → [0…100] */
function normalize(db: number): number {
  if (!isFinite(db)) return 0;
  return ((db + 60) / 60) * 100;
}

function dbLabel(db: number): string {
  if (!isFinite(db)) return '-∞';
  return db.toFixed(1);
}

/**
 * VU Meter — segmented PPM-style with peak hold, RMS, and dB readout.
 * compact=true → slim dual-bar for the header toolbar.
 */
export function VUMeter({ updateInterval = 50, compact = false }: VUMeterProps) {
  const [vuData, setVuData] = useState<VUData>({
    left:  { peak: 0, peak_hold: 0, rms: 0 },
    right: { peak: 0, peak_hold: 0, rms: 0 },
  });

  // track clip state independently so it stays visible briefly
  const clipTimerL = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clipTimerR = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [clipL, setClipL] = useState(false);
  const [clipR, setClipR] = useState(false);

  useEffect(() => {
    const fetch = async () => {
      try {
        const data = await tauri.getVUData();
        setVuData(data);
        // Clip detection (> -0.1 dB ≈ 0.989)
        if (data.left.peak > 0.989) {
          setClipL(true);
          if (clipTimerL.current) clearTimeout(clipTimerL.current);
          clipTimerL.current = setTimeout(() => setClipL(false), 1500);
        }
        if (data.right.peak > 0.989) {
          setClipR(true);
          if (clipTimerR.current) clearTimeout(clipTimerR.current);
          clipTimerR.current = setTimeout(() => setClipR(false), 1500);
        }
      } catch { /* engine not started yet */ }
    };
    fetch();
    const id = setInterval(fetch, updateInterval);
    return () => {
      clearInterval(id);
      if (clipTimerL.current) clearTimeout(clipTimerL.current);
      if (clipTimerR.current) clearTimeout(clipTimerR.current);
    };
  }, [updateInterval]);

  if (compact) {
    return (
      <div className="vum-compact">
        <CompactBar label="L" channel={vuData.left} clip={clipL} />
        <CompactBar label="R" channel={vuData.right} clip={clipR} />
      </div>
    );
  }

  return (
    <div className="vum-full">
      <FullBar label="L" channel={vuData.left} clip={clipL} />
      <FullBar label="R" channel={vuData.right} clip={clipR} />
      {/* dB scale ticks */}
      <div className="vum-scale-row">
        {['-60','-48','-36','-24','-18','-12','-6','-3','0'].map(db => (
          <span key={db} className="vum-scale-tick"
            style={{ left: `${normalize(parseFloat(db))}%` }}
          >{db}</span>
        ))}
      </div>
    </div>
  );
}

/* ---- Compact bar (header) ---- */
function CompactBar({ label, channel, clip }: { label: string; channel: VUChannel; clip: boolean }) {
  const peakDb  = toDb(channel.peak);
  const rmsDb   = toDb(channel.rms);
  const holdDb  = toDb(channel.peak_hold);
  const peakPct = normalize(peakDb);
  const rmsPct  = normalize(rmsDb);
  const holdPct = normalize(holdDb);

  return (
    <div className="vum-compact-channel">
      <span className="vum-compact-label">{label}</span>
      <div className="vum-bar-wrap">
        {/* ghost gradient background */}
        <div className="vum-ghost" />
        {/* RMS underlay */}
        <div className="vum-rms-bar" style={{ width: `${rmsPct}%` }} />
        {/* Peak fill */}
        <div className="vum-peak-bar" style={{ width: `${peakPct}%` }} />
        {/* Peak-hold tick */}
        {channel.peak_hold > 0.001 && (
          <div className="vum-hold-tick" style={{ left: `calc(${holdPct}% - 1px)` }} />
        )}
      </div>
      <span className="vum-compact-db" style={{ color: clip ? '#e74c3c' : peakDb > -12 ? '#f39c12' : '#8ec86e' }}>
        {clip ? 'CLIP' : dbLabel(peakDb)}
      </span>
    </div>
  );
}

/* ---- Full bar (standalone) ---- */
function FullBar({ label, channel, clip }: { label: string; channel: VUChannel; clip: boolean }) {
  const peakDb  = toDb(channel.peak);
  const rmsDb   = toDb(channel.rms);
  const holdDb  = toDb(channel.peak_hold);
  const peakPct = normalize(peakDb);
  const rmsPct  = normalize(rmsDb);
  const holdPct = normalize(holdDb);

  return (
    <div className="vum-full-channel">
      <span className="vum-full-label">{label}</span>
      <div className="vum-full-bar-wrap">
        <div className="vum-ghost" />
        <div className="vum-rms-bar" style={{ width: `${rmsPct}%` }} />
        <div className="vum-peak-bar" style={{ width: `${peakPct}%` }} />
        {channel.peak_hold > 0.001 && (
          <div className="vum-hold-tick" style={{ left: `calc(${holdPct}% - 1px)` }} />
        )}
      </div>
      <span className="vum-full-db" style={{ color: clip ? '#e74c3c' : peakDb > -12 ? '#f39c12' : '#8ec86e' }}>
        {clip ? '▲CLIP' : `${dbLabel(peakDb)} dB`}
      </span>
    </div>
  );
}

interface VUChannel {
  peak: number;      // 0.0 - 1.0
  peak_hold: number; // 0.0 - 1.0
  rms: number;       // 0.0 - 1.0
}

interface VUData {
  left: VUChannel;
  right: VUChannel;
}

interface VUMeterProps {
  /** Update interval in milliseconds (default: 50ms = 20 FPS) */
  updateInterval?: number;
  /** Show RMS levels in addition to peak (default: false) */
  showRMS?: boolean;
  /** Compact mode (smaller height, no labels) */
  compact?: boolean;
}

