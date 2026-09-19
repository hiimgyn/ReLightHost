import { Tooltip, theme } from 'antd';
import { Mic, Volume2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from '../../i18n';

interface ChainEndpointCardProps {
  variant: 'in' | 'out';
  tooltipTitle: ReactNode;
  active?: boolean;
}

/**
 * Signal-chain terminal node (IN or OUT) — a jack, not a plugin. Deliberately
 * card-free (no border box, no fixed 192px height matching PluginCard) so it
 * reads as the wire's endpoint rather than a third kind of rack module.
 */
export default function ChainEndpointCard({ variant, tooltipTitle, active = false }: ChainEndpointCardProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const isIn = variant === 'in';
  const label = isIn ? t('chain.inBadge') : t('chain.outBadge');
  const srLabel = isIn ? t('chain.input') : t('chain.output');

  const accent = isIn
    ? {
        dot: token.colorSuccess,
        hover: token.colorSuccessHover,
        badgeBg: token.colorSuccessBgHover,
      }
    : {
        dot: token.colorInfo,
        hover: token.colorInfoHover,
        badgeBg: token.colorInfoBgHover,
      };

  return (
    <Tooltip title={tooltipTitle} overlayClassName="io-node-tooltip">
      <div
        aria-label={srLabel}
        style={{
          position: 'relative',
          width: 88,
          flexShrink: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 8,
          cursor: 'default',
        }}
      >
        <div
          style={{
            position: 'relative',
            width: 56,
            height: 56,
            borderRadius: '50%',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            background: `radial-gradient(circle at 35% 32%, ${accent.hover} 0%, ${accent.badgeBg} 46%, ${token.colorBgContainer} 100%)`,
            border: `1px solid ${active ? accent.dot : token.colorBorder}`,
            boxShadow: active ? `0 0 18px ${accent.dot}55, 0 6px 16px rgba(2,6,23,0.3)` : '0 2px 8px rgba(2,6,23,0.16)',
            transition: 'all 260ms ease',
          }}
        >
          {active && (
            <span
              style={{
                position: 'absolute',
                inset: -4,
                borderRadius: '50%',
                border: `1px solid ${accent.dot}66`,
                animation: 'rh-ring-wave 2s cubic-bezier(0.25, 0.8, 0.25, 1) infinite',
                pointerEvents: 'none',
              }}
            />
          )}
          {isIn ? (
            <Mic
              size={20}
              style={{ color: accent.dot, filter: active ? `drop-shadow(0 0 6px ${accent.dot})` : 'none', transition: 'filter 240ms ease' }}
            />
          ) : (
            <Volume2
              size={20}
              style={{ color: accent.dot, filter: active ? `drop-shadow(0 0 6px ${accent.dot})` : 'none', transition: 'filter 240ms ease' }}
            />
          )}
          {/* Jack pin — the stub the signal wire visually plugs into */}
          <span
            style={{
              position: 'absolute',
              top: '50%',
              [isIn ? 'right' : 'left']: -7,
              transform: 'translateY(-50%)',
              width: 7,
              height: 3,
              borderRadius: 2,
              background: active ? accent.dot : token.colorBorder,
              boxShadow: active ? `0 0 6px ${accent.dot}` : 'none',
              transition: 'all 260ms ease',
            }}
          />
        </div>

        <span
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: 5,
            fontSize: 10,
            fontWeight: 700,
            letterSpacing: 1,
            textTransform: 'uppercase',
            color: active ? accent.dot : token.colorTextTertiary,
            transition: 'color 240ms ease',
          }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: '50%',
              background: active ? accent.dot : token.colorTextQuaternary,
              boxShadow: active ? `0 0 6px ${accent.dot}` : 'none',
              animation: active ? 'rh-pulse-glow 1.8s ease-in-out infinite' : 'none',
              flexShrink: 0,
            }}
          />
          {label}
        </span>
      </div>
    </Tooltip>
  );
}
