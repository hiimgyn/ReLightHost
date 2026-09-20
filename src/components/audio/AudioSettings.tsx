import { useEffect, useState } from "react";
import {
  Modal,
  Form,
  Select,
  Button,
  Tag,
  Space,
  message,
  Typography,
  theme,
} from "antd";
import {
  Check,
  Sliders,
  Zap,
  RefreshCw,
} from 'lucide-react';
import { useAudioStore } from "../../stores/audioStore";
import { getHostTypeColor, isAsioHost, isAsioId } from "./audioDeviceDisplay";
import { useAudioDeviceLists } from "./useAudioDeviceLists";
import AsioDeviceFields from "./AsioDeviceFields";
import StandardDeviceFields from "./StandardDeviceFields";
import { useTranslation } from "../../i18n";

interface AudioSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

const { Text } = Typography;

export default function AudioSettings({ isOpen, onClose }: AudioSettingsProps) {
  const { token } = theme.useToken();
  const { t } = useTranslation();
  const {
    devices,
    selectedDevice,
    selectedInputDevice,
    selectedVirtualOutputDevice,
    sampleRate,
    bufferSize,
    inputChannelOffset,
    outputChannelOffset,
    setOutputDevice,
    setInputDevice,
    setVirtualOutputDevice,
    setInputChannelOffset,
    setOutputChannelOffset,
    setSampleRate,
    setBufferSize,
    toggleMonitoring,
    fetchDevices,
    fetchStatus,
  } = useAudioStore();
  const [form] = Form.useForm();
  const [selectedHostType, setSelectedHostType] = useState<string>("");
  const modalWidth = typeof window === 'undefined' ? 660 : 'clamp(520px, 65vw, 680px)';
  const handleClose = () => {
    setSelectedHostType("");
    onClose();
  };

  const {
    hostTypes,
    asioMode,
    asioDevices,
    outputDevices,
    inputDevices,
    monitorOutputDevices,
    defaultMonitorOutputId,
  } = useAudioDeviceLists({ devices, selectedHostType, selectedDevice, selectedInputDevice });

  useEffect(() => {
    if (!isOpen) {
      setSelectedHostType("");
      return;
    }

    // Recompute the host-type every time the modal opens so we do not keep a
    // stale ASIO/non-ASIO choice from a previous session.
    setSelectedHostType("");

    // Only enumerate devices when the list is empty (app start or explicit refresh).
    // Re-enumerating ASIO hosts while a stream is active kills the running driver.
    if (devices.length === 0) fetchDevices();

    // Auto-detect host type from the currently stored device (runs immediately if
    // the devices list is already populated; otherwise the effect below will catch it).
    const hostDeviceId = isAsioId(selectedInputDevice) ? selectedInputDevice : (selectedDevice ?? selectedInputDevice);
    if (hostDeviceId && devices.length > 0) {
      const found = devices.find((d) => d.id === hostDeviceId);
      if (found) {
        setSelectedHostType(found.host_type);
      }
    }

    const currentIsAsio = isAsioId(selectedInputDevice ?? selectedDevice);
    const storedMonitorOutput = !isAsioId(selectedVirtualOutputDevice)
      ? selectedVirtualOutputDevice
      : null;
    const monitorOutputToUse = storedMonitorOutput ?? defaultMonitorOutputId ?? undefined;
    form.setFieldsValue({
      asioDevice: currentIsAsio ? (selectedInputDevice ?? selectedDevice) : undefined,
      outputDevice: !currentIsAsio ? (selectedDevice ?? undefined) : undefined,
      inputDevice: !currentIsAsio ? selectedInputDevice || "" : "",
      virtualOutputDevice: currentIsAsio
        ? monitorOutputToUse
        : (selectedVirtualOutputDevice || ""),
      inputChannelOffset,
      outputChannelOffset,
      sampleRate: String(sampleRate),
      bufferSize: String(bufferSize),
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  // When devices finish loading (async) while the modal is already open,
  // re-detect the host type so the Audio API dropdown shows the correct entry.
  useEffect(() => {
    const hostDeviceId = isAsioId(selectedInputDevice) ? selectedInputDevice : (selectedDevice ?? selectedInputDevice);
    if (!isOpen || !devices.length) return;
    if (hostDeviceId && !selectedHostType) {
      const found = devices.find((d) => d.id === hostDeviceId);
      if (found) {
        setSelectedHostType(found.host_type);
      }
    }
    if (asioMode) {
      const currentMonitorOutput = form.getFieldValue("virtualOutputDevice");
      if (!currentMonitorOutput && defaultMonitorOutputId) {
        form.setFieldsValue({ virtualOutputDevice: defaultMonitorOutputId });
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [devices]);

  const handleHostTypeChange = (ht: string) => {
    setSelectedHostType(ht);
    // Clear device selections so user picks from the new API's list
    form.setFieldsValue({
      asioDevice: undefined,
      outputDevice: undefined,
      inputDevice: "",
      virtualOutputDevice: isAsioHost(ht) ? (defaultMonitorOutputId ?? undefined) : "",
      inputChannelOffset: 0,
      outputChannelOffset: 0,
    });
  };

  const handleApply = async () => {
    try {
      const values = await form.validateFields();

      // Stop the always-running stream before changing config, then restart it.
      await toggleMonitoring(false);

      if (asioMode) {
        // ASIO is full-duplex: one device ID handles both I/O
        if (values.asioDevice) {
          const monitorOutputId =
            values.virtualOutputDevice || defaultMonitorOutputId || null;
          await setInputDevice(values.asioDevice);
          await setOutputDevice(values.asioDevice);
          await setVirtualOutputDevice(monitorOutputId);
          await setInputChannelOffset(values.inputChannelOffset ?? 0);
          await setOutputChannelOffset(values.outputChannelOffset ?? 0);
        }
      } else {
        const outputToSet = values.outputDevice || null;
        const virtualToSet = values.virtualOutputDevice || null;

        // Always call setOutputDevice so clearing the field sets a null output.
        await setOutputDevice(outputToSet);
        await setInputDevice(values.inputDevice || null);
        await setVirtualOutputDevice(virtualToSet || null);
        // WASAPI devices are stereo — no channel picker shown, always channel 1-2.
        await setInputChannelOffset(0);
        await setOutputChannelOffset(0);
      }

      await setSampleRate(parseInt(values.sampleRate));
      await setBufferSize(parseInt(values.bufferSize));

      // Always restart the stream after config change.
      await toggleMonitoring(true);
      await fetchStatus();
      message.success(t('audioSettings.appliedSuccess'));
      localStorage.setItem("audioConfigured", "true");
      onClose();
    } catch (error) {
      console.error("Failed to apply settings:", error);
      message.error(t('audioSettings.appliedFailed'));
    }
  };

  return (
    <Modal
      title={
        <Space size={12} align="center">
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 10,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: `${token.colorPrimary}1c`,
              border: `1px solid ${token.colorPrimary}38`,
            }}
          >
            <Sliders size={18} style={{ color: token.colorPrimary }} />
          </div>
          <div>
            <Text strong style={{ fontSize: 16, display: 'block', lineHeight: 1.2 }}>
              {t('audioSettings.title')}
            </Text>
            <Text type="secondary" style={{ fontSize: 11.5 }}>
              {t('audioSettings.subtitle')}
            </Text>
          </div>
        </Space>
      }
      open={isOpen}
      onCancel={handleClose}
      width={modalWidth}
      style={{ top: 28, maxWidth: 680 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 180px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '16px 22px 22px',
        },
      }}
      footer={[
        <Button key="cancel" onClick={handleClose} style={{ minWidth: 70, borderRadius: 8 }}>
          {t('audioSettings.cancel')}
        </Button>,
        <Button key="refresh" icon={<RefreshCw size={14} />} onClick={fetchDevices} style={{ borderRadius: 8 }}>
          {t('audioSettings.refreshDevices')}
        </Button>,
        <Button
          key="apply"
          type="primary"
          icon={<Check size={14} strokeWidth={2.2} />}
          onClick={handleApply}
          style={{ borderRadius: 8 }}
        >
          {t('audioSettings.apply')}
        </Button>,
      ]}
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={{ sampleRate: "48000", bufferSize: "1024", inputChannelOffset: 0, outputChannelOffset: 0 }}
      >
        {/* Audio API */}
        <Form.Item label={t('audioSettings.audioApi')}>
          <Select
            size="large"
            placeholder={t('audioSettings.allApis')}
            allowClear
            value={selectedHostType || undefined}
            onChange={(v) => handleHostTypeChange(v ?? "")}
          >
            {hostTypes.map((ht) => (
              <Select.Option key={ht} value={ht}>
                <Space>
                  <Tag color={getHostTypeColor(ht)}>{ht}</Tag>
                  {isAsioHost(ht) && (
                    <Tag icon={<Zap size={11} style={{ display: 'inline-block', verticalAlign: '-1px' }} />} color="blue">
                      {t('audioSettings.fullDuplex')}
                    </Tag>
                  )}
                </Space>
              </Select.Option>
            ))}
          </Select>
        </Form.Item>

        {asioMode ? (
          <AsioDeviceFields asioDevices={asioDevices} monitorOutputDevices={monitorOutputDevices} />
        ) : (
          <StandardDeviceFields inputDevices={inputDevices} outputDevices={outputDevices} />
        )}

        {/* Sample Rate & Buffer Size Grid */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 16 }}>
          {/* Sample Rate */}
          <Form.Item label={t('audioSettings.sampleRate')} name="sampleRate">
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
            label={t('audioSettings.bufferSize')}
            name="bufferSize"
            extra={
              asioMode
                ? t('audioSettings.bufferHintAsio')
                : t('audioSettings.bufferHintStandard')
            }
          >
            <Select size="large">
              <Select.Option value="64">64 smp (1.3 ms)</Select.Option>
              <Select.Option value="128">128 smp (2.7 ms)</Select.Option>
              <Select.Option value="256">256 smp (5.3 ms)</Select.Option>
              <Select.Option value="512">512 smp (10.7 ms)</Select.Option>
              <Select.Option value="1024">1024 smp (21.3 ms)</Select.Option>
              <Select.Option value="2048">2048 smp (42.7 ms)</Select.Option>
            </Select>
          </Form.Item>
        </div>
      </Form>
    </Modal>
  );
}
