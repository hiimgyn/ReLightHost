import { create } from 'zustand';
import type { AudioStatus, AudioDeviceInfo } from '../lib/types';
import * as tauri from '../lib/tauri';

interface AudioStore {
  // State
  status: AudioStatus;
  devices: AudioDeviceInfo[];
  selectedDevice: string | null;
  selectedInputDevice: string | null;
  sampleRate: number;
  bufferSize: number;
  
  // Actions
  fetchStatus: () => Promise<void>;
  fetchDevices: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  setDevice: (deviceId: string) => void; // legacy alias
  setOutputDevice: (deviceId: string) => Promise<void>;
  setInputDevice: (deviceId: string | null) => Promise<void>;
  setSampleRate: (rate: number) => Promise<void>;
  setBufferSize: (size: number) => Promise<void>;
}

export const useAudioStore = create<AudioStore>((set, get) => ({
  status: {
    is_monitoring: false,
    sample_rate: 48000,
    buffer_size: 512,
    cpu_usage: 0,
    latency_ms: 0,
  },
  devices: [],
  selectedDevice: null,
  selectedInputDevice: null,
  sampleRate: 48000,
  bufferSize: 512,

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

  setDevice: (deviceId: string) => {
    set({ selectedDevice: deviceId });
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

  setSampleRate: async (rate: number) => {
    set({ sampleRate: rate });
    try {
      await tauri.setSampleRate(rate);
    } catch (error) {
      console.error('Failed to set sample rate:', error);
      throw error;
    }
  },

  setBufferSize: async (size: number) => {
    set({ bufferSize: size });
    try {
      await tauri.setBufferSize(size);
    } catch (error) {
      console.error('Failed to set buffer size:', error);
      throw error;
    }
  },
}));
