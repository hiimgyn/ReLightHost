import type { GlobalToken } from 'antd';

export function getFormatColor(format: string) {
  switch (format) {
    case 'vst3': return 'purple';
    case 'vst': return 'blue';
    case 'clap': return 'green';
    case 'builtin': return 'cyan';
    default: return 'default';
  }
}

export function getAuthorLabel(author: string) {
  return author === 'Unknown' ? 'Unknown folder' : author;
}

export function getAuthorAccent(token: GlobalToken, author: string) {
  if (author === 'Unknown') return token.colorTextQuaternary;
  const first = author.trim().charAt(0).toUpperCase();
  const code = first ? first.charCodeAt(0) : 0;
  const palette = [token.colorPrimary, token.colorInfo, token.colorSuccess, token.colorWarning];
  return palette[code % palette.length];
}
