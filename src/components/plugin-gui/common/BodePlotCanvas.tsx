import React, { useRef, useEffect } from 'react';
import { useThemeStore } from '../../../stores/themeStore';

interface BodePlotCanvasProps {
  lowGain: number;   // -12 to +12 dB
  midGain: number;   // -12 to +12 dB
  highGain: number;  // -12 to +12 dB
  onLowGainChange?: (val: number) => void;
  onMidGainChange?: (val: number) => void;
  onHighGainChange?: (val: number) => void;
  accentColor?: string;
}

export const BodePlotCanvas: React.FC<BodePlotCanvasProps> = ({
  lowGain,
  midGain,
  highGain,
  onLowGainChange,
  onMidGainChange,
  onHighGainChange,
  accentColor = '#6366f1',
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const activeDragNode = useRef<'low' | 'mid' | 'high' | null>(null);

  // Frequencies of the 3 Voice Designer bands
  const F_LOW = 200;
  const F_MID = 2000;
  const F_HIGH = 8000;

  // Log frequency span: 20 Hz to 20,000 Hz (3 decades)
  const MIN_FREQ = 20;
  const MAX_FREQ = 20000;
  const LOG_MIN = Math.log10(MIN_FREQ);
  const LOG_MAX = Math.log10(MAX_FREQ);
  const LOG_SPAN = LOG_MAX - LOG_MIN;

  // dB range (-14 dB to +14 dB for headroom)
  const MIN_DB = -14;
  const MAX_DB = 14;
  const DB_SPAN = MAX_DB - MIN_DB;

  // Calculate estimated filter gain at frequency `f` (Hz)
  const calculateGainAtFreq = (f: number) => {
    // 1. Low shelf at 200 Hz
    let gLow = 0;
    if (f < F_LOW) {
      gLow = lowGain;
    } else if (f < F_LOW * 3) {
      const t = (Math.log10(f) - Math.log10(F_LOW)) / (Math.log10(F_LOW * 3) - Math.log10(F_LOW));
      gLow = lowGain * (1 - t * t * (3 - 2 * t)); // smoothstep
    }

    // 2. Mid peak at 2000 Hz (Q approx 1.0, bandwidth ~1 octave)
    const logRatio = Math.abs(Math.log2(f / F_MID));
    const gMid = midGain * Math.exp(-0.5 * (logRatio * logRatio) / 0.4);

    // 3. High shelf at 8000 Hz
    let gHigh = 0;
    if (f > F_HIGH) {
      gHigh = highGain;
    } else if (f > F_HIGH / 3) {
      const t = (Math.log10(f) - Math.log10(F_HIGH / 3)) / (Math.log10(F_HIGH) - Math.log10(F_HIGH / 3));
      gHigh = highGain * (t * t * (3 - 2 * t));
    }

    return gLow + gMid + gHigh;
  };

  const isDark = useThemeStore((s) => s.theme === 'dark');

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const width = rect.width;
    const height = rect.height;

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr;
      canvas.height = height * dpr;
    }

    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const padL = 28;
    const padR = 20;
    const padT = 16;
    const padB = 22;

    const plotW = width - padL - padR;
    const plotH = height - padT - padB;

    const toX = (f: number) => padL + ((Math.log10(f) - LOG_MIN) / LOG_SPAN) * plotW;
    const toY = (db: number) => padT + (1 - (db - MIN_DB) / DB_SPAN) * plotH;

    // 1. Frequency Grid lines & Labels
    const freqMarkers = [
      { f: 50, label: '50' },
      { f: 100, label: '100' },
      { f: 200, label: '200' },
      { f: 500, label: '500' },
      { f: 1000, label: '1k' },
      { f: 2000, label: '2k' },
      { f: 5000, label: '5k' },
      { f: 10000, label: '10k' },
    ];

    ctx.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.05)' : 'rgba(0, 0, 0, 0.07)';
    ctx.lineWidth = 1;
    ctx.font = '9px JetBrains Mono, monospace';
    ctx.fillStyle = isDark ? 'rgba(255, 255, 255, 0.35)' : 'rgba(15, 23, 42, 0.45)';
    ctx.textAlign = 'center';

    freqMarkers.forEach(({ f, label }) => {
      const x = toX(f);
      ctx.beginPath();
      ctx.moveTo(x, padT);
      ctx.lineTo(x, padT + plotH);
      ctx.stroke();
      ctx.fillText(label, x, height - 7);
    });

    // 2. dB Horizontal Grid lines (-12, -6, 0, +6, +12)
    const dbMarkers = [-12, -6, 0, 6, 12];
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';

    dbMarkers.forEach((db) => {
      const y = toY(db);
      ctx.beginPath();
      ctx.strokeStyle =
        db === 0
          ? (isDark ? 'rgba(255, 255, 255, 0.15)' : 'rgba(0, 0, 0, 0.18)')
          : (isDark ? 'rgba(255, 255, 255, 0.05)' : 'rgba(0, 0, 0, 0.07)');
      ctx.lineWidth = db === 0 ? 1.5 : 1;
      ctx.moveTo(padL, y);
      ctx.lineTo(padL + plotW, y);
      ctx.stroke();
      ctx.fillText(`${db > 0 ? '+' : ''}${db}`, padL - 4, y);
    });

    // 3. Combined EQ Response Curve
    const numPoints = 120;
    const curvePoints: [number, number][] = [];
    for (let i = 0; i <= numPoints; i++) {
      const logF = LOG_MIN + (i / numPoints) * LOG_SPAN;
      const f = Math.pow(10, logF);
      const totalDb = calculateGainAtFreq(f);
      curvePoints.push([toX(f), toY(totalDb)]);
    }

    // Gradient fill under EQ curve
    const zeroY = toY(0);
    const grad = ctx.createLinearGradient(0, padT, 0, padT + plotH);
    grad.addColorStop(0, `${accentColor}30`);
    grad.addColorStop(0.5, `${accentColor}12`);
    grad.addColorStop(1, `${accentColor}02`);

    ctx.beginPath();
    ctx.moveTo(toX(MIN_FREQ), zeroY);
    curvePoints.forEach(([px, py]) => ctx.lineTo(px, py));
    ctx.lineTo(toX(MAX_FREQ), zeroY);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();

    // Solid EQ curve
    ctx.beginPath();
    curvePoints.forEach(([px, py], i) => {
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    });
    ctx.strokeStyle = accentColor;
    ctx.lineWidth = 2.5;
    ctx.stroke();

    // 4. Interactive Node Handles (Low, Mid, High)
    const nodes = [
      { key: 'low' as const, f: F_LOW, gain: lowGain, color: '#38bdf8', label: 'BASS' },
      { key: 'mid' as const, f: F_MID, gain: midGain, color: '#f59e0b', label: 'MID' },
      { key: 'high' as const, f: F_HIGH, gain: highGain, color: '#a855f7', label: 'AIR' },
    ];

    nodes.forEach(({ f, gain, color, label }) => {
      const nx = toX(f);
      const ny = toY(gain);

      // Node ring
      ctx.beginPath();
      ctx.arc(nx, ny, 6, 0, Math.PI * 2);
      ctx.fillStyle = '#ffffff';
      ctx.shadowColor = color;
      ctx.shadowBlur = 10;
      ctx.fill();
      ctx.strokeStyle = color;
      ctx.lineWidth = 2.5;
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Small band name tag
      ctx.fillStyle = color;
      ctx.font = '8px Inter, sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(label, nx, ny - 10);
    });

    ctx.restore();
  }, [lowGain, midGain, highGain, accentColor, isDark]);

  // Pointer drag on nodes
  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const padL = 28;
    const padR = 20;
    const plotW = rect.width - padL - padR;

    const toX = (f: number) => padL + ((Math.log10(f) - LOG_MIN) / LOG_SPAN) * plotW;
    const xLow = toX(F_LOW);
    const xMid = toX(F_MID);
    const xHigh = toX(F_HIGH);

    const hitDist = 24; // Horizontal proximity to grab node
    if (Math.abs(x - xLow) < hitDist) {
      activeDragNode.current = 'low';
      canvas.setPointerCapture(e.pointerId);
    } else if (Math.abs(x - xMid) < hitDist) {
      activeDragNode.current = 'mid';
      canvas.setPointerCapture(e.pointerId);
    } else if (Math.abs(x - xHigh) < hitDist) {
      activeDragNode.current = 'high';
      canvas.setPointerCapture(e.pointerId);
    }
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!activeDragNode.current) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const padT = 16;
    const padB = 22;
    const plotH = rect.height - padT - padB;

    // Y position to dB (-12 to +12 dB)
    const normY = Math.max(0, Math.min(1, (y - padT) / plotH));
    const newDb = Math.round((MAX_DB - normY * DB_SPAN) * 10) / 10;
    const clampedDb = Math.max(-12, Math.min(12, newDb));

    if (activeDragNode.current === 'low' && onLowGainChange) onLowGainChange(clampedDb);
    else if (activeDragNode.current === 'mid' && onMidGainChange) onMidGainChange(clampedDb);
    else if (activeDragNode.current === 'high' && onHighGainChange) onHighGainChange(clampedDb);
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (activeDragNode.current) {
      activeDragNode.current = null;
      try {
        canvasRef.current?.releasePointerCapture(e.pointerId);
      } catch {
        // Safe fallback
      }
    }
  };

  return (
    <div className="relative w-full h-full select-none cursor-ns-resize">
      <canvas
        ref={canvasRef}
        className="w-full h-full block"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      />
    </div>
  );
};
export default BodePlotCanvas;
