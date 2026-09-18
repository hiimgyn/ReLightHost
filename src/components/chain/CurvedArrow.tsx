import { theme } from 'antd';

interface CurvedArrowProps {
  width?: number;
  height?: number;
  color?: string;
  active?: boolean;
}

export default function CurvedArrow({ width = 28, height = 24, color, active = false }: CurvedArrowProps) {
  const { token } = theme.useToken();
  const stroke = color ?? (active ? token.colorPrimary : token.colorTextTertiary);
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{
        display: 'block',
        filter: active ? `drop-shadow(0 0 4px ${token.colorPrimary})` : 'none',
        transition: 'filter 240ms ease',
      }}
    >
      <path
        d="M2 12 C8 4, 16 4, 22 12"
        stroke={stroke}
        strokeWidth={active ? 1.8 : 1.4}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeOpacity={active ? 0.95 : 0.68}
        fill="none"
        strokeDasharray={active ? '5 3' : '6 4'}
        className={active ? 'rh-flowing-signal' : undefined}
      />
      <path
        d="M20 10 L22 12 L20 14"
        stroke={stroke}
        strokeWidth={active ? 1.9 : 1.6}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeOpacity={active ? 1 : 0.85}
        fill="none"
      />
    </svg>
  );
}
