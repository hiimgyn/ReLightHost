import { useEffect, useState } from 'react';
import { Modal, Form, Select, Button, Divider, Tag, Space, message, Alert } from 'antd';
import { CheckOutlined, AudioOutlined, ThunderboltOutlined } from '@ant-design/icons';
import { useAudioStore } from '../stores/audioStore';
import type { AudioDeviceInfo } from '../lib/types';

interface AudioSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

const isAsioId   = (id: string | null | undefined): boolean => id?.startsWith('asio_') ?? false;
const isAsioHost = (ht: string): boolean => ht.toLowerCase().includes('asio');

function getHostTypeColor(hostType: string) {
  if (isAsioHost(hostType))            return 'blue';
  if (hostType.includes('WASAPI'))     return 'green';
  if (hostType.includes('DirectSound')) return 'orange';
  if (hostType.includes('CoreAudio'))  return 'purple';
  return 'default';
}

function DeviceOption({ device }: { device: AudioDeviceInfo }) {
  const channelLabel = device.input_channels > 0 && device.output_channels > 0
    ? `${device.input_channels} in / ${device.output_channels} out`
    : device.output_channels > 0
      ? `${device.output_channels} ch out`
      : `${device.input_channels} ch in`;

  return (
    <Space direction="vertical" size={0} style={{ width: '100%' }}>
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

export default function AudioSettings({ isOpen, onClose }: AudioSettingsProps) {
  const {
    devices, selectedDevice, selectedInputDevice,
    sampleRate, bufferSize,
    setOutputDevice, setInputDevice, setSampleRate, setBufferSize,
    toggleMonitoring, fetchDevices, fetchStatus,
  } = useAudioStore();
  const [form] = Form.useForm();
  const [selectedHostType, setSelectedHostType] = useState<string>('');

  const hostTypes = Array.from(new Set(devices.map(d => d.host_type))).sort();
  const asioMode  = isAsioHost(selectedHostType);

  // Partition devices for the current host type
  const filteredDevices = selectedHostType
    ? devices.filter(d => d.host_type === selectedHostType)
    : devices;

  // Full-duplex ASIO devices (both channels populated)
  const asioDevices  = filteredDevices.filter(d => d.input_channels > 0 && d.output_channels > 0);
  // Separate output / input lists for non-ASIO
  const outputDevices = filteredDevices.filter(d => d.output_channels > 0 && d.input_channels === 0);
  const inputDevices  = filteredDevices.filter(d => d.input_channels  > 0 && d.output_channels === 0);

  useEffect(() => {
    if (!isOpen) return;
    fetchDevices();

    // Auto-detect host type from the currently stored device
    const currentIsAsio = isAsioId(selectedDevice);
    if (currentIsAsio && !selectedHostType) {
      const stored = devices.find(d => d.id === selectedDevice);
      if (stored) setSelectedHostType(stored.host_type);
    }

    form.setFieldsValue({
      asioDevice:   currentIsAsio ? selectedDevice : undefined,
      outputDevice: !currentIsAsio ? (selectedDevice ?? undefined) : undefined,
      inputDevice:  !currentIsAsio ? (selectedInputDevice || '') : '',
      sampleRate:   String(sampleRate),
      bufferSize:   String(bufferSize),
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  const handleHostTypeChange = (ht: string) => {
    setSelectedHostType(ht);
    // Clear device selections so user picks from the new API's list
    form.setFieldsValue({ asioDevice: undefined, outputDevice: undefined, inputDevice: '' });
  };

  const handleApply = async () => {
    try {
      const values = await form.validateFields();

      // Stop the always-running stream before changing config, then restart it.
      await toggleMonitoring(false);

      const tasks: Promise<void>[] = [];

      if (asioMode) {
        // ASIO is full-duplex: one device ID handles both I/O
        if (values.asioDevice) {
          tasks.push(setOutputDevice(values.asioDevice));
          tasks.push(setInputDevice(values.asioDevice));
        }
      } else {
        if (values.outputDevice) tasks.push(setOutputDevice(values.outputDevice));
        tasks.push(setInputDevice(values.inputDevice || null));
      }

      tasks.push(setSampleRate(parseInt(values.sampleRate)));
      tasks.push(setBufferSize(parseInt(values.bufferSize)));

      await Promise.all(tasks);

      // Always restart the stream after config change.
      await toggleMonitoring(true);
      await fetchStatus();
      message.success('Audio settings applied — stream restarted');
      onClose();
    } catch (error) {
      console.error('Failed to apply settings:', error);
      message.error('Failed to apply audio settings');
    }
  };

  return (
    <Modal
      title={
        <Space>
          <AudioOutlined style={{ color: '#1677ff' }} />
          <span>Audio Settings</span>
        </Space>
      }
      open={isOpen}
      onCancel={onClose}
      width={600}
      footer={[
        <Button key="cancel" onClick={onClose}>Cancel</Button>,
        <Button key="apply" type="primary" icon={<CheckOutlined />} onClick={handleApply}>
          Apply
        </Button>,
      ]}
    >
      <Divider />
      <Form form={form} layout="vertical" initialValues={{ sampleRate: '48000', bufferSize: '1024' }}>

        {/* Audio API */}
        <Form.Item label="Audio API">
          <Select
            size="large"
            placeholder="All APIs"
            allowClear
            value={selectedHostType || undefined}
            onChange={(v) => handleHostTypeChange(v ?? '')}
          >
            {hostTypes.map(ht => (
              <Select.Option key={ht} value={ht}>
                <Space>
                  <Tag color={getHostTypeColor(ht)}>{ht}</Tag>
                  {isAsioHost(ht) && (
                    <Tag icon={<ThunderboltOutlined />} color="blue">Full-Duplex</Tag>
                  )}
                </Space>
              </Select.Option>
            ))}
          </Select>
        </Form.Item>

        {/* ── ASIO MODE: single full-duplex device ── */}
        {asioMode ? (
          <>
            <Alert
              type="info"
              showIcon
              icon={<ThunderboltOutlined />}
              style={{ marginBottom: 16 }}
              message="ASIO — Full-Duplex"
              description={
                <>
                  ASIO drivers manage input and output through a single device.
                  Select one device below; it will be used for both input and output.
                  The buffer size must match your ASIO driver's current setting
                  (configured in the driver's own control panel).
                </>
              }
            />

            <Form.Item
              label="ASIO Device"
              name="asioDevice"
              rules={[{ required: true, message: 'Please select an ASIO device' }]}
            >
              <Select
                size="large"
                placeholder="Select ASIO device"
                optionLabelProp="label"
              >
                {asioDevices.map(device => (
                  <Select.Option key={device.id} value={device.id} label={device.name}>
                    <DeviceOption device={device} />
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          </>
        ) : (
          /* ── NON-ASIO MODE: separate output / input ── */
          <>
            <Form.Item
              label="Output Device"
              name="outputDevice"
              rules={[{ required: true, message: 'Please select an output device' }]}
            >
              <Select size="large" placeholder="Select output device" optionLabelProp="label">
                {outputDevices.map(device => (
                  <Select.Option key={device.id} value={device.id} label={device.name}>
                    <DeviceOption device={device} />
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>

            <Form.Item label="Input Device" name="inputDevice">
              <Select size="large" placeholder="None (No Input)" allowClear optionLabelProp="label">
                <Select.Option value="" label="None (No Input)">
                  <span>None (No Input)</span>
                </Select.Option>
                {inputDevices.map(device => (
                  <Select.Option key={device.id} value={device.id} label={device.name}>
                    <DeviceOption device={device} />
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          </>
        )}

        {/* Sample Rate */}
        <Form.Item label="Sample Rate" name="sampleRate">
          <Select size="large">
            <Select.Option value="44100">44.1 kHz</Select.Option>
            <Select.Option value="48000">48 kHz</Select.Option>
            <Select.Option value="88200">88.2 kHz</Select.Option>
            <Select.Option value="96000">96 kHz</Select.Option>
            <Select.Option value="192000">192 kHz</Select.Option>
          </Select>
        </Form.Item>

        {/* Buffer Size */}
        <Form.Item
          label="Buffer Size"
          name="bufferSize"
          extra={
            asioMode
              ? 'Must match the buffer size set in your ASIO driver control panel'
              : 'Lower buffer size = lower latency but higher CPU usage'
          }
        >
          <Select size="large">
            <Select.Option value="64">64 samples (1.3 ms @ 48 kHz)</Select.Option>
            <Select.Option value="128">128 samples (2.7 ms @ 48 kHz)</Select.Option>
            <Select.Option value="256">256 samples (5.3 ms @ 48 kHz)</Select.Option>
            <Select.Option value="512">512 samples (10.7 ms @ 48 kHz)</Select.Option>
            <Select.Option value="1024">1024 samples (21.3 ms @ 48 kHz)</Select.Option>
            <Select.Option value="2048">2048 samples (42.7 ms @ 48 kHz)</Select.Option>
          </Select>
        </Form.Item>

      </Form>
    </Modal>
  );
}

