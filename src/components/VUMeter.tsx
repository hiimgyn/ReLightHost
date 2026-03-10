import { useEffect, useState } from 'react';
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
  /** Update interval in milliseconds (default: 50ms = 20 FPS) */
  updateInterval?: number;
  /** Show RMS levels in addition to peak (default: false) */
  showRMS?: boolean;
  /** Compact mode (smaller height, no labels) */
  compact?: boolean;
}

/**
 * VU Meter component for real-time audio level visualization
 * 
 * Features:
 * - Peak detection with color coding (green/yellow/red)
 * - Peak hold indicators
 * - RMS display for average loudness
 * - dB scale (-60 to 0 dB)
 * - Inspired by rust-vst3-host metering
 */
export function VUMeter({ updateInterval = 50, showRMS = false, compact = false }: VUMeterProps) {
  const [vuData, setVuData] = useState<VUData>({
    left: { peak: 0, peak_hold: 0, rms: 0 },
    right: { peak: 0, peak_hold: 0, rms: 0 },
  });

  useEffect(() => {
    const fetchVU = async () => {
      try {
        const data = await tauri.getVUData();
        setVuData(data);
      } catch (err) {
        // Silently fail - audio engine may not be started
      }
    };

    fetchVU(); // Initial fetch
    const interval = setInterval(fetchVU, updateInterval);
    return () => clearInterval(interval);
  }, [updateInterval]);

  return (
    <div className={`vu-meter ${compact ? 'vu-meter-compact' : ''}`}>
      <VUChannel
        label="L"
        channel={vuData.left}
        showRMS={showRMS}
        compact={compact}
      />
      <VUChannel
        label="R"
        channel={vuData.right}
        showRMS={showRMS}
        compact={compact}
      />
    </div>
  );
}

interface VUChannelProps {
  label: string;
  channel: VUChannel;
  showRMS: boolean;
  compact: boolean;
}

function VUChannel({ label, channel, showRMS, compact }: VUChannelProps) {
  const peakDb = toDb(channel.peak);
  const rmsDb = toDb(channel.rms);
  const holdDb = toDb(channel.peak_hold);

  // Calculate percentage for display (map -60dB to 0dB → 0% to 100%)
  const peakPercent = normalize(peakDb);
  const rmsPercent = normalize(rmsDb);
  const holdPercent = normalize(holdDb);

  const peakColor = getColorForDb(peakDb);
  const rmsColor = 'rgba(255, 255, 255, 0.3)'; // Dim white for RMS

  return (
    <div className="vu-channel">
      {!compact && <span className="vu-label">{label}</span>}
      
      <div className="vu-bar-container">
        {/* RMS bar (background) */}
        {showRMS && (
          <div
            className="vu-bar vu-rms"
            style={{
              width: `${rmsPercent}%`,
              backgroundColor: rmsColor,
            }}
          />
        )}
        
        {/* Peak bar (foreground) */}
        <div
          className="vu-bar vu-peak"
          style={{
            width: `${peakPercent}%`,
            backgroundColor: peakColor,
          }}
        />
        
        {/* Peak hold line */}
        {channel.peak_hold > 0.001 && (
          <div
            className="vu-hold"
            style={{ left: `${holdPercent}%` }}
          />
        )}
        
        {/* dB scale markers */}
        <div className="vu-scale">
          <div className="vu-mark" style={{ left: '0%' }} title="-60 dB" />
          <div className="vu-mark" style={{ left: '50%' }} title="-30 dB" />
          <div className="vu-mark vu-mark-hot" style={{ left: '80%' }} title="-12 dB" />
          <div className="vu-mark vu-mark-clip" style={{ left: '95%' }} title="-3 dB" />
          <div className="vu-mark vu-mark-clip" style={{ left: '100%' }} title="0 dB" />
        </div>
      </div>
      
      {!compact && (
        <span className="vu-value">
          {peakDb > -60 ? `${peakDb.toFixed(1)}` : '-∞'}
          <span className="vu-unit">dB</span>
        </span>
      )}
    </div>
  );
}

/**
 * Convert linear amplitude to decibels
 */
function toDb(linear: number): number {
  const SILENCE_THRESHOLD = 0.00001; // -100 dB
  if (linear > SILENCE_THRESHOLD) {
    return Math.max(20 * Math.log10(linear), -60);
  }
  return -Infinity;
}

/**
 * Normalize dB to percentage (map -60dB to 0dB → 0% to 100%)
 */
function normalize(db: number): number {
  if (!isFinite(db)) return 0;
  return ((db + 60) / 60) * 100;
}

/**
 * Get color based on dB level
 * Green: < -12 dB (normal)
 * Yellow: -12 to -3 dB (hot)
 * Red: > -3 dB (clipping warning)
 */
function getColorForDb(db: number): string {
  if (!isFinite(db) || db < -60) return '#2ecc71'; // Green (silence)
  if (db > -3) return '#e74c3c';   // Red (clipping)
  if (db > -12) return '#f39c12';  // Yellow (hot)
  return '#2ecc71';                // Green (normal)
}
