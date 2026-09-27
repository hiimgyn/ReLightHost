import React from 'react';
import { Badge, Tooltip } from 'antd';
import { Power, RotateCcw } from 'lucide-react';
import { useThemeStore } from '../../../stores/themeStore';

interface PluginHeaderProps {
  title: string;
  subtitle?: string;
  badgeText?: string;
  badgeColor?: string;
  enabled?: boolean;
  onToggleEnabled?: () => void;
  onResetAll?: () => void;
  icon?: React.ReactNode;
}

export const PluginHeader: React.FC<PluginHeaderProps> = ({
  title,
  subtitle,
  badgeText = 'SYSTEM DSP',
  badgeColor = '#6366f1',
  enabled = true,
  onToggleEnabled,
  onResetAll,
  icon,
}) => {
  const isDark = useThemeStore((s) => s.theme === 'dark');

  return (
    <div
      className={`flex items-center justify-between pb-3 mb-3 border-b select-none ${
        isDark ? 'border-white/10' : 'border-slate-200'
      }`}
    >
      <div className="flex items-center gap-3">
        {onToggleEnabled && (
          <Tooltip title={enabled ? 'Bypass Plugin' : 'Activate Plugin'}>
            <button
              onClick={onToggleEnabled}
              className={`p-2 rounded-lg border transition-all cursor-pointer flex items-center justify-center ${
                enabled
                  ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-400 shadow-[0_0_12px_rgba(16,185,129,0.3)]'
                  : isDark
                  ? 'bg-white/5 border-white/10 text-white/40 hover:text-white/70'
                  : 'bg-slate-100 border-slate-200 text-slate-400 hover:text-slate-700'
              }`}
            >
              <Power size={14} className={enabled ? 'stroke-[2.5]' : 'stroke-[1.5]'} />
            </button>
          </Tooltip>
        )}

        {icon && (
          <div className={isDark ? 'text-white/70' : 'text-slate-700'}>
            {icon}
          </div>
        )}

        <div>
          <div className="flex items-center gap-2">
            <h2
              className={`text-sm font-bold tracking-wide uppercase m-0 leading-none ${
                isDark ? 'text-white' : 'text-slate-900'
              }`}
            >
              {title}
            </h2>
            <Badge
              count={badgeText}
              style={{
                backgroundColor: `${badgeColor}22`,
                color: badgeColor,
                borderColor: `${badgeColor}55`,
                fontSize: 9,
                fontWeight: 700,
                letterSpacing: '0.06em',
                padding: '0 6px',
                height: 18,
                lineHeight: '16px',
              }}
            />
          </div>
          {subtitle && (
            <p
              className={`text-[11px] m-0 mt-1 leading-tight ${
                isDark ? 'text-white/50' : 'text-slate-500'
              }`}
            >
              {subtitle}
            </p>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2">
        {onResetAll && (
          <Tooltip title="Reset all parameters to factory defaults">
            <button
              onClick={onResetAll}
              className={`flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium rounded-md transition-colors cursor-pointer border ${
                isDark
                  ? 'text-white/60 hover:text-white bg-white/5 hover:bg-white/10 border-white/10'
                  : 'text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 border-slate-300'
              }`}
            >
              <RotateCcw size={11} />
              <span>Defaults</span>
            </button>
          </Tooltip>
        )}
      </div>
    </div>
  );
};
export default PluginHeader;
