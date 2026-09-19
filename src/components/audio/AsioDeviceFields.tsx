import { Alert, Form, Select } from 'antd';
import { Zap } from 'lucide-react';
import type { AudioDeviceInfo } from '../../lib/types';
import { DeviceOption } from './audioDeviceDisplay';
import { useTranslation } from '../../i18n';

interface AsioDeviceFieldsProps {
  asioDevices: AudioDeviceInfo[];
  monitorOutputDevices: AudioDeviceInfo[];
}

/** ASIO mode: a single full-duplex device handles both input and output. */
export default function AsioDeviceFields({ asioDevices, monitorOutputDevices }: AsioDeviceFieldsProps) {
  const { t } = useTranslation();

  return (
    <>
      <Alert
        type="info"
        showIcon
        icon={<Zap size={15} />}
        style={{ marginBottom: 16 }}
        title={t('audioSettings.asioAlertTitle')}
        description={t('audioSettings.asioAlertDesc')}
      />

      <Form.Item
        label={t('audioSettings.asioDeviceLabel')}
        name="asioDevice"
        rules={[{ required: true, message: t('audioSettings.selectAsioDevice') }]}
      >
        <Select size="large" placeholder={t('audioSettings.selectAsioDevice')} optionLabelProp="label">
          {asioDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      <Form.Item
        label={t('audioSettings.asioMonitorLabel')}
        name="virtualOutputDevice"
        extra="WASAPI monitor output used with the Monitor Output toggle to check audio."
      >
        <Select size="large" placeholder={t('audioSettings.noneDisabled')} allowClear optionLabelProp="label">
          <Select.Option value="" label={t('audioSettings.noneDisabled')}>
            <span>{t('audioSettings.noneDisabled')}</span>
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
