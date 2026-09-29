import React, { useRef, useEffect } from 'react';
import { useWindowVisibility } from '../../../lib/windowVisibility';
import { useThemeStore } from '../../../stores/themeStore';
import { getPluginScope } from '../../../lib/tauri';
import { useTranslation } from '../../../i18n';

interface WaveformDualCanvasProps {
  /** Plugin instance whose input/output levels are drawn. */
  instanceId: string;
  active?: boolean;
}

const POLL_MS = 33;

/**
 * Real before/after level history of one plugin: the backend keeps a peak
 * pair per 10 ms window (≈2 s); this polls it and draws both envelopes
 * mirrored around the centre line — input dim, output bright.
 */
export const WaveformDualCanvas: React.FC<WaveformDualCanvasProps> = ({ instanceId, active = true }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sizeRef = useRef({ width: 0, height: 0 });
  const isWindowVisible = useWindowVisibility();
  const isDark = useThemeStore((s) => s.theme === 'dark');
  const { t } = useTranslation();
  const inputLabel = t('common.scopeInput');
  const outputLabel = t('common.scopeOutput');

  // Track the canvas size without a layout read every frame.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      sizeRef.current = { width, height };
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!isWindowVisible || !active) return;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;

    let cancelled = false;
    let timer: number | undefined;

    // Peaks → height: sqrt lifts quiet material so speech and noise floor
    // are both visible without a full dB scale.
    const level = (peak: number) => Math.sqrt(Math.min(1, Math.max(0, peak)));

    const draw = (points: [number, number][]) => {
      const { width, height } = sizeRef.current;
      if (width === 0 || height === 0) return;
      const dpr = window.devicePixelRatio || 1;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, width, height);

      const centerY = height / 2;
      const amp = height * 0.42;
      ctx.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.06)' : 'rgba(0, 0, 0, 0.08)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, centerY);
      ctx.lineTo(width, centerY);
      ctx.stroke();

      if (points.length > 1) {
        const stepX = width / (points.length - 1);
        const envelope = (pick: (p: [number, number]) => number, fill: string) => {
          ctx.beginPath();
          points.forEach((p, i) => {
            const y = centerY - level(pick(p)) * amp;
            if (i === 0) ctx.moveTo(0, y);
            else ctx.lineTo(i * stepX, y);
          });
          for (let i = points.length - 1; i >= 0; i--) {
            ctx.lineTo(i * stepX, centerY + level(pick(points[i])) * amp);
          }
          ctx.closePath();
          ctx.fillStyle = fill;
          ctx.fill();
        };
        envelope((p) => p[0], isDark ? 'rgba(245, 158, 11, 0.28)' : 'rgba(217, 119, 6, 0.30)');
        envelope((p) => p[1], isDark ? 'rgba(0, 240, 255, 0.55)' : 'rgba(8, 145, 178, 0.55)');
      }

      ctx.font = '9px Inter, sans-serif';
      ctx.fillStyle = isDark ? '#00f0ff' : '#0891b2';
      ctx.fillText(`● ${outputLabel}`, 10, 16);
      ctx.fillStyle = isDark ? 'rgba(245, 158, 11, 0.8)' : 'rgba(217, 119, 6, 0.9)';
      ctx.fillText(`● ${inputLabel}`, 90, 16);
    };

    const poll = async () => {
      try {
        const points = await getPluginScope(instanceId);
        if (!cancelled) draw(points);
      } catch {
        /* instance removed or backend busy — try again next tick */
      } finally {
        if (!cancelled) timer = window.setTimeout(poll, POLL_MS);
      }
    };
    poll();

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [instanceId, active, isWindowVisible, isDark, inputLabel, outputLabel]);

  return (
    <div className="relative w-full h-full select-none">
      <canvas ref={canvasRef} className="w-full h-full block" />
    </div>
  );
};
export default WaveformDualCanvas;
