import { invoke } from '@tauri-apps/api/core';
import type {
  AudioStatus,
  AudioDeviceInfo,
  AudioConfig,
  PluginInfo,
  PluginInstanceInfo,
  SystemStats,
  PluginStatus,
  VUData,
  PluginCrashStatusItem,
  LaunchPluginsResult,
} from './types';

// Audio Commands
export async function startAudio(): Promise<void> {
  return invoke('start_audio');
}

export async function stopAudio(): Promise<void> {
  return invoke('stop_audio');
}

export async function getAudioStatus(): Promise<AudioStatus> {
  return invoke('get_audio_status');
}

export async function getAudioConfig(): Promise<AudioConfig> {
  return invoke('get_audio_config');
}

export async function listAudioDevices(): Promise<AudioDeviceInfo[]> {
  return invoke('list_audio_devices');
}

export async function setOutputDevice(deviceId: string | null): Promise<void> {
  return invoke('set_output_device', { deviceId });
}

export async function setInputDevice(deviceId: string | null): Promise<void> {
  return invoke('set_input_device', { deviceId });
}

export async function setVirtualOutputDevice(deviceId: string | null): Promise<void> {
  return invoke('set_virtual_output_device', { deviceId });
}

export async function setSampleRate(sampleRate: number): Promise<void> {
  return invoke('set_sample_rate', { rate: sampleRate });
}

export async function setBufferSize(bufferSize: number): Promise<void> {
  return invoke('set_buffer_size', { size: bufferSize });
}

export async function toggleMonitoring(enabled: boolean): Promise<void> {
  return invoke('toggle_monitoring', { enabled });
}

export async function setMuted(muted: boolean): Promise<void> {
  return invoke('set_muted', { muted });
}

export async function setLoopback(enabled: boolean): Promise<void> {
  return invoke('set_loopback', { enabled });
}

export async function getVUData(): Promise<VUData> {
  return invoke('get_vu_data');
}

// Plugin Commands
export async function scanPlugins(): Promise<PluginInfo[]> {
  return invoke('scan_plugins');
}

export async function loadPlugin(pluginInfo: PluginInfo): Promise<string> {
  return invoke('load_plugin', { info: pluginInfo });
}

export async function removePlugin(instanceId: string): Promise<void> {
  return invoke('remove_plugin', { instanceId });
}

export async function getPluginChain(): Promise<PluginInstanceInfo[]> {
  return invoke('get_plugin_chain');
}

export async function setPluginBypass(instanceId: string, bypass: boolean): Promise<void> {
  return invoke('set_plugin_bypass', { instanceId, bypass });
}

export async function setPluginParameter(instanceId: string, paramId: number, value: number): Promise<void> {
  return invoke('set_plugin_parameter', { instanceId, paramId, value });
}

export async function reorderPluginChain(fromIndex: number, toIndex: number): Promise<void> {
  return invoke('reorder_plugin_chain', { fromIndex, toIndex });
}

export async function renamePlugin(instanceId: string, newName: string): Promise<void> {
  return invoke('rename_plugin', { instanceId, newName });
}

export async function playTestSound(): Promise<void> {
  return invoke('play_test_sound');
}

export async function getSystemStats(): Promise<SystemStats> {
  return invoke('get_system_stats');
}

export async function launchPlugin(instanceId: string): Promise<void> {
  return invoke('launch_plugin', { instanceId });
}

/** Open many native GUIs. Omit IDs to open every VST/VST3/CLAP in the chain that is not already open. */
export async function launchPlugins(instanceIds?: string[] | null): Promise<LaunchPluginsResult> {
  return invoke('launch_plugins', { instanceIds: instanceIds ?? null });
}

/** Request close of many native plugin GUIs. Omit IDs to close every open GUI. */
export async function closePlugins(instanceIds?: string[] | null): Promise<LaunchPluginsResult> {
  return invoke('close_plugins', { instanceIds: instanceIds ?? null });
}

// Crash Protection Commands
export async function getPluginCrashStatus(instanceId: string): Promise<PluginStatus> {
  return invoke('get_plugin_crash_status', { instanceId });
}

export async function getPluginCrashStatuses(): Promise<PluginCrashStatusItem[]> {
  return invoke('get_plugin_crash_statuses');
}

export async function resetPluginCrashProtection(instanceId: string): Promise<void> {
  return invoke('reset_plugin_crash_protection', { instanceId });
}

export async function getNoiseSuppressorVad(instanceId: string): Promise<number> {
  return invoke('get_noise_suppressor_vad', { instanceId });
}

export async function getPluginParameters(instanceId: string): Promise<import('./types').PluginParameter[]> {
  return invoke('get_plugin_parameters', { instanceId });
}
