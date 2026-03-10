import { create } from 'zustand';
import type { Preset } from '../lib/types';
import * as tauri from '../lib/tauri';
import { usePluginStore } from './pluginStore';

interface PresetStore {
  // State
  presets: string[];
  currentPreset: Preset | null;
  
  // Actions
  fetchPresets: () => Promise<void>;
  savePreset: (name: string) => Promise<void>;
  loadPreset: (name: string) => Promise<void>;
  deletePreset: (name: string) => Promise<void>;
}

export const usePresetStore = create<PresetStore>((set, get) => ({
  presets: [],
  currentPreset: null,

  fetchPresets: async () => {
    try {
      const presets = await tauri.listPresets();
      set({ presets });
    } catch (error) {
      console.error('Failed to fetch presets:', error);
    }
  },

  savePreset: async (name: string) => {
    try {
      await tauri.savePreset(name);
      await get().fetchPresets();
    } catch (error) {
      console.error('Failed to save preset:', error);
      throw error;
    }
  },

  loadPreset: async (name: string) => {
    try {
      await tauri.applyPreset(name);
      const preset = await tauri.loadPreset(name);
      set({ currentPreset: preset });
      // Refresh the plugin chain in UI after applying preset
      await usePluginStore.getState().fetchChain();
    } catch (error) {
      console.error('Failed to load preset:', error);
      throw error;
    }
  },

  deletePreset: async (name: string) => {
    try {
      await tauri.deletePreset(name);
      await get().fetchPresets();
    } catch (error) {
      console.error('Failed to delete preset:', error);
      throw error;
    }
  },
}));
