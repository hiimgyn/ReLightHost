import React, { useRef, useEffect } from 'react';
import { useWindowVisibility } from '../../../lib/windowVisibility';
import { useThemeStore } from '../../../stores/themeStore';

interface NeuralVADOrbProps {
  vad: number; // 0.0 to 1.0 voice activity probability
  size?: number;
  label?: string;
  className?: string;
}

export const NeuralVADOrb: React.FC<NeuralVADOrbProps> = ({
  vad,
  size = 72,
  label = 'AI SPEECH VAD',
  className = '',
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const smoothedVadRef = useRef(vad);
  const phaseRef = useRef(0);
  const isWindowVisible = useWindowVisibility();
  const isDark = useThemeStore((s) => s.theme === 'dark');

  useEffect(() => {
    if (!isWindowVisible) return;
    let animId: number;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== size * dpr || canvas.height !== size * dpr) {
      canvas.width = size * dpr;
      canvas.height = size * dpr;
    }

    const render = () => {
      // Ballistic smoothing
      smoothedVadRef.current += (vad - smoothedVadRef.current) * 0.15;
      const v = Math.max(0, Math.min(1, smoothedVadRef.current));
      phaseRef.current += 0.04 + v * 0.08;

      ctx.save();
      ctx.scale(dpr, dpr);
      ctx.clearRect(0, 0, size, size);

      const cx = size / 2;
      const cy = size / 2;
      const baseR = size * 0.26;
      const currentR = baseR + v * (size * 0.16);

      // Color interpolation: Slate/Amber (0) -> Bright Aqua/Emerald (1)
      const r = Math.round(56 + v * (16 - 56));
      const g = Math.round(189 + v * (185 - 189));
      const b = Math.round(248 + v * (129 - 248));
      const glowColor = `rgba(${r}, ${g}, ${b}, ${0.2 + v * 0.6})`;

      // 1. Ambient outer energy ripple ring
      if (v > 0.05) {
        const rippleR = currentR + Math.sin(phaseRef.current * 1.5) * (4 + v * 6);
        ctx.beginPath();
        ctx.arc(cx, cy, Math.max(1, rippleR), 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(${r}, ${g}, ${b}, ${0.15 + v * 0.35})`;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }

      // 2. Main pulsing orb body with radial gradient
      const orbGrad = ctx.createRadialGradient(cx, cy, 2, cx, cy, currentR);
      orbGrad.addColorStop(0, `rgba(${r}, ${g}, ${b}, ${0.85 + v * 0.15})`);
      orbGrad.addColorStop(0.6, `rgba(${r}, ${g}, ${b}, ${0.4 + v * 0.4})`);
      orbGrad.addColorStop(1, 'rgba(0,0,0,0)');

      ctx.beginPath();
      ctx.arc(cx, cy, currentR, 0, Math.PI * 2);
      ctx.fillStyle = orbGrad;
      ctx.shadowColor = glowColor;
      ctx.shadowBlur = 12 * (0.5 + v);
      ctx.fill();
      ctx.shadowBlur = 0;

      // 3. Inner core highlight
      ctx.beginPath();
      ctx.arc(cx - currentR * 0.25, cy - currentR * 0.25, currentR * 0.3, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(255, 255, 255, ${0.4 + v * 0.5})`;
      ctx.fill();

      ctx.restore();
      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);
    return () => cancelAnimationFrame(animId);
  }, [vad, size, isWindowVisible]);

  return (
    <div className={`flex flex-col items-center select-none ${className}`}>
      <div className="relative flex items-center justify-center" style={{ width: size, height: size }}>
        <canvas ref={canvasRef} style={{ width: size, height: size }} className="block" />
        <span
          className="absolute font-mono text-[10px] font-bold tabular-nums tracking-tighter"
          style={{
            color: vad > 0.4 ? (isDark ? '#ffffff' : '#047857') : (isDark ? 'rgba(255,255,255,0.4)' : 'rgba(15,23,42,0.45)'),
            textShadow: vad > 0.4 ? '0 0 6px rgba(16,185,129,0.8)' : 'none',
          }}
        >
          {Math.round(vad * 100)}%
        </span>
      </div>
      <span
        className={`text-[9px] uppercase tracking-wider font-semibold mt-0.5 ${
          isDark ? 'text-white/50' : 'text-slate-500'
        }`}
      >
        {label}
      </span>
    </div>
  );
};
export default NeuralVADOrb;
