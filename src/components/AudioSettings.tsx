import { useEffect, useState } from 'react';
import { Modal, Form, Select, Button, Divider, Tag, Space, message } from 'antd';
import { CheckOutlined, AudioOutlined, SoundOutlined } from '@ant-design/icons';
import { useAudioStore } from '../stores/audioStore';
import * as tauri from '../lib/tauri';

interface AudioSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function AudioSettings({ isOpen, onClose }: AudioSettingsProps) {
  const { devices, selectedDevice, selectedInputDevice, sampleRate, bufferSize, status, setOutputDevice, setInputDevice, setSampleRate, setBufferSize, fetchDevices, fetchStatus } = useAudioStore();
  const [form] = Form.useForm();
  const [selectedHostType, setSelectedHostType] = useState<string>('');

  // Derive sorted unique host types from available devices
  const hostTypes = Array.from(new Set(devices.map(d => d.host_type))).sort();

  // Filter devices by selected host type
  const filteredDevices = selectedHostType
    ? devices.filter(d => d.host_type === selectedHostType)
    : devices;

  useEffect(() => {
    if (isOpen) {
      fetchDevices();
      form.setFieldsValue({
        outputDevice: selectedDevice,
        inputDevice: selectedInputDevice || '',
        sampleRate: String(sampleRate),
        bufferSize: String(bufferSize),
      });
    }
  }, [isOpen, selectedDevice, selectedInputDevice, sampleRate, bufferSize, fetchDevices, form]);

  // When host type changes, reset device selections if they no longer match
  const handleHostTypeChange = (ht: string) => {
    setSelectedHostType(ht);
    const filtered = ht ? devices.filter(d => d.host_type === ht) : devices;
    const currentOut = form.getFieldValue('outputDevice');
    const currentIn = form.getFieldValue('inputDevice');
    if (currentOut && !filtered.find(d => d.id === currentOut)) {
      form.setFieldValue('outputDevice', undefined);
    }
    if (currentIn && !filtered.find(d => d.id === currentIn)) {
      form.setFieldValue('inputDevice', '');
    }
  };

  const handleApply = async () => {
    try {
      const values = await form.validateFields();
      const tasks: Promise<void>[] = [];

      if (values.outputDevice) tasks.push(setOutputDevice(values.outputDevice));
      tasks.push(setInputDevice(values.inputDevice || null));
      tasks.push(setSampleRate(parseInt(values.sampleRate)));
      tasks.push(setBufferSize(parseInt(values.bufferSize)));

      await Promise.all(tasks);
      message.success('Audio settings applied');
      onClose();
    } catch (error) {
      console.error('Failed to apply settings:', error);
      message.error('Failed to apply audio settings');
    }
  };

  const getHostTypeColor = (hostType: string) => {
    if (hostType.includes('ASIO')) return 'blue';
    if (hostType.includes('WASAPI')) return 'green';
    if (hostType.includes('DirectSound')) return 'orange';
    if (hostType.includes('CoreAudio')) return 'purple';
    return 'default';
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
        <Button key="cancel" onClick={onClose}>
          Cancel
        </Button>,
        <Button
          key="apply"
          type="primary"
          icon={<CheckOutlined />}
          onClick={handleApply}
        >
          Apply
        </Button>,
      ]}
    >
      <Divider />
      <Form
        form={form}
        layout="vertical"
        initialValues={{
          outputDevice: selectedDevice,
          inputDevice: '',
          sampleRate: '48000',
          bufferSize: '512',
        }}
      >
        {/* Host Type (API) Selection */}
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
                <Tag color={getHostTypeColor(ht)}>{ht}</Tag>
              </Select.Option>
            ))}
          </Select>
        </Form.Item>

        {/* Output Device Selection */}
        <Form.Item
          label="Output Device"
          name="outputDevice"
          rules={[{ required: true, message: 'Please select an output device' }]}
        >
          <Select
            size="large"
            placeholder="Select output device"
            optionLabelProp="label"
          >
            {filteredDevices.filter(d => d.output_channels > 0).map((device) => (
              <Select.Option 
                key={device.id} 
                value={device.id}
                label={device.name}
              >
                <Space direction="vertical" size={0} style={{ width: '100%' }}>
                  <Space>
                    <span style={{ fontWeight: 600 }}>{device.name}</span>
                    {device.is_default && <Tag color="blue">Default</Tag>}
                  </Space>
                  <Space size={4}>
                    <Tag color={getHostTypeColor(device.host_type)}>{device.host_type}</Tag>
                    <Tag color="cyan">{device.output_channels} channels</Tag>
                  </Space>
                </Space>
              </Select.Option>
            ))}
          </Select>
        </Form.Item>

        {/* Input Device Selection */}
        <Form.Item
          label="Input Device"
          name="inputDevice"
        >
          <Select
            size="large"
            placeholder="None (No Input)"
            allowClear
            optionLabelProp="label"
          >
            <Select.Option value="" label="None (No Input)">
              <span>None (No Input)</span>
            </Select.Option>
            {filteredDevices.filter(d => d.input_channels > 0).map((device) => (
              <Select.Option 
                key={device.id} 
                value={device.id}
                label={device.name}
              >
                <Space direction="vertical" size={0} style={{ width: '100%' }}>
                  <Space>
                    <span style={{ fontWeight: 600 }}>{device.name}</span>
                    {device.is_default && <Tag color="blue">Default</Tag>}
                  </Space>
                  <Space size={4}>
                    <Tag color={getHostTypeColor(device.host_type)}>{device.host_type}</Tag>
                    <Tag color="green">{device.input_channels} channels</Tag>
                  </Space>
                </Space>
              </Select.Option>
            ))}
          </Select>
        </Form.Item>

        {/* Sample Rate */}
        <Form.Item
          label="Sample Rate"
          name="sampleRate"
        >
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
          extra="Lower buffer size = lower latency but higher CPU usage"
        >
          <Select size="large">
            <Select.Option value="64">64 samples (1.3ms @ 48kHz)</Select.Option>
            <Select.Option value="128">128 samples (2.7ms @ 48kHz)</Select.Option>
            <Select.Option value="256">256 samples (5.3ms @ 48kHz)</Select.Option>
            <Select.Option value="512">512 samples (10.7ms @ 48kHz)</Select.Option>
            <Select.Option value="1024">1024 samples (21.3ms @ 48kHz)</Select.Option>
            <Select.Option value="2048">2048 samples (42.7ms @ 48kHz)</Select.Option>
          </Select>
        </Form.Item>

        {/* Test Audio */}
        <Divider orientationMargin={0} style={{ fontSize: 13 }}>Test Audio</Divider>
        <Space size="middle">
          <Button
            icon={<SoundOutlined />}
            onClick={async () => {
              try {
                await tauri.playTestSound();
                message.success('Playing test sound...');
              } catch {
                message.error('Failed to play test sound');
              }
            }}
          >
            Test Output
          </Button>
          <Button
            type={status.is_monitoring ? 'primary' : 'default'}
            icon={<AudioOutlined />}
            onClick={async () => {
              try {
                const next = !status.is_monitoring;
                await tauri.toggleMonitoring(next);
                await fetchStatus();
                message.info(next ? 'Input monitoring started' : 'Input monitoring stopped');
              } catch {
                message.error('Failed to toggle monitoring');
              }
            }}
          >
            {status.is_monitoring ? 'Monitoring...' : 'Hear Input'}
          </Button>
        </Space>
      </Form>
    </Modal>
  );
}

