import { invoke } from '@tauri-apps/api/core';
import type { AudioStatus, AudioDeviceInfo, AudioConfig, PluginInfo, PluginInstanceInfo, Preset, SystemStats, PluginStatus, VUData } from './types';

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

export async function setOutputDevice(deviceId: string): Promise<void> {
  return invoke('set_output_device', { deviceId });
}

export async function setInputDevice(deviceId: string | null): Promise<void> {
  return invoke('set_input_device', { deviceId });
}

export async function setVirtualOutputDevice(deviceId: string | null): Promise<void> {
  return invoke('set_virtual_output_device', { deviceId });
}

export async function setSampleRate(sampleRate: number): Promise<void> {
  return invoke('set_sample_rate', { sampleRate });
}

export async function setBufferSize(bufferSize: number): Promise<void> {
  return invoke('set_buffer_size', { bufferSize });
}

export async function toggleMonitoring(enabled: boolean): Promise<void> {
  return invoke('toggle_monitoring', { enabled });
}

export async function setMuted(muted: boolean): Promise<void> {
  return invoke('set_muted', { muted });
}

export async function getVUData(): Promise<VUData> {
  return invoke('get_vu_data');
}

// Plugin Commands
export async function scanPlugins(): Promise<PluginInfo[]> {
  return invoke('scan_plugins');
}

export async function loadPlugin(pluginInfo: PluginInfo): Promise<string> {
  return invoke('load_plugin', { pluginInfo });
}

export async function removePlugin(instanceId: string): Promise<void> {
  return invoke('remove_plugin', { instanceId });
}

export async function getPluginChain(): Promise<PluginInstanceInfo[]> {
  return invoke('get_plugin_chain');
}

export async function setPluginBypass(instanceId: string, bypassed: boolean): Promise<void> {
  return invoke('set_plugin_bypass', { instanceId, bypassed });
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

export async function applyPreset(name: string): Promise<void> {
  return invoke('apply_preset', { name });
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

// Preset Commands
export async function savePreset(name: string): Promise<string> {
  return invoke('save_preset', { name });
}

export async function loadPreset(name: string): Promise<Preset> {
  return invoke('load_preset', { name });
}

export async function listPresets(): Promise<string[]> {
  return invoke('list_presets');
}

export async function deletePreset(name: string): Promise<void> {
  return invoke('delete_preset', { name });
}

export async function autoSavePreset(): Promise<void> {
  return invoke('auto_save_preset');
}

// Crash Protection Commands
export async function getPluginCrashStatus(instanceId: string): Promise<PluginStatus> {
  return invoke('get_plugin_crash_status', { instanceId });
}

export async function resetPluginCrashProtection(instanceId: string): Promise<void> {
  return invoke('reset_plugin_crash_protection', { instanceId });
}

export async function midiPanic(): Promise<void> {
  return invoke('midi_panic');
}

export async function getNoiseSuppressorVad(instanceId: string): Promise<number> {
  return invoke('get_noise_suppressor_vad', { instanceId });
}
