import { Alert, Form, Select } from 'antd';
import { Zap } from 'lucide-react';
import type { AudioDeviceInfo } from '../../lib/types';
import { DeviceOption } from './audioDeviceDisplay';
import { useTranslation } from '../../i18n';

interface AsioDeviceFieldsProps {
  asioDevices: AudioDeviceInfo[];
  monitorOutputDevices: AudioDeviceInfo[];
}

/** "Ch 1-2", "Ch 3-4", ... one option per stereo pair the device exposes. */
function ChannelPairOptions({ channelCount }: { channelCount: number }) {
  const pairs = Math.max(1, Math.floor(channelCount / 2));
  return (
    <>
      {Array.from({ length: pairs }, (_, i) => {
        const offset = i * 2;
        return (
          <Select.Option key={offset} value={offset}>
            {`Ch ${offset + 1}-${offset + 2}`}
          </Select.Option>
        );
      })}
    </>
  );
}

/** ASIO mode: a single full-duplex device handles both input and output. */
export default function AsioDeviceFields({ asioDevices, monitorOutputDevices }: AsioDeviceFieldsProps) {
  const { t } = useTranslation();
  const form = Form.useFormInstance();
  const selectedDeviceId: string | undefined = Form.useWatch('asioDevice', form);
  const selectedDevice = asioDevices.find((d) => d.id === selectedDeviceId);
  // Only surface channel-pair pickers for interfaces that actually have more
  // than one stereo pair — a plain 2-in/2-out ASIO device has nothing to pick.
  const showInputChannelPicker = (selectedDevice?.input_channels ?? 0) > 2;
  const showOutputChannelPicker = (selectedDevice?.output_channels ?? 0) > 2;

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
        <Select
          size="large"
          placeholder={t('audioSettings.selectAsioDevice')}
          optionLabelProp="label"
          onChange={() => form.setFieldsValue({ inputChannelOffset: 0, outputChannelOffset: 0 })}
        >
          {asioDevices.map((device) => (
            <Select.Option key={device.id} value={device.id} label={device.name}>
              <DeviceOption device={device} />
            </Select.Option>
          ))}
        </Select>
      </Form.Item>

      {(showInputChannelPicker || showOutputChannelPicker) && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 16 }}>
          {showInputChannelPicker && (
            <Form.Item label={t('audioSettings.inputChannelLabel')} name="inputChannelOffset">
              <Select size="large">
                <ChannelPairOptions channelCount={selectedDevice?.input_channels ?? 2} />
              </Select>
            </Form.Item>
          )}
          {showOutputChannelPicker && (
            <Form.Item label={t('audioSettings.outputChannelLabel')} name="outputChannelOffset">
              <Select size="large">
                <ChannelPairOptions channelCount={selectedDevice?.output_channels ?? 2} />
              </Select>
            </Form.Item>
          )}
        </div>
      )}

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
