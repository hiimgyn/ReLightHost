import React from 'react';
import { Badge, Tooltip } from 'antd';
import { Power, RotateCcw } from 'lucide-react';

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
  badgeText = 'BUILT-IN DSP',
  badgeColor = '#6366f1',
  enabled = true,
  onToggleEnabled,
  onResetAll,
  icon,
}) => {
  return (
    <div className="flex items-center justify-between pb-3 mb-3 border-b border-white/10 select-none">
      <div className="flex items-center gap-3">
        {onToggleEnabled && (
          <Tooltip title={enabled ? 'Bypass Plugin' : 'Activate Plugin'}>
            <button
              onClick={onToggleEnabled}
              className={`p-2 rounded-lg border transition-all cursor-pointer flex items-center justify-center ${
                enabled
                  ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-400 shadow-[0_0_12px_rgba(16,185,129,0.3)]'
                  : 'bg-white/5 border-white/10 text-white/40 hover:text-white/70'
              }`}
            >
              <Power size={14} className={enabled ? 'stroke-[2.5]' : 'stroke-[1.5]'} />
            </button>
          </Tooltip>
        )}

        {icon && <div className="text-white/70">{icon}</div>}

        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-bold tracking-wide text-white uppercase m-0 leading-none">
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
            <p className="text-[11px] text-white/50 m-0 mt-1 leading-tight">{subtitle}</p>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2">
        {onResetAll && (
          <Tooltip title="Reset all parameters to factory defaults">
            <button
              onClick={onResetAll}
              className="flex items-center gap-1.5 px-2.5 py-1 text-[11px] font-medium text-white/60 hover:text-white bg-white/5 hover:bg-white/10 border border-white/10 rounded-md transition-colors cursor-pointer"
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
