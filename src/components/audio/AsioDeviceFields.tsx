import { Alert, Form, Select } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import type { AudioDeviceInfo } from '../../lib/types';
import { DeviceOption } from './audioDeviceDisplay';

interface AsioDeviceFieldsProps {
  asioDevices: AudioDeviceInfo[];
  monitorOutputDevices: AudioDeviceInfo[];
}

/** ASIO mode: a single full-duplex device handles both input and output. */
export default function AsioDeviceFields({ asioDevices, monitorOutputDevices }: AsioDeviceFieldsProps) {
  return (
    <>
      <Alert
        type="info"
        showIcon
        icon={<ThunderboltOutlined />}
        style={{ marginBottom: 16 }}
        title="ASIO — Full-Duplex"
        description={
          <>
            ASIO drivers manage input and output through a single device.
            Select one device below; it will be used for both input and
            output. The buffer size must match your ASIO driver's current
            setting (configured in the driver's own control panel).
          </>
        }
      />

      <Form.Item
        label="ASIO Device"
        name="asioDevice"
        rules={[{ required: true, message: 'Please select an ASIO device' }]}
      >
        <Select size="large" placeholder="Select ASIO device" optionLabelProp="label">
          {asioDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item
        label="Monitor Output"
        name="virtualOutputDevice"
        extra="WASAPI monitor output used with the Monitor Output toggle to check audio."
      >
        <Select size="large" placeholder="None (disabled)" allowClear optionLabelProp="label">
          <Select.Option value="" label="None (disabled)">
            <span>None (disabled)</span>
          </Select.Option>
          {monitorOutputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>
    </>
  );
}
