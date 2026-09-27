import React, { useRef, useEffect } from 'react';
import { useWindowVisibility } from '../../../lib/windowVisibility';
import { useThemeStore } from '../../../stores/themeStore';

interface WaveformDualCanvasProps {
  vad: number;          // 0.0 to 1.0
  reductionDb: number;  // 0.0 to 60.0 dB
  active?: boolean;
}

export const WaveformDualCanvas: React.FC<WaveformDualCanvasProps> = ({
  vad,
  reductionDb,
  active = true,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bufferRef = useRef<{ raw: number[]; clean: number[] }>({
    raw: new Array(160).fill(0),
    clean: new Array(160).fill(0),
  });
  const timeRef = useRef(0);
  const isWindowVisible = useWindowVisibility();

  const isDark = useThemeStore((s) => s.theme === 'dark');

  useEffect(() => {
    if (!isWindowVisible || !active) return;
    let animId: number;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const render = () => {
      timeRef.current += 0.05;
      const t = timeRef.current;

      // Handle resize/DPR
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

      // Generate waveform history point based on simulated/live VAD and reduction
      const buf = bufferRef.current;
      const speechSignal = vad > 0.15 ? Math.sin(t * 8) * 0.45 * vad + Math.sin(t * 19) * 0.25 * vad : 0;
      const noiseSignal = (Math.sin(t * 43) * 0.12 + Math.cos(t * 97) * 0.09) * (active ? 1 : 0);

      const rawSample = Math.max(-0.95, Math.min(0.95, speechSignal + noiseSignal));
      // Attenuate noise component according to reductionDb
      const noiseAtten = Math.pow(10, -reductionDb / 20);
      const cleanSample = Math.max(-0.95, Math.min(0.95, speechSignal + noiseSignal * noiseAtten));

      buf.raw.push(rawSample);
      buf.raw.shift();
      buf.clean.push(cleanSample);
      buf.clean.shift();

      const centerY = height / 2;
      const amp = height * 0.42;

      // Center reference line
      ctx.strokeStyle = isDark ? 'rgba(255, 255, 255, 0.06)' : 'rgba(0, 0, 0, 0.08)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, centerY);
      ctx.lineTo(width, centerY);
      ctx.stroke();

      const numPoints = buf.raw.length;
      const stepX = width / (numPoints - 1);

      // 1. Draw Raw Input Waveform (Dim Amber / Red)
      ctx.beginPath();
      buf.raw.forEach((val, i) => {
        const x = i * stepX;
        const y = centerY - val * amp;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.strokeStyle = isDark ? 'rgba(245, 158, 11, 0.35)' : 'rgba(217, 119, 6, 0.55)';
      ctx.lineWidth = 1.5;
      ctx.stroke();

      // 2. Draw Clean Speech Waveform (Bright Electric Cyan)
      ctx.beginPath();
      buf.clean.forEach((val, i) => {
        const x = i * stepX;
        const y = centerY - val * amp;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.strokeStyle = isDark ? '#00f0ff' : '#0891b2';
      ctx.lineWidth = 2.0;
      ctx.shadowColor = isDark ? '#00f0ff' : '#0891b2';
      ctx.shadowBlur = isDark ? 6 : 2;
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Legend overlay
      ctx.font = '9px Inter, sans-serif';
      ctx.fillStyle = isDark ? '#00f0ff' : '#0891b2';
      ctx.fillText('● CLEAN SPEECH', 10, 16);
      ctx.fillStyle = isDark ? 'rgba(245, 158, 11, 0.7)' : 'rgba(217, 119, 6, 0.85)';
      ctx.fillText('● RAW NOISE FLOOR', 105, 16);

      ctx.restore();
      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);
    return () => cancelAnimationFrame(animId);
  }, [vad, reductionDb, active, isWindowVisible, isDark]);

  return (
    <div className="relative w-full h-full select-none">
      <canvas ref={canvasRef} className="w-full h-full block" />
    </div>
  );
};
export default WaveformDualCanvas;
