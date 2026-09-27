import React from 'react';
import { useThemeStore } from '../../../stores/themeStore';

interface VisualStageContainerProps {
  title?: string;
  badge?: React.ReactNode;
  height?: number | string;
  children: React.ReactNode;
  className?: string;
  controls?: React.ReactNode;
}

export const VisualStageContainer: React.FC<VisualStageContainerProps> = ({
  title,
  badge,
  height = 180,
  children,
  className = '',
  controls,
}) => {
  const isDark = useThemeStore((s) => s.theme === 'dark');

  return (
    <div
      className={`relative rounded-xl overflow-hidden flex flex-col border ${
        isDark ? 'border-white/10' : 'border-slate-200'
      } ${className}`}
      style={{
        height,
        background: isDark
          ? 'linear-gradient(180deg, #0e1118 0%, #080a0e 100%)'
          : 'linear-gradient(180deg, #ffffff 0%, #f1f5f9 100%)',
        boxShadow: isDark
          ? 'inset 0 1px 1px rgba(255, 255, 255, 0.05), 0 8px 24px rgba(0, 0, 0, 0.45)'
          : 'inset 0 1px 1px rgba(255, 255, 255, 0.8), 0 4px 14px rgba(0, 0, 0, 0.06)',
      }}
    >
      {/* Top Bar (Title & Mode Badges) */}
      {(title || badge || controls) && (
        <div
          className={`flex items-center justify-between px-3.5 py-2 border-b select-none z-10 ${
            isDark
              ? 'border-white/5 bg-white/[0.02]'
              : 'border-slate-200/80 bg-slate-50/80'
          }`}
        >
          <div className="flex items-center gap-2">
            {title && (
              <span
                className={`text-[11px] font-semibold tracking-wider uppercase ${
                  isDark ? 'text-white/70' : 'text-slate-700'
                }`}
              >
                {title}
              </span>
            )}
            {badge}
          </div>
          {controls && <div className="flex items-center gap-2">{controls}</div>}
        </div>
      )}

      {/* Main Visual Content */}
      <div className="relative flex-1 w-full h-full overflow-hidden">
        {/* Subtle Background Grid Lines */}
        <div
          className="absolute inset-0 pointer-events-none opacity-20"
          style={{
            backgroundImage: `
              linear-gradient(to right, ${isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(0, 0, 0, 0.06)'} 1px, transparent 1px),
              linear-gradient(to bottom, ${isDark ? 'rgba(255, 255, 255, 0.08)' : 'rgba(0, 0, 0, 0.06)'} 1px, transparent 1px)
            `,
            backgroundSize: '24px 24px',
          }}
        />

        {children}
      </div>
    </div>
  );
};
export default VisualStageContainer;
