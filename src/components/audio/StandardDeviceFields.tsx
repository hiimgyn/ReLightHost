import { Form, Select } from 'antd';
import type { AudioDeviceInfo } from '../../lib/types';
import { DeviceOption } from './audioDeviceDisplay';
import { useTranslation } from '../../i18n';

interface StandardDeviceFieldsProps {
  inputDevices: AudioDeviceInfo[];
  outputDevices: AudioDeviceInfo[];
}

/** Non-ASIO mode: separate input / virtual-output / monitor-output device pickers. */
export default function StandardDeviceFields({ inputDevices, outputDevices }: StandardDeviceFieldsProps) {
  const { t } = useTranslation();

  return (
    <>
      <Form.Item label={t('audioSettings.standardInputLabel')} name="inputDevice">
        <Select size="large" placeholder={t('audioSettings.noneNoInput')} allowClear optionLabelProp="label">
          <Select.Option value="" label={t('audioSettings.noneNoInput')}>
            <span>{t('audioSettings.noneNoInput')}</span>
          </Select.Option>
          {inputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item label={t('audioSettings.standardVirtualOutputLabel')} name="virtualOutputDevice">
        <Select size="large" placeholder={t('audioSettings.noneDisabled')} allowClear optionLabelProp="label">
          <Select.Option value="" label={t('audioSettings.noneDisabled')}>
            <span>{t('audioSettings.noneDisabled')}</span>
          </Select.Option>
          {outputDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item
        label={t('audioSettings.standardOutputLabel')}
        name="outputDevice"
        extra="Hardware monitoring device (speakers/headphones). Enabled when Monitor Output is ON."
      >
        <Select size="large" placeholder={t('audioSettings.noneDisabled')} allowClear optionLabelProp="label">
          <Select.Option value="" label={t('audioSettings.noneDisabled')}>
            <span>{t('audioSettings.noneDisabled')}</span>
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
