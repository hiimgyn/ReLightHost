import { create } from 'zustand';
import type { AudioStatus, AudioDeviceInfo } from '../lib/types';
import * as tauri from '../lib/tauri';

interface AudioStore {
  // State
  status: AudioStatus;
  devices: AudioDeviceInfo[];
  selectedDevice: string | null;
  selectedInputDevice: string | null;
  selectedVirtualOutputDevice: string | null;
  sampleRate: number;
  bufferSize: number;
  isMuted: boolean;
  isLoopbackEnabled: boolean;
  
  // Actions
  fetchStatus: () => Promise<void>;
  fetchDevices: () => Promise<void>;
  /** Sync selectedDevice/Input/VirtualOutput/sampleRate/bufferSize from the backend config. */
  syncFromBackend: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  toggleMonitoring: (enabled: boolean) => Promise<void>;
  setOutputDevice: (deviceId: string) => Promise<void>;
  setInputDevice: (deviceId: string | null) => Promise<void>;
  setVirtualOutputDevice: (deviceId: string | null) => Promise<void>;
  setSampleRate: (rate: number) => Promise<void>;
  setBufferSize: (size: number) => Promise<void>;
  setMuted: (muted: boolean) => Promise<void>;
  setLoopback: (enabled: boolean) => Promise<void>;
}

export const useAudioStore = create<AudioStore>((set, get) => ({
  status: {
    is_monitoring: false,
    sample_rate: 48000,
    buffer_size: 1024,
    cpu_usage: 0,
    latency_ms: 0,
  },
  devices: [],
  selectedDevice: null,
  selectedInputDevice: null,
  selectedVirtualOutputDevice: null,
  sampleRate: 48000,
  bufferSize: 1024,
  isMuted: false,
  isLoopbackEnabled: false,

  fetchStatus: async () => {
    try {
      const status = await tauri.getAudioStatus();
      set({ status });
    } catch (error) {
      console.error('Failed to fetch audio status:', error);
    }
  },

  fetchDevices: async () => {
    try {
      const devices = await tauri.listAudioDevices();
      set({ devices });
      
      // Set default device if not selected
      const defaultDevice = devices.find(d => d.is_default);
      if (defaultDevice && !get().selectedDevice) {
        set({ selectedDevice: defaultDevice.id });
      }
    } catch (error) {
      console.error('Failed to fetch audio devices:', error);
    }
  },

  syncFromBackend: async () => {
    try {
      const config = await tauri.getAudioConfig();
      set({
        selectedDevice:              config.output_device_id ?? null,
        selectedInputDevice:         config.input_device_id ?? null,
        selectedVirtualOutputDevice: config.virtual_output_device_id ?? null,
        sampleRate:                  config.sample_rate,
        bufferSize:                  config.buffer_size,
      });
    } catch (error) {
      console.error('Failed to sync audio config from backend:', error);
    }
  },

  start: async () => {
    try {
      await tauri.startAudio();
      await get().fetchStatus();
    } catch (error) {
      console.error('Failed to start audio:', error);
      throw error;
    }
  },

  stop: async () => {
    try {
      await tauri.stopAudio();
      await get().fetchStatus();
    } catch (error) {
      console.error('Failed to stop audio:', error);
      throw error;
    }
  },

  toggleMonitoring: async (enabled: boolean) => {
    try {
      await tauri.toggleMonitoring(enabled);
      set(state => ({ status: { ...state.status, is_monitoring: enabled } }));
    } catch (error) {
      console.error('Failed to toggle monitoring:', error);
      throw error;
    }
  },

  setOutputDevice: async (deviceId: string) => {
    set({ selectedDevice: deviceId });
    try {
      await tauri.setOutputDevice(deviceId);
    } catch (error) {
      console.error('Failed to set output device:', error);
      throw error;
    }
  },

  setInputDevice: async (deviceId: string | null) => {
    set({ selectedInputDevice: deviceId });
    try {
      await tauri.setInputDevice(deviceId);
    } catch (error) {
      console.error('Failed to set input device:', error);
      throw error;
    }
  },

  setVirtualOutputDevice: async (deviceId: string | null) => {
    set({ selectedVirtualOutputDevice: deviceId });
    try {
      await tauri.setVirtualOutputDevice(deviceId);
    } catch (error) {
      console.error('Failed to set virtual output device:', error);
      throw error;
    }
  },

  setSampleRate: async (rate: number) => {
    set({ sampleRate: rate });
    try {
      await tauri.setSampleRate(rate);
      await get().fetchStatus();
    } catch (error) {
      console.error('Failed to set sample rate:', error);
      throw error;
    }
  },

  setBufferSize: async (size: number) => {
    set({ bufferSize: size });
    try {
      await tauri.setBufferSize(size);
      await get().fetchStatus();
    } catch (error) {
      console.error('Failed to set buffer size:', error);
      throw error;
    }
  },

  setMuted: async (muted: boolean) => {
    set({ isMuted: muted });
    try {
      await tauri.setMuted(muted);
    } catch (error) {
      console.error('Failed to set mute:', error);
      set({ isMuted: !muted }); // revert on error
      throw error;
    }
  },

  setLoopback: async (enabled: boolean) => {
    set({ isLoopbackEnabled: enabled });
    try {
      await tauri.setLoopback(enabled);
    } catch (error) {
      console.error('Failed to set loopback:', error);
      set({ isLoopbackEnabled: !enabled }); // revert on error
      throw error;
    }
  },
}));
