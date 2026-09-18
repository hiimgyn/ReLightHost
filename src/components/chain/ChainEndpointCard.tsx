import { Card, Tooltip, theme } from 'antd';
import { AudioOutlined } from '@ant-design/icons';
import type { ReactNode } from 'react';

interface ChainEndpointCardProps {
  variant: 'in' | 'out';
  tooltipTitle: ReactNode;
  active?: boolean;
}

/**
 * Decorative signal-chain endpoint node (IN or OUT). The two variants share
 * identical layout — only the color role (success vs info) and label differ.
 */
export default function ChainEndpointCard({ variant, tooltipTitle, active = false }: ChainEndpointCardProps) {
  const { token } = theme.useToken();
  const isIn = variant === 'in';
  const label = isIn ? 'INPUT' : 'OUTPUT';
  const badge = isIn ? 'IN' : 'OUT';

  const accent = isIn
    ? {
        bg: token.colorSuccessBg,
        border: active ? token.colorSuccess : token.colorSuccessBorder,
        dot: token.colorSuccess,
        badgeBg: token.colorSuccessBgHover,
        hover: token.colorSuccessHover,
        boxShadow: active
          ? `0 14px 32px rgba(2,6,23,0.42), 0 0 16px ${token.colorSuccess}44, inset 0 1px 0 rgba(255,255,255,0.06)`
          : `0 14px 32px rgba(2,6,23,0.42), inset 0 1px 0 rgba(255,255,255,0.06)`,
      }
    : {
        bg: token.colorInfoBg,
        border: active ? token.colorPrimary : token.colorInfoBorder,
        dot: token.colorInfo,
        badgeBg: token.colorInfoBgHover,
        hover: token.colorInfoHover,
        boxShadow: active
          ? `0 14px 32px rgba(2,6,23,0.32), 0 0 16px ${token.colorPrimary}44`
          : 'none',
      };

  return (
    <Tooltip title={tooltipTitle}>
      <div style={{ position: 'relative', width: 148, flexShrink: 0, borderRadius: 18 }}>
        <Card
          className="glass-card"
          style={{
            width: '100%',
            height: 145,
            flexShrink: 0,
            overflow: 'hidden',
            transition: 'all 300ms ease',
          }}
          styles={{
            body: {
              position: 'relative',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'stretch',
              justifyContent: 'space-between',
              padding: 12,
              height: '100%',
              background: `linear-gradient(160deg, ${accent.bg} 0%, ${token.colorBgContainer} 55%, ${token.colorFillQuaternary} 100%)`,
              border: `1px solid ${accent.border}`,
              boxShadow: accent.boxShadow,
            },
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
            <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: 999,
                  background: accent.dot,
                  boxShadow: active ? `0 0 14px ${accent.dot}` : 'none',
                  animation: active ? 'rh-pulse-glow 1.8s ease-in-out infinite' : 'none',
                }}
              />
              <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.8, color: token.colorTextSecondary }}>
                {label}
              </span>
            </div>
            <span
              style={{
                padding: '3px 8px',
                borderRadius: 999,
                background: accent.badgeBg,
                color: accent.dot,
                fontSize: 10,
                fontWeight: 700,
                letterSpacing: 0.4,
                boxShadow: active ? `0 0 8px ${accent.dot}44` : 'none',
              }}
            >
              {badge}
            </span>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flex: 1 }}>
            <div
              style={{
                position: 'relative',
                width: 64,
                height: 64,
                borderRadius: '50%',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                background: `radial-gradient(circle at 35% 35%, ${accent.hover} 0%, ${accent.badgeBg} 45%, ${token.colorBgContainer} 100%)`,
                border: `1px solid ${accent.border}`,
                boxShadow: active ? `0 0 20px ${accent.dot}55, 0 10px 24px ${accent.bg}` : 'none',
                transition: 'all 300ms ease',
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
              <AudioOutlined
                style={{
                  fontSize: 22,
                  color: accent.dot,
                  filter: active ? `drop-shadow(0 0 6px ${accent.dot})` : 'none',
                  transition: 'filter 240ms ease',
                }}
              />
            </div>
          </div>
        </Card>
      </div>
    </Tooltip>
  );
}
