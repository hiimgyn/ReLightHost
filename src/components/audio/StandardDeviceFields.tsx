import { Form, Select } from 'antd';
import type { AudioDeviceInfo } from '../../lib/types';
import { DeviceOption } from './audioDeviceDisplay';

interface StandardDeviceFieldsProps {
  inputDevices: AudioDeviceInfo[];
  outputDevices: AudioDeviceInfo[];
}

/** Non-ASIO mode: separate input / virtual-output / monitor-output device pickers. */
export default function StandardDeviceFields({ inputDevices, outputDevices }: StandardDeviceFieldsProps) {
  return (
    <>
      <Form.Item label="Input Device" name="inputDevice">
        <Select size="large" placeholder="None (No Input)" allowClear optionLabelProp="label">
          <Select.Option value="" label="None (No Input)">
            <span>None (No Input)</span>
          </Select.Option>
          {inputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item label="Virtual Output" name="virtualOutputDevice">
        <Select size="large" placeholder="None (disabled)" allowClear optionLabelProp="label">
          <Select.Option value="" label="None (disabled)">
            <span>None (disabled)</span>
          </Select.Option>
          {outputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item
        label="Monitor Output"
        name="outputDevice"
        extra="Hardware monitoring device (speakers/headphones). Enabled when Monitor Output is ON."
      >
        <Select size="large" placeholder="None (disabled)" allowClear optionLabelProp="label">
          <Select.Option value="" label="None (disabled)">
            <span>None (disabled)</span>
          </Select.Option>
          {outputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>
    </>
  );
}
