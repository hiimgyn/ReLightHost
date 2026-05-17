import { useEffect, useState } from "react";
import {
  Modal,
  Form,
  Select,
  Button,
  Tag,
  Space,
  message,
  Alert,
  Typography,
  theme,
} from "antd";
import {
  CheckOutlined,
  AudioOutlined,
  ThunderboltOutlined,
  SyncOutlined,
} from "@ant-design/icons";
import { useAudioStore } from "../../stores/audioStore";
import type { AudioDeviceInfo } from "../../lib/types";

interface AudioSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

const { Text } = Typography;

const isAsioId = (id: string | null | undefined): boolean =>
  id?.startsWith("asio_") ?? false;
const isAsioHost = (ht: string): boolean => ht.toLowerCase().includes("asio");

function getHostTypeColor(hostType: string) {
  if (isAsioHost(hostType)) return "blue";
  if (hostType.includes("WASAPI")) return "green";
  if (hostType.includes("DirectSound")) return "orange";
  if (hostType.includes("CoreAudio")) return "purple";
  return "default";
}

function DeviceOption({ device }: { device: AudioDeviceInfo }) {
  const channelLabel =
    device.input_channels > 0 && device.output_channels > 0
      ? `${device.input_channels} in / ${device.output_channels} out`
      : device.output_channels > 0
        ? `${device.output_channels} ch out`
        : `${device.input_channels} ch in`;

  return (
    <Space orientation="vertical" size={0} style={{ width: "100%" }}>
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
  const { token } = theme.useToken();
  const {
    devices,
    selectedDevice,
    selectedInputDevice,
    selectedVirtualOutputDevice,
    sampleRate,
    bufferSize,
    setOutputDevice,
    setInputDevice,
    setVirtualOutputDevice,
    setSampleRate,
    setBufferSize,
    toggleMonitoring,
    fetchDevices,
    fetchStatus,
  } = useAudioStore();
  const [form] = Form.useForm();
  const [selectedHostType, setSelectedHostType] = useState<string>("");
  const modalWidth = typeof window === 'undefined' ? 544 : 'clamp(360px, 66vw, 544px)';
  const handleClose = () => {
    setSelectedHostType("");
    onClose();
  };

  const hostTypes = Array.from(new Set(devices.map((d) => d.host_type))).sort();
  // If no host type has been selected in the UI yet, infer from the stored
  // device ID — "asio_*" devices are always ASIO regardless of the dropdown.
  // This ensures the ASIO section is shown immediately on first render rather
  // than waiting for the async setSelectedHostType state update.
  const asioMode =
    isAsioHost(selectedHostType) ||
    (!selectedHostType && isAsioId(selectedInputDevice ?? selectedDevice));

  // Partition devices for the current host type
  const filteredDevices = selectedHostType
    ? devices.filter((d) => d.host_type === selectedHostType)
    : devices;

  // Full-duplex ASIO devices (both channels populated)
  const asioDevices = filteredDevices.filter(
    (d) => d.input_channels > 0 && d.output_channels > 0,
  );
  // Separate output / input lists for non-ASIO
  const outputDevices = filteredDevices.filter(
    (d) => d.output_channels > 0 && d.input_channels === 0,
  );
  const inputDevices = filteredDevices.filter(
    (d) => d.input_channels > 0 && d.output_channels === 0,
  );
  const monitorOutputDevices = devices.filter(
    (d) => d.output_channels > 0 && d.input_channels === 0 && !isAsioHost(d.host_type),
  );
  const defaultMonitorOutputId = monitorOutputDevices.find((d) => d.is_default)?.id;

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
    const hostDeviceId = isAsioId(selectedInputDevice) ? selectedInputDevice : selectedDevice;
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
      sampleRate: String(sampleRate),
      bufferSize: String(bufferSize),
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  // When devices finish loading (async) while the modal is already open,
  // re-detect the host type so the Audio API dropdown shows the correct entry.
  useEffect(() => {
    const hostDeviceId = isAsioId(selectedInputDevice) ? selectedInputDevice : selectedDevice;
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
    });
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
          const monitorOutputId =
            values.virtualOutputDevice || defaultMonitorOutputId || null;
          tasks.push(setInputDevice(values.asioDevice));
          tasks.push(setOutputDevice(values.asioDevice));
          tasks.push(setVirtualOutputDevice(monitorOutputId));
        }
      } else {
        const outputToSet = values.outputDevice || null;
        const virtualToSet = values.virtualOutputDevice || null;

        // Always call setOutputDevice so clearing the field sets a null output.
        tasks.push(setOutputDevice(outputToSet));
        tasks.push(setInputDevice(values.inputDevice || null));
        tasks.push(setVirtualOutputDevice(virtualToSet || null));
      }

      tasks.push(setSampleRate(parseInt(values.sampleRate)));
      tasks.push(setBufferSize(parseInt(values.bufferSize)));

      await Promise.all(tasks);

      // Always restart the stream after config change.
      await toggleMonitoring(true);
      await fetchStatus();
      message.success("Audio settings applied — stream restarted");
      localStorage.setItem("audioConfigured", "true");
      onClose();
    } catch (error) {
      console.error("Failed to apply settings:", error);
      message.error("Failed to apply audio settings");
    }
  };

  return (
    <Modal
      title={
        <Space>
          <AudioOutlined style={{ color: token.colorPrimary }} />
          <Text strong style={{ fontSize: 15, letterSpacing: '-0.01em' }}>Audio Settings</Text>
        </Space>
      }
      open={isOpen}
      onCancel={handleClose}
      width={modalWidth}
      style={{ top: 12, maxWidth: 544 }}
      styles={{
        body: {
          maxHeight: 'calc(100vh - 220px)',
          overflowY: 'auto',
          overflowX: 'hidden',
          padding: '12px 16px 16px',
        },
      }}
      footer={[
        <Button key="cancel" onClick={handleClose}>
          Cancel
        </Button>,
        <Button key="refresh" icon={<SyncOutlined />} onClick={fetchDevices}>
          Refresh Devices
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
   
      <Form
        form={form}
        layout="vertical"
        initialValues={{ sampleRate: "48000", bufferSize: "1024" }}
      >
        {/* Audio API */}
        <Form.Item label="Audio API">
          <Select
            size="large"
            placeholder="All APIs"
            allowClear
            value={selectedHostType || undefined}
            onChange={(v) => handleHostTypeChange(v ?? "")}
          >
            {hostTypes.map((ht) => (
              <Select.Option key={ht} value={ht}>
                <Space>
                  <Tag color={getHostTypeColor(ht)}>{ht}</Tag>
                  {isAsioHost(ht) && (
                    <Tag icon={<ThunderboltOutlined />} color="blue">
                      Full-Duplex
                    </Tag>
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
              rules={[
                { required: true, message: "Please select an ASIO device" },
              ]}
            >
              <Select
                size="large"
                placeholder="Select ASIO device"
                optionLabelProp="label"
              >
                {asioDevices.map((device) => (
                  <Select.Option
                    key={device.id}
                    value={device.id}
                    label={device.name}
                  >
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
              <Select
                size="large"
                placeholder="None (disabled)"
                allowClear
                optionLabelProp="label"
              >
                <Select.Option value="" label="None (disabled)">
                  <span>None (disabled)</span>
                </Select.Option>
                {monitorOutputDevices.map((device) => (
                  <Select.Option
                    key={device.id}
                    value={device.id}
                    label={device.name}
                  >
                    <DeviceOption device={device} />
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          </>
        ) : (
          /* ── NON-ASIO MODE: separate output / input ── */
          <>
            <Form.Item label="Input Device" name="inputDevice">
              <Select
                size="large"
                placeholder="None (No Input)"
                allowClear
                optionLabelProp="label"
              >
                <Select.Option value="" label="None (No Input)">
                  <span>None (No Input)</span>
                </Select.Option>
                {inputDevices.map((device) => (
                  <Select.Option
                    key={device.id}
                    value={device.id}
                    label={device.name}
                  >
                    <DeviceOption device={device} />
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
            
            <Form.Item label="Virtual Output" name="virtualOutputDevice">
              <Select
                size="large"
                placeholder="None (disabled)"
                allowClear
                optionLabelProp="label"
              >
                <Select.Option value="" label="None (disabled)">
                  <span>None (disabled)</span>
                </Select.Option>
                {outputDevices.map((device) => (
                  <Select.Option
                    key={device.id}
                    value={device.id}
                    label={device.name}
                  >
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
              <Select
                size="large"
                placeholder="None (disabled)"
                allowClear
                optionLabelProp="label"
              >
                <Select.Option value="" label="None (disabled)">
                  <span>None (disabled)</span>
                </Select.Option>
                {outputDevices.map((device) => (
                  <Select.Option
                    key={device.id}
                    value={device.id}
                    label={device.name}
                  >
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
              ? "Must match the buffer size set in your ASIO driver control panel"
              : "Lower buffer size = lower latency but higher CPU usage"
          }
        >
          <Select size="large">
            <Select.Option value="64">
              64 samples (1.3 ms @ 48 kHz)
            </Select.Option>
            <Select.Option value="128">
              128 samples (2.7 ms @ 48 kHz)
            </Select.Option>
            <Select.Option value="256">
              256 samples (5.3 ms @ 48 kHz)
            </Select.Option>
            <Select.Option value="512">
              512 samples (10.7 ms @ 48 kHz)
            </Select.Option>
            <Select.Option value="1024">
              1024 samples (21.3 ms @ 48 kHz)
            </Select.Option>
            <Select.Option value="2048">
              2048 samples (42.7 ms @ 48 kHz)
            </Select.Option>
          </Select>
        </Form.Item>
      </Form>
    </Modal>
  );
}
