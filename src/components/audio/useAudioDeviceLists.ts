import { useMemo } from 'react';
import type { AudioDeviceInfo } from '../../lib/types';
import { isAsioHost, isAsioId } from './audioDeviceDisplay';

interface UseAudioDeviceListsOptions {
  devices: AudioDeviceInfo[];
  selectedHostType: string;
  selectedDevice: string | null;
  selectedInputDevice: string | null;
}

/** Derives the API/device dropdown lists from the raw device list + current selection. */
export function useAudioDeviceLists({
  devices,
  selectedHostType,
  selectedDevice,
  selectedInputDevice,
}: UseAudioDeviceListsOptions) {
  const hostTypes = useMemo(
    () => Array.from(new Set(devices.map((d) => d.host_type))).sort(),
    [devices],
  );

  // If no host type has been selected in the UI yet, infer from the stored
  // device ID — "asio_*" devices are always ASIO regardless of the dropdown.
  // This ensures the ASIO section is shown immediately on first render rather
  // than waiting for the async setSelectedHostType state update.
  const asioMode =
    isAsioHost(selectedHostType) ||
    (!selectedHostType && isAsioId(selectedInputDevice ?? selectedDevice));

  // Partition devices for the current host type
  const filteredDevices = useMemo(
    () => (selectedHostType ? devices.filter((d) => d.host_type === selectedHostType) : devices),
    [devices, selectedHostType],
  );

  // Full-duplex ASIO devices (both channels populated)
  const asioDevices = useMemo(
    () => filteredDevices.filter((d) => d.input_channels > 0 && d.output_channels > 0),
    [filteredDevices],
  );
  // Separate output / input lists for non-ASIO
  const outputDevices = useMemo(
    () => filteredDevices.filter((d) => d.output_channels > 0 && d.input_channels === 0),
    [filteredDevices],
  );
  const inputDevices = useMemo(
    () => filteredDevices.filter((d) => d.input_channels > 0 && d.output_channels === 0),
    [filteredDevices],
  );
  const monitorOutputDevices = useMemo(
    () =>
      devices.filter(
        (d) => d.output_channels > 0 && d.input_channels === 0 && !isAsioHost(d.host_type),
      ),
    [devices],
  );
  const defaultMonitorOutputId = monitorOutputDevices.find((d) => d.is_default)?.id;

  return {
    hostTypes,
    asioMode,
    filteredDevices,
    asioDevices,
    outputDevices,
    inputDevices,
    monitorOutputDevices,
    defaultMonitorOutputId,
  };
}
