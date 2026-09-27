import React, { useRef, useState } from 'react';
import { Tooltip } from 'antd';
import { RotateCcw } from 'lucide-react';

export interface AudioKnobProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  defaultValue: number;
  format?: (v: number) => string;
  onChange: (v: number) => void;
  color?: string;
  size?: 'sm' | 'md' | 'lg';
  bipolar?: boolean;
  unit?: string;
  disabled?: boolean;
}

const SIZE_CONFIG = {
  sm: { diameter: 44, stroke: 3.5, labelSize: 10, valueSize: 11, capSize: 32 },
  md: { diameter: 58, stroke: 4.5, labelSize: 11, valueSize: 12, capSize: 42 },
  lg: { diameter: 74, stroke: 5.5, labelSize: 12, valueSize: 14, capSize: 54 },
};

export const AudioKnob: React.FC<AudioKnobProps> = ({
  label,
  value,
  min,
  max,
  step = 0.1,
  defaultValue,
  format = (v) => v.toFixed(1),
  onChange,
  color = '#6366f1',
  size = 'md',
  bipolar = false,
  unit,
  disabled = false,
}) => {
  const cfg = SIZE_CONFIG[size];
  const knobRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [isHovered, setIsHovered] = useState(false);

  // Drag tracking refs
  const dragStartY = useRef(0);
  const dragStartValue = useRef(value);

  // Knob angle geometry: 270 degree total sweep (from -135deg to +135deg)
  const START_ANGLE = -135;
  const END_ANGLE = 135;
  const TOTAL_ANGLE = END_ANGLE - START_ANGLE; // 270 degrees

  // Normalized value (0.0 to 1.0)
  const norm = Math.max(0, Math.min(1, (value - min) / (max - min || 1)));
  const currentAngle = START_ANGLE + norm * TOTAL_ANGLE;

  // Arc path math
  const r = (cfg.diameter - cfg.stroke) / 2;
  const center = cfg.diameter / 2;

  const polarToCartesian = (centerX: number, centerY: number, radius: number, angleInDegrees: number) => {
    // 0 deg is at top (12 o'clock)
    const angleInRadians = ((angleInDegrees - 90) * Math.PI) / 180.0;
    return {
      x: centerX + radius * Math.cos(angleInRadians),
      y: centerY + radius * Math.sin(angleInRadians),
    };
  };

  const describeArc = (x: number, y: number, radius: number, startAngle: number, endAngle: number) => {
    const start = polarToCartesian(x, y, radius, endAngle);
    const end = polarToCartesian(x, y, radius, startAngle);
    const arcSweep = endAngle - startAngle <= 180 ? '0' : '1';
    return ['M', start.x, start.y, 'A', radius, radius, 0, arcSweep, 0, end.x, end.y].join(' ');
  };

  // Background track arc
  const trackPath = describeArc(center, center, r, START_ANGLE, END_ANGLE);

  // Active filled arc
  let activePath = '';
  if (bipolar) {
    const zeroNorm = (0 - min) / (max - min || 1);
    const zeroAngle = START_ANGLE + zeroNorm * TOTAL_ANGLE;
    if (currentAngle >= zeroAngle) {
      activePath = describeArc(center, center, r, zeroAngle, Math.max(zeroAngle + 0.1, currentAngle));
    } else {
      activePath = describeArc(center, center, r, currentAngle, zeroAngle);
    }
  } else {
    activePath = norm > 0.005 ? describeArc(center, center, r, START_ANGLE, currentAngle) : '';
  }

  // Pointer drag handling
  const handlePointerDown = (e: React.PointerEvent) => {
    if (disabled) return;
    setIsDragging(true);
    dragStartY.current = e.clientY;
    dragStartValue.current = value;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!isDragging || disabled) return;
    const deltaY = dragStartY.current - e.clientY; // Upward = positive
    const sensitivity = e.shiftKey ? 0.001 : 0.005; // 5x precision with Shift
    const range = max - min;
    let nextVal = dragStartValue.current + deltaY * sensitivity * range;

    if (step > 0) {
      nextVal = Math.round(nextVal / step) * step;
    }
    nextVal = Math.max(min, Math.min(max, nextVal));
    if (nextVal !== value) {
      onChange(nextVal);
    }
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if (isDragging) {
      setIsDragging(false);
      try {
        (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        // Safe fallback
      }
    }
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!disabled && defaultValue !== undefined) {
      onChange(defaultValue);
    }
  };

  const handleWheel = (e: React.WheelEvent) => {
    if (disabled) return;
    e.preventDefault();
    const direction = e.deltaY < 0 ? 1 : -1;
    const stepMul = e.shiftKey ? 0.2 : 1;
    const delta = (step || (max - min) * 0.02) * direction * stepMul;
    let nextVal = value + delta;
    if (step > 0) {
      nextVal = Math.round(nextVal / step) * step;
    }
    nextVal = Math.max(min, Math.min(max, nextVal));
    if (nextVal !== value) {
      onChange(nextVal);
    }
  };

  return (
    <div
      className="flex flex-col items-center select-none"
      style={{ opacity: disabled ? 0.45 : 1, width: cfg.diameter + 16 }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Parameter Label */}
      <div className="flex items-center justify-center gap-1 mb-1 text-center w-full">
        <span
          style={{
            fontSize: cfg.labelSize,
            color: 'var(--rh-text-muted, rgba(255,255,255,0.65))',
            letterSpacing: '0.04em',
            textTransform: 'uppercase',
            fontWeight: 600,
          }}
          className="truncate"
          title={label}
        >
          {label}
        </span>
        {value !== defaultValue && !disabled && (
          <Tooltip title={`Reset to ${format(defaultValue)}${unit ? ' ' + unit : ''}`}>
            <button
              onClick={() => onChange(defaultValue)}
              className="text-white/30 hover:text-white/80 transition-colors p-0.5 cursor-pointer"
            >
              <RotateCcw size={10} />
            </button>
          </Tooltip>
        )}
      </div>

      {/* Rotary Dial Visual */}
      <div
        ref={knobRef}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onDoubleClick={handleDoubleClick}
        onWheel={handleWheel}
        className="relative flex items-center justify-center cursor-ns-resize touch-none"
        style={{ width: cfg.diameter, height: cfg.diameter }}
      >
        <svg
          width={cfg.diameter}
          height={cfg.diameter}
          className="overflow-visible"
        >
          {/* Subtle Outer Glow Filter */}
          <defs>
            <filter id={`glow-${label}`} x="-20%" y="-20%" width="140%" height="140%">
              <feDropShadow dx="0" dy="0" stdDeviation="2.5" floodColor={color} floodOpacity={isDragging ? 0.6 : 0.3} />
            </filter>
          </defs>

          {/* Background Track Arc */}
          <path
            d={trackPath}
            fill="none"
            stroke="rgba(255, 255, 255, 0.12)"
            strokeWidth={cfg.stroke}
            strokeLinecap="round"
          />

          {/* Active Progress Arc */}
          {activePath && (
            <path
              d={activePath}
              fill="none"
              stroke={color}
              strokeWidth={cfg.stroke}
              strokeLinecap="round"
              filter={`url(#glow-${label})`}
              className="transition-[stroke-width] duration-150"
            />
          )}
        </svg>

        {/* Center Metal Cap */}
        <div
          className="absolute rounded-full flex items-center justify-center shadow-lg transition-transform duration-75"
          style={{
            width: cfg.capSize,
            height: cfg.capSize,
            background: isDragging
              ? 'linear-gradient(145deg, #2b3040, #171a24)'
              : isHovered
              ? 'linear-gradient(145deg, #262a38, #13151e)'
              : 'linear-gradient(145deg, #202431, #101218)',
            border: `1px solid ${isDragging ? color : isHovered ? 'rgba(255,255,255,0.2)' : 'rgba(255,255,255,0.1)'}`,
            boxShadow: isDragging
              ? `0 0 12px ${color}55, inset 0 1px 1px rgba(255,255,255,0.2)`
              : '0 4px 10px rgba(0,0,0,0.5), inset 0 1px 1px rgba(255,255,255,0.12)',
            transform: `rotate(${currentAngle}deg)`,
          }}
        >
          {/* Indicator Notch Line */}
          <div
            className="absolute top-1 rounded-full"
            style={{
              width: cfg.stroke - 1.5,
              height: cfg.capSize * 0.28,
              backgroundColor: isDragging ? '#ffffff' : color,
              boxShadow: `0 0 4px ${color}`,
            }}
          />
        </div>
      </div>

      {/* Numeric Value Readout */}
      <div
        className="mt-1 font-mono font-medium tracking-tight text-center tabular-nums cursor-ns-resize"
        onDoubleClick={handleDoubleClick}
        style={{
          fontSize: cfg.valueSize,
          color: isDragging ? '#ffffff' : 'rgba(255,255,255,0.88)',
          textShadow: isDragging ? `0 0 8px ${color}88` : 'none',
        }}
      >
        {format(value)}
        {unit && <span className="ml-0.5 text-white/50 text-[10px]">{unit}</span>}
      </div>
    </div>
  );
};
export default AudioKnob;
