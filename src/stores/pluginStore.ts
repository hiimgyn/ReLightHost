import { create } from 'zustand';
import type { PluginInfo, PluginInstanceInfo } from '../lib/types';
import * as tauri from '../lib/tauri';

interface PluginStore {
  // State
  availablePlugins: PluginInfo[];
  pluginChain: PluginInstanceInfo[];
  isScanning: boolean;
  
  // Actions
  scanPlugins: () => Promise<void>;
  addToChain: (plugin: PluginInfo) => Promise<void>;
  removeFromChain: (instanceId: string) => Promise<void>;
  toggleBypass: (instanceId: string) => Promise<void>;
  fetchChain: () => Promise<void>;
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  availablePlugins: [],
  pluginChain: [],
  isScanning: false,

  scanPlugins: async () => {
    set({ isScanning: true });
    try {
      const plugins = await tauri.scanPlugins();
      set({ availablePlugins: plugins });
    } catch (error) {
      console.error('Failed to scan plugins:', error);
    } finally {
      set({ isScanning: false });
    }
  },

  addToChain: async (plugin: PluginInfo) => {
    try {
      await tauri.loadPlugin(plugin);
      await get().fetchChain();
      // Auto-save after adding plugin
      await tauri.autoSavePreset();
    } catch (error) {
      console.error('Failed to add plugin to chain:', error);
      throw error;
    }
  },

  removeFromChain: async (instanceId: string) => {
    try {
      await tauri.removePlugin(instanceId);
      await get().fetchChain();
      // Auto-save after removing plugin
      await tauri.autoSavePreset();
    } catch (error) {
      console.error('Failed to remove plugin from chain:', error);
      throw error;
    }
  },

  toggleBypass: async (instanceId: string) => {
    const instance = get().pluginChain.find(p => p.instance_id === instanceId);
    if (!instance) return;

    try {
      // Auto-save after toggling bypass
      await tauri.autoSavePreset();
      await tauri.setPluginBypass(instanceId, !instance.bypassed);
      await get().fetchChain();
    } catch (error) {
      console.error('Failed to toggle bypass:', error);
      throw error;
    }
  },

  fetchChain: async () => {
    try {
      const chain = await tauri.getPluginChain();
      set({ pluginChain: chain });
    } catch (error) {
      console.error('Failed to fetch plugin chain:', error);
    }
  },

}));
