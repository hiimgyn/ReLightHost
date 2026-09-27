import React from 'react';

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
  return (
    <div
      className={`relative rounded-xl overflow-hidden flex flex-col border border-white/10 ${className}`}
      style={{
        height,
        background: 'linear-gradient(180deg, #0e1118 0%, #080a0e 100%)',
        boxShadow: 'inset 0 1px 1px rgba(255, 255, 255, 0.05), 0 8px 24px rgba(0, 0, 0, 0.45)',
      }}
    >
      {/* Top Bar (Title & Mode Badges) */}
      {(title || badge || controls) && (
        <div className="flex items-center justify-between px-3.5 py-2 border-b border-white/5 bg-white/[0.02] select-none z-10">
          <div className="flex items-center gap-2">
            {title && (
              <span className="text-[11px] font-semibold tracking-wider uppercase text-white/70">
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
              linear-gradient(to right, rgba(255, 255, 255, 0.08) 1px, transparent 1px),
              linear-gradient(to bottom, rgba(255, 255, 255, 0.08) 1px, transparent 1px)
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
