import { Space, Tag } from 'antd';
import type { AudioDeviceInfo } from '../../lib/types';

export const isAsioId = (id: string | null | undefined): boolean =>
  id?.startsWith('asio_') ?? false;

export const isAsioHost = (ht: string): boolean => ht.toLowerCase().includes('asio');

export function getHostTypeColor(hostType: string) {
  if (isAsioHost(hostType)) return 'blue';
  if (hostType.includes('WASAPI')) return 'green';
  if (hostType.includes('DirectSound')) return 'orange';
  if (hostType.includes('CoreAudio')) return 'purple';
  return 'default';
}

export function DeviceOption({ device }: { device: AudioDeviceInfo }) {
  const channelLabel =
    device.input_channels > 0 && device.output_channels > 0
      ? `${device.input_channels} in / ${device.output_channels} out`
      : device.output_channels > 0
        ? `${device.output_channels} ch out`
        : `${device.input_channels} ch in`;

  return (
    <Space orientation="vertical" size={0} style={{ width: '100%' }}>
      <Space>
        <span style={{ fontWeight: 600 }}>{device.name}</span>
        {device.is_default && <Tag color="blue">Default</Tag>}
      </Space>
      <Space size={4}>
        <Tag color={getHostTypeColor(device.host_type)}>{device.host_type}</Tag>
        <Tag color="cyan">{channelLabel}</Tag>
      </Space>
    </Space>
  );
}
