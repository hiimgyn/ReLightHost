// Audio Types
export interface AudioStatus {
  is_monitoring: boolean;
  sample_rate: number;
  buffer_size: number;
  cpu_usage: number;
  latency_ms: number;
}

export interface AudioConfig {
  sample_rate: number;
  buffer_size: number;
  output_device_id: string | null;
  input_device_id: string | null;
  virtual_output_device_id: string | null;
}

export interface AudioDeviceInfo {
  id: string;
  name: string;
  is_default: boolean;
  input_channels: number;
  output_channels: number;
  host_type: string; // ASIO, WASAPI, DirectSound, CoreAudio, ALSA, JACK
}

// Plugin Types
export type PluginFormat = 'clap' | 'vst3' | 'vst' | 'builtin';

export interface PluginInfo {
  id: string;
  name: string;
  manufacture: string;
  version: string;
  path: string;
  format: PluginFormat;
  category: string;
}

export interface PluginParameter {
  id: number;
  name: string;
  value: number;
  min: number;
  max: number;
  default: number;
}

export interface PluginInstanceInfo {
  instance_id: string;
  plugin_id: string;
  name: string;
  manufacture: string;
  version: string;
  path: string;
  format: PluginFormat;
  category: string;
  bypassed: boolean;
  parameters: PluginParameter[];
  /** True while the native GUI window is open */
  gui_open: boolean;
}

export interface Preset {
  name: string;
  description: string;
  created_at: string;
  plugin_chain: PresetPlugin[];
}

export interface PresetPlugin {
  plugin_id: string;
  plugin_name: string;
  bypassed: boolean;
  parameters: PresetParameter[];
}

export interface PresetParameter {
  id: number;
  name: string;
  value: number;
}

export interface SystemStats {
  cpu_percent: number;
  ram_percent: number;
  ram_used_mb: number;
  ram_total_mb: number;
}

// Crash Protection Types
export type PluginStatus = 
  | { type: 'Ok' }
  | { type: 'Crashed'; data: string }
  | { type: 'Timeout'; data: { secs: number; nanos: number } }
  | { type: 'Error'; data: string };

// VU Meter Types
export interface VUChannel {
  peak: number;       // 0.0 - 1.0
  peak_hold: number;  // 0.0 - 1.0
  rms: number;        // 0.0 - 1.0
}

export interface VUData {
  left: VUChannel;
  right: VUChannel;
}
