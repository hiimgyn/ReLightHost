import { theme } from 'antd';

interface CurvedArrowProps {
  width?: number;
  height?: number;
  color?: string;
}

export default function CurvedArrow({ width = 28, height = 24, color }: CurvedArrowProps) {
  const { token } = theme.useToken();
  const stroke = color ?? token.colorTextQuaternary;
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{ display: 'block' }}
    >
      <path
        d="M2 12 C8 4, 16 4, 22 12"
        stroke={stroke}
        strokeWidth={1.4}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeOpacity={0.52}
        fill="none"
        strokeDasharray="6 4"
      />
      <path
        d="M20 10 L22 12 L20 14"
        stroke={stroke}
        strokeWidth={1.6}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeOpacity={0.7}
        fill="none"
      />
    </svg>
  );
}
