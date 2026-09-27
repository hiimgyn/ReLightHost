import React, { useRef, useEffect, useCallback } from 'react';

interface TransferCurveCanvasProps {
  threshold: number; // dB, e.g. -60 to 0
  ratio: number;     // e.g. 1 to 20
  knee: number;      // dB, e.g. 0 to 24
  makeup: number;    // dB, e.g. 0 to 24
  liveInputDb?: number; // Live input level in dB (-60 to 0)
  liveGainReductionDb?: number; // Live GR in dB (0 to 24)
  onThresholdChange?: (val: number) => void;
  accentColor?: string;
}

export const TransferCurveCanvas: React.FC<TransferCurveCanvasProps> = ({
  threshold,
  ratio,
  knee,
  makeup,
  liveInputDb = -60,
  liveGainReductionDb = 0,
  onThresholdChange,
  accentColor = '#f59e0b',
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const isDraggingRef = useRef<'threshold' | 'ratio' | null>(null);

  // Compute compressor static curve output in dB for given input dB
  const computeOutputDb = useCallback((x: number) => {
    let y = x;
    const halfKnee = knee / 2;

    if (knee > 0 && x > threshold - halfKnee && x < threshold + halfKnee) {
      // Quadratic soft knee
      const delta = x - threshold + halfKnee;
      y = x + ((1 / ratio - 1) * delta * delta) / (2 * knee);
    } else if (x >= threshold + halfKnee) {
      y = threshold + (x - threshold) / ratio;
    }
    return Math.min(0, Math.max(-60, y + makeup));
  }, [threshold, ratio, knee, makeup]);

  // Coordinate mapping (-60 dB to 0 dB)
  const MIN_DB = -60;
  const MAX_DB = 0;
  const DB_SPAN = MAX_DB - MIN_DB;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Handle high DPI
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

    // Padding for axes & GR meter
    const padL = 28;
    const padR = 36; // Right margin holds the GR meter bar
    const padT = 16;
    const padB = 22;

    const plotW = width - padL - padR;
    const plotH = height - padT - padB;

    const toX = (db: number) => padL + ((db - MIN_DB) / DB_SPAN) * plotW;
    const toY = (db: number) => padT + (1 - (db - MIN_DB) / DB_SPAN) * plotH;

    // 1. Grid & dB labels
    const gridDbs = [-48, -36, -24, -12, 0];
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.06)';
    ctx.lineWidth = 1;
    ctx.font = '9px JetBrains Mono, monospace';
    ctx.fillStyle = 'rgba(255, 255, 255, 0.3)';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';

    gridDbs.forEach((db) => {
      const y = toY(db);
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(padL + plotW, y);
      ctx.stroke();
      ctx.fillText(`${db}`, padL - 4, y);

      const x = toX(db);
      ctx.beginPath();
      ctx.moveTo(x, padT);
      ctx.lineTo(x, padT + plotH);
      ctx.stroke();
      if (db !== 0) {
        ctx.textAlign = 'center';
        ctx.fillText(`${db}`, x, height - 8);
      }
    });

    // 2. 1:1 Unity gain reference dashed line
    ctx.setLineDash([3, 3]);
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.15)';
    ctx.beginPath();
    ctx.moveTo(toX(MIN_DB), toY(MIN_DB));
    ctx.lineTo(toX(MAX_DB), toY(MAX_DB));
    ctx.stroke();
    ctx.setLineDash([]);

    // 3. Compression Transfer Curve
    const points: [number, number][] = [];
    const step = 0.5; // Every 0.5 dB
    for (let db = MIN_DB; db <= MAX_DB; db += step) {
      points.push([toX(db), toY(computeOutputDb(db))]);
    }

    // Gradient fill under the curve
    const grad = ctx.createLinearGradient(0, padT, 0, padT + plotH);
    grad.addColorStop(0, `${accentColor}33`);
    grad.addColorStop(1, `${accentColor}03`);

    ctx.beginPath();
    ctx.moveTo(toX(MIN_DB), toY(MIN_DB));
    points.forEach(([px, py]) => ctx.lineTo(px, py));
    ctx.lineTo(toX(MAX_DB), toY(MIN_DB));
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();

    // Solid curve line
    ctx.beginPath();
    points.forEach(([px, py], i) => {
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    });
    ctx.strokeStyle = accentColor;
    ctx.lineWidth = 2.5;
    ctx.stroke();

    // 4. Threshold & Knee Interactive Handle
    const threshX = toX(threshold);
    const threshY = toY(computeOutputDb(threshold));

    // Knee span highlight
    if (knee > 0) {
      const kLeftX = toX(threshold - knee / 2);
      const kRightX = toX(threshold + knee / 2);
      ctx.fillStyle = `${accentColor}18`;
      ctx.fillRect(kLeftX, padT, kRightX - kLeftX, plotH);
    }

    // Knee pivot handle circle
    ctx.beginPath();
    ctx.arc(threshX, threshY, 6, 0, Math.PI * 2);
    ctx.fillStyle = '#ffffff';
    ctx.fill();
    ctx.strokeStyle = accentColor;
    ctx.lineWidth = 2;
    ctx.stroke();

    // 5. Live Input & Output Dot (Signal Tracker)
    if (liveInputDb > MIN_DB) {
      const liveX = toX(Math.max(MIN_DB, Math.min(MAX_DB, liveInputDb)));
      const liveY = toY(computeOutputDb(liveInputDb));

      ctx.beginPath();
      ctx.arc(liveX, liveY, 4.5, 0, Math.PI * 2);
      ctx.fillStyle = '#38bdf8';
      ctx.shadowColor = '#38bdf8';
      ctx.shadowBlur = 8;
      ctx.fill();
      ctx.shadowBlur = 0;
    }

    // 6. Right-Edge Gain Reduction (GR) Meter Bar
    const grX = width - padR + 10;
    const grW = 8;
    const grH = plotH;

    // Meter track
    ctx.fillStyle = 'rgba(255, 255, 255, 0.05)';
    ctx.fillRect(grX, padT, grW, grH);

    // Active GR fill (drops down from 0dB at top)
    const grClamped = Math.max(0, Math.min(24, liveGainReductionDb));
    const grHeight = (grClamped / 24) * grH;

    if (grHeight > 0) {
      const grGrad = ctx.createLinearGradient(0, padT, 0, padT + grH);
      grGrad.addColorStop(0, '#f59e0b');
      grGrad.addColorStop(0.5, '#ef4444');
      grGrad.addColorStop(1, '#b91c1c');

      ctx.fillStyle = grGrad;
      ctx.fillRect(grX, padT, grW, grHeight);
    }

    // GR label
    ctx.textAlign = 'center';
    ctx.fillStyle = 'rgba(255, 255, 255, 0.4)';
    ctx.font = '8px monospace';
    ctx.fillText('GR', grX + grW / 2, height - 8);

    ctx.restore();
  }, [threshold, ratio, knee, makeup, liveInputDb, liveGainReductionDb, accentColor, computeOutputDb]);

  // Pointer drag to adjust threshold or ratio on the canvas
  const handlePointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const padL = 28;
    const padR = 36;
    const plotW = rect.width - padL - padR;

    const clickedDb = MIN_DB + ((x - padL) / plotW) * DB_SPAN;
    if (Math.abs(clickedDb - threshold) < 12 && onThresholdChange) {
      isDraggingRef.current = 'threshold';
      canvas.setPointerCapture(e.pointerId);
    }
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (!isDraggingRef.current) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const padL = 28;
    const padR = 36;
    const plotW = rect.width - padL - padR;

    const newDb = Math.max(-60, Math.min(0, MIN_DB + ((x - padL) / plotW) * DB_SPAN));
    if (isDraggingRef.current === 'threshold' && onThresholdChange) {
      onThresholdChange(Math.round(newDb * 10) / 10);
    }
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (isDraggingRef.current) {
      isDraggingRef.current = null;
      try {
        canvasRef.current?.releasePointerCapture(e.pointerId);
      } catch {
        // Safe fallback
      }
    }
  };

  return (
    <div className="relative w-full h-full select-none cursor-crosshair">
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
export default TransferCurveCanvas;
